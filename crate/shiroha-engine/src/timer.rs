//! 定时器轮
//!
//! 管理状态机转移超时。当 Job 进入某状态后，如果该状态的出边配置了 timeout，
//! Controller 通过此模块注册定时器。到期后通过 channel 发送 [`TimerEvent`]。
//!
//! 支持按 Job 粒度暂停/恢复定时器（配合 Job pause/resume）。

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;
use uuid::Uuid;

/// 定时器句柄，用于取消特定定时器
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TimerHandle(pub u64);

/// 定时器到期事件
#[derive(Debug, Clone)]
pub struct TimerEvent {
    pub job_id: Uuid,
    /// 到期后要注入 Job event inbox 的事件名
    pub event: String,
}

/// 内部定时器条目
struct TimerEntry {
    job_id: Uuid,
    event: String,
    /// 用于取消异步 sleep 任务
    abort_handle: JoinHandle<()>,
    registered_at: Instant,
    duration: Duration,
    paused: bool,
    /// 暂停时记录的剩余时间
    remaining: Option<Duration>,
}

/// 定时器轮
///
/// 每个定时器对应一个独立的 tokio sleep 任务。到期后通过 mpsc channel 发送事件。
/// 这种设计简单且够用于 Phase 1 的单机场景。
pub struct TimerWheel {
    next_id: AtomicU64,
    timers: Arc<Mutex<HashMap<u64, TimerEntry>>>,
    sender: mpsc::Sender<TimerEvent>,
}

impl TimerWheel {
    /// 创建定时器轮，返回 (轮, 事件接收端)
    pub fn new() -> (Self, mpsc::Receiver<TimerEvent>) {
        let (sender, receiver) = mpsc::channel(256);
        let wheel = Self {
            next_id: AtomicU64::new(1),
            timers: Arc::new(Mutex::new(HashMap::new())),
            sender,
        };
        (wheel, receiver)
    }

    /// 注册定时器，到期后发送 TimerEvent
    pub async fn register(&self, job_id: Uuid, event: String, duration: Duration) -> TimerHandle {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let sender = self.sender.clone();
        let timers = self.timers.clone();
        let evt = event.clone();

        // 每个定时器对应一个独立的异步任务
        let handle = tokio::spawn(async move {
            tokio::time::sleep(duration).await;
            let _ = sender.send(TimerEvent { job_id, event: evt }).await;
            timers.lock().await.remove(&id);
        });

        let entry = TimerEntry {
            job_id,
            event,
            abort_handle: handle,
            registered_at: Instant::now(),
            duration,
            paused: false,
            remaining: None,
        };

        self.timers.lock().await.insert(id, entry);

        TimerHandle(id)
    }

    /// 取消单个定时器
    pub async fn cancel(&self, handle: &TimerHandle) {
        if let Some(entry) = self.timers.lock().await.remove(&handle.0) {
            entry.abort_handle.abort();
        }
    }

    /// 暂停指定 Job 的所有定时器，记录剩余时间
    pub async fn pause_job_timers(&self, job_id: Uuid) {
        let mut timers = self.timers.lock().await;
        for entry in timers.values_mut() {
            if entry.job_id == job_id && !entry.paused {
                entry.abort_handle.abort();
                let elapsed = entry.registered_at.elapsed();
                // 暂停时把“原始持续时间 - 已流逝时间”记下来，恢复时继续倒计时。
                entry.remaining = Some(entry.duration.saturating_sub(elapsed));
                entry.paused = true;
            }
        }
    }

    /// 恢复指定 Job 的所有暂停中的定时器，用剩余时间重新注册
    pub async fn resume_job_timers(&self, job_id: Uuid) {
        let mut timers = self.timers.lock().await;
        let ids: Vec<u64> = timers
            .iter()
            .filter(|(_, e)| e.job_id == job_id && e.paused)
            .map(|(id, _)| *id)
            .collect();

        for id in ids {
            if let Some(entry) = timers.get_mut(&id) {
                let remaining = entry.remaining.unwrap_or(entry.duration);
                let sender = self.sender.clone();
                let evt = entry.event.clone();
                let jid = entry.job_id;
                let timer_map = self.timers.clone();

                let handle = tokio::spawn(async move {
                    tokio::time::sleep(remaining).await;
                    let _ = sender
                        .send(TimerEvent {
                            job_id: jid,
                            event: evt,
                        })
                        .await;
                    timer_map.lock().await.remove(&id);
                });

                entry.abort_handle = handle;
                entry.registered_at = Instant::now();
                // 恢复后新的“完整 duration”就是剩余时间，支持多次 pause/resume 叠加。
                entry.duration = remaining;
                entry.paused = false;
                entry.remaining = None;
            }
        }
    }

    /// 取消指定 Job 的所有定时器（Job 取消/完成时调用）
    pub async fn cancel_all_job_timers(&self, job_id: Uuid) {
        let mut timers = self.timers.lock().await;
        let ids: Vec<u64> = timers
            .iter()
            .filter(|(_, e)| e.job_id == job_id)
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            if let Some(entry) = timers.remove(&id) {
                entry.abort_handle.abort();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::time::{Duration, sleep, timeout};

    use super::*;

    #[tokio::test]
    async fn registered_timer_emits_event() {
        let (wheel, mut receiver) = TimerWheel::new();
        let job_id = Uuid::now_v7();

        wheel
            .register(job_id, "timeout".into(), Duration::from_millis(20))
            .await;

        let event = timeout(Duration::from_millis(200), receiver.recv())
            .await
            .expect("timer should fire")
            .expect("channel should yield event");

        assert_eq!(event.job_id, job_id);
        assert_eq!(event.event, "timeout");
    }

    #[tokio::test]
    async fn paused_timer_only_fires_after_resume() {
        let (wheel, mut receiver) = TimerWheel::new();
        let job_id = Uuid::now_v7();

        wheel
            .register(job_id, "timeout".into(), Duration::from_millis(80))
            .await;

        sleep(Duration::from_millis(30)).await;
        wheel.pause_job_timers(job_id).await;

        assert!(
            timeout(Duration::from_millis(100), receiver.recv())
                .await
                .is_err()
        );

        wheel.resume_job_timers(job_id).await;
        let event = timeout(Duration::from_millis(200), receiver.recv())
            .await
            .expect("resumed timer should fire")
            .expect("channel should yield event");

        assert_eq!(event.job_id, job_id);
        assert_eq!(event.event, "timeout");
    }

    #[tokio::test]
    async fn cancelling_job_timers_prevents_delivery() {
        let (wheel, mut receiver) = TimerWheel::new();
        let job_id = Uuid::now_v7();

        wheel
            .register(job_id, "timeout".into(), Duration::from_millis(40))
            .await;
        wheel.cancel_all_job_timers(job_id).await;

        assert!(
            timeout(Duration::from_millis(120), receiver.recv())
                .await
                .is_err()
        );
    }
}
