//! 定时器轮
//!
//! 管理状态机转移超时。当 Job 进入某状态后，如果该状态的出边配置了 timeout，
//! Controller 通过此模块注册定时器。到期后通过 channel 发送 [`TimerEvent`]。
//!
//! 支持按 Job 粒度暂停/恢复定时器（配合 Job pause/resume）。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;
use tokio_stream::StreamExt;
use tokio_util::time::{DelayQueue, delay_queue::Key};
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

/// 内部定时器状态
enum TimerState {
    Scheduled { key: Key },
    Paused { remaining: Duration },
}

/// 内部定时器条目
struct TimerEntry {
    job_id: Uuid,
    event: String,
    pause_with_job: bool,
    state: TimerState,
}

enum TimerCommand {
    Register {
        id: u64,
        job_id: Uuid,
        event: String,
        duration: Duration,
        pause_with_job: bool,
        ack: oneshot::Sender<()>,
    },
    Cancel {
        id: u64,
        ack: oneshot::Sender<()>,
    },
    PauseJob {
        job_id: Uuid,
        ack: oneshot::Sender<()>,
    },
    ResumeJob {
        job_id: Uuid,
        ack: oneshot::Sender<()>,
    },
    CancelAllJob {
        job_id: Uuid,
        ack: oneshot::Sender<()>,
    },
}

/// 定时器轮
///
/// 所有定时器由单个后台驱动器统一管理，内部使用 `DelayQueue`
/// 维护到期顺序，而不是为每个定时器单独创建一个 sleep 任务。
pub struct TimerWheel {
    next_id: AtomicU64,
    commands: mpsc::Sender<TimerCommand>,
}

fn remove_timer_entry(queue: &mut DelayQueue<u64>, timers: &mut HashMap<u64, TimerEntry>, id: u64) {
    if let Some(entry) = timers.remove(&id)
        && let TimerState::Scheduled { key } = entry.state
    {
        let _ = queue.try_remove(&key);
    }
}

fn pause_job_entries(
    queue: &mut DelayQueue<u64>,
    timers: &mut HashMap<u64, TimerEntry>,
    job_id: Uuid,
) {
    let now = Instant::now();
    for entry in timers.values_mut() {
        if entry.job_id != job_id || !entry.pause_with_job {
            continue;
        }

        let TimerState::Scheduled { key } = entry.state else {
            continue;
        };

        let remaining = queue.deadline(&key).saturating_duration_since(now);
        let _ = queue.try_remove(&key);
        entry.state = TimerState::Paused { remaining };
    }
}

fn resume_job_entries(
    queue: &mut DelayQueue<u64>,
    timers: &mut HashMap<u64, TimerEntry>,
    job_id: Uuid,
) {
    let ids = timers
        .iter()
        .filter(|(_, entry)| entry.job_id == job_id)
        .filter_map(|(id, entry)| matches!(entry.state, TimerState::Paused { .. }).then_some(*id))
        .collect::<Vec<_>>();

    for id in ids {
        if let Some(entry) = timers.get_mut(&id) {
            let TimerState::Paused { remaining } = entry.state else {
                continue;
            };
            let key = queue.insert(id, remaining);
            entry.state = TimerState::Scheduled { key };
        }
    }
}

fn spawn_timer_driver(
    mut commands: mpsc::Receiver<TimerCommand>,
    sender: mpsc::Sender<TimerEvent>,
) {
    tokio::spawn(async move {
        let mut queue = DelayQueue::new();
        let mut timers = HashMap::<u64, TimerEntry>::new();

        loop {
            tokio::select! {
                biased;

                Some(command) = commands.recv() => {
                    match command {
                        TimerCommand::Register {
                            id,
                            job_id,
                            event,
                            duration,
                            pause_with_job,
                            ack,
                        } => {
                            let key = queue.insert(id, duration);
                            timers.insert(id, TimerEntry {
                                job_id,
                                event,
                                pause_with_job,
                                state: TimerState::Scheduled { key },
                            });
                            let _ = ack.send(());
                        }
                        TimerCommand::Cancel { id, ack } => {
                            remove_timer_entry(&mut queue, &mut timers, id);
                            let _ = ack.send(());
                        }
                        TimerCommand::PauseJob { job_id, ack } => {
                            pause_job_entries(&mut queue, &mut timers, job_id);
                            let _ = ack.send(());
                        }
                        TimerCommand::ResumeJob { job_id, ack } => {
                            resume_job_entries(&mut queue, &mut timers, job_id);
                            let _ = ack.send(());
                        }
                        TimerCommand::CancelAllJob { job_id, ack } => {
                            let ids = timers
                                .iter()
                                .filter(|(_, entry)| entry.job_id == job_id)
                                .map(|(id, _)| *id)
                                .collect::<Vec<_>>();
                            for id in ids {
                                remove_timer_entry(&mut queue, &mut timers, id);
                            }
                            let _ = ack.send(());
                        }
                    }
                }
                Some(expired) = queue.next(), if !queue.is_empty() => {
                    let id = expired.into_inner();
                    if let Some(entry) = timers.remove(&id) {
                        let _ = sender
                            .send(TimerEvent {
                                job_id: entry.job_id,
                                event: entry.event,
                            })
                            .await;
                    }
                }
                else => break,
            }
        }
    });
}

