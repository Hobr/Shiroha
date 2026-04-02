use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TimerHandle(pub u64);

#[derive(Debug, Clone)]
pub struct TimerEvent {
    pub job_id: Uuid,
    pub event: String,
}

struct TimerEntry {
    job_id: Uuid,
    event: String,
    abort_handle: JoinHandle<()>,
    registered_at: Instant,
    duration: Duration,
    paused: bool,
    remaining: Option<Duration>,
}

pub struct TimerWheel {
    next_id: AtomicU64,
    timers: Arc<Mutex<HashMap<u64, TimerEntry>>>,
    sender: mpsc::Sender<TimerEvent>,
}

impl TimerWheel {
    pub fn new() -> (Self, mpsc::Receiver<TimerEvent>) {
        let (sender, receiver) = mpsc::channel(256);
        let wheel = Self {
            next_id: AtomicU64::new(1),
            timers: Arc::new(Mutex::new(HashMap::new())),
            sender,
        };
        (wheel, receiver)
    }

    pub fn register(&self, job_id: Uuid, event: String, duration: Duration) -> TimerHandle {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let sender = self.sender.clone();
        let timers = self.timers.clone();
        let evt = event.clone();

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

        // We can't block here, so spawn a task to insert
        let timers = self.timers.clone();
        tokio::spawn(async move {
            timers.lock().await.insert(id, entry);
        });

        TimerHandle(id)
    }

    pub async fn cancel(&self, handle: &TimerHandle) {
        if let Some(entry) = self.timers.lock().await.remove(&handle.0) {
            entry.abort_handle.abort();
        }
    }

    pub async fn pause_job_timers(&self, job_id: Uuid) {
        let mut timers = self.timers.lock().await;
        for entry in timers.values_mut() {
            if entry.job_id == job_id && !entry.paused {
                entry.abort_handle.abort();
                let elapsed = entry.registered_at.elapsed();
                entry.remaining = Some(entry.duration.saturating_sub(elapsed));
                entry.paused = true;
            }
        }
    }

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
                    let _ = sender.send(TimerEvent { job_id: jid, event: evt }).await;
                    timer_map.lock().await.remove(&id);
                });

                entry.abort_handle = handle;
                entry.registered_at = Instant::now();
                entry.duration = remaining;
                entry.paused = false;
                entry.remaining = None;
            }
        }
    }

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
