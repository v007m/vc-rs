use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::{mpsc, oneshot};

use crate::error::MockError;
use crate::state::State;

type Job = Box<dyn FnOnce(&mut State) + Send>;

/// Handle to the state actor: one task owns [`State`] and runs the jobs it receives in order.
#[derive(Clone)]
pub struct Store {
    jobs: mpsc::UnboundedSender<Job>,
    tickets: Arc<AtomicU64>,
}

impl Store {
    /// Spawns the actor on the current runtime. It stops when the last `Store` is dropped.
    pub fn spawn() -> Self {
        let (jobs, mut inbox) = mpsc::unbounded_channel::<Job>();
        tokio::spawn(async move {
            let mut state = State::default();
            while let Some(job) = inbox.recv().await {
                job(&mut state);
            }
        });
        Self { jobs, tickets: Arc::default() }
    }

    /// Runs `job` on the actor and returns its result.
    pub async fn call<R, F>(&self, job: F) -> Result<R, MockError>
    where
        R: Send + 'static,
        F: FnOnce(&mut State) -> R + Send + 'static,
    {
        let (reply, answer) = oneshot::channel();
        self.tell(move |state| {
            let _ = reply.send(job(state));
        });
        answer.await.map_err(|_| MockError::Stopped)
    }

    /// A number no other request on this server gets, never reused across resets.
    pub fn ticket(&self) -> u64 {
        self.tickets.fetch_add(1, Ordering::Relaxed)
    }

    /// Queues `job` without waiting; a stopped actor drops it.
    pub fn tell(&self, job: impl FnOnce(&mut State) + Send + 'static) {
        let _ = self.jobs.send(Box::new(job));
    }
}