impl TimerWheel {
    /// 创建定时器轮，返回 (轮, 事件接收端)
    pub fn new() -> (Self, mpsc::Receiver<TimerEvent>) {
        let (sender, receiver) = mpsc::channel(256);
        let (commands, command_rx) = mpsc::channel(256);
        spawn_timer_driver(command_rx, sender);
        let wheel = Self {
            next_id: AtomicU64::new(1),
            commands,
        };
        (wheel, receiver)
    }

    /// 注册定时器，到期后发送 TimerEvent
    pub async fn register(&self, job_id: Uuid, event: String, duration: Duration) -> TimerHandle {
        self.register_with_policy(job_id, event, duration, true)
            .await
    }

    /// 注册定时器，并指定它是否应在 `pause_job_timers()` 时被冻结。
    pub async fn register_with_policy(
        &self,
        job_id: Uuid,
        event: String,
        duration: Duration,
        pause_with_job: bool,
    ) -> TimerHandle {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (ack, wait) = oneshot::channel();
        self.commands
            .send(TimerCommand::Register {
                id,
                job_id,
                event,
                duration,
                pause_with_job,
                ack,
            })
            .await
            .expect("timer wheel command channel should stay alive");
        wait.await
            .expect("timer wheel driver should acknowledge register");

        TimerHandle(id)
    }

    /// 取消单个定时器
    pub async fn cancel(&self, handle: &TimerHandle) {
        let (ack, wait) = oneshot::channel();
        self.commands
            .send(TimerCommand::Cancel { id: handle.0, ack })
            .await
            .expect("timer wheel command channel should stay alive");
        wait.await
            .expect("timer wheel driver should acknowledge cancel");
    }

    /// 暂停指定 Job 的所有定时器，记录剩余时间
    pub async fn pause_job_timers(&self, job_id: Uuid) {
        let (ack, wait) = oneshot::channel();
        self.commands
            .send(TimerCommand::PauseJob { job_id, ack })
            .await
            .expect("timer wheel command channel should stay alive");
        wait.await
            .expect("timer wheel driver should acknowledge pause");
    }

    /// 恢复指定 Job 的所有暂停中的定时器，用剩余时间重新注册
    pub async fn resume_job_timers(&self, job_id: Uuid) {
        let (ack, wait) = oneshot::channel();
        self.commands
            .send(TimerCommand::ResumeJob { job_id, ack })
            .await
            .expect("timer wheel command channel should stay alive");
        wait.await
            .expect("timer wheel driver should acknowledge resume");
    }

    /// 取消指定 Job 的所有定时器（Job 取消/完成时调用）
    pub async fn cancel_all_job_timers(&self, job_id: Uuid) {
        let (ack, wait) = oneshot::channel();
        self.commands
            .send(TimerCommand::CancelAllJob { job_id, ack })
            .await
            .expect("timer wheel command channel should stay alive");
        wait.await
            .expect("timer wheel driver should acknowledge cancel-all");
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

    #[tokio::test]
    async fn cancelling_single_timer_prevents_delivery() {
        let (wheel, mut receiver) = TimerWheel::new();
        let job_id = Uuid::now_v7();

        let handle = wheel
            .register(job_id, "timeout".into(), Duration::from_millis(40))
            .await;
        wheel.cancel(&handle).await;

        assert!(
            timeout(Duration::from_millis(120), receiver.recv())
                .await
                .is_err()
        );
    }
}
