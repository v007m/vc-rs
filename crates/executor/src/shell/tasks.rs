use std::collections::HashMap;
use std::future::Future;

use tokio::task::{Id, JoinSet};

use super::crew::{Crew, Job};
use crate::cancel::{CancelTree, Ran};
use crate::{ActionReport, FileInput, RunInput};

/// The inputs a frame's workers report for shared actions and cleanups.
pub trait FrameInput: Send + 'static {
    /// A shared action ended (or was cut off).
    fn acted(ran: Ran<ActionReport>) -> Self;
    /// A cleanup ended.
    fn cleaned() -> Self;
}

/// Both frame inputs report workers the same way.
macro_rules! frame_input {
    ($($input:ty),+) => {$(
        impl FrameInput for $input {
            fn acted(ran: Ran<ActionReport>) -> Self {
                match ran {
                    Ran::Done(report) => Self::ActionDone(report),
                    Ran::Cut | Ran::Aborted => Self::ActionCut,
                }
            }

            fn cleaned() -> Self {
                Self::CleanupDone
            }
        }
    )+};
}

frame_input!(FileInput, RunInput);

/// A frame's workers: each reports one input when it ends. A worker that panics or is aborted
/// reports the fallback given when it was spawned, so the frame's machine always hears back.
/// Dropping the set aborts the workers still running.
pub struct Tasks<I> {
    set: JoinSet<I>,
    fallbacks: HashMap<Id, I>,
}

impl<I: Send + 'static> Tasks<I> {
    pub fn new() -> Self {
        Self { set: JoinSet::new(), fallbacks: HashMap::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }

    pub fn spawn(&mut self, work: impl Future<Output = I> + Send + 'static, fallback: I) {
        let id = self.set.spawn(work).id();
        self.fallbacks.insert(id, fallback);
    }

    /// The input of the next worker to end; `None` when none is running.
    pub async fn next(&mut self) -> Option<I> {
        let (id, input) = match self.set.join_next_with_id().await? {
            Ok((id, input)) => (id, Some(input)),
            Err(error) => (error.id(), None),
        };
        let fallback = self.fallbacks.remove(&id);
        input.or(fallback)
    }
}

impl<I: FrameInput> Tasks<I> {
    /// Runs a shared action of the frame.
    pub fn act(&mut self, crew: &Crew, job: Job, cancel: &CancelTree) {
        let (crew, cancel) = (crew.clone(), cancel.clone());
        self.spawn(async move { I::acted(crew.act(job, &cancel).await) }, I::acted(Ran::Cut));
    }

    /// Runs a cleanup of the frame.
    pub fn clean(&mut self, crew: &Crew, job: Job, cancel: &CancelTree) {
        let (crew, cancel) = (crew.clone(), cancel.clone());
        self.spawn(
            async move {
                crew.clean(job, &cancel).await;
                I::cleaned()
            },
            I::cleaned(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::Tasks;

    #[tokio::test]
    async fn a_worker_that_panics_reports_its_fallback() {
        let mut tasks = Tasks::new();
        assert!(tasks.is_empty());
        tasks.spawn(async { 1 }, 10);
        tasks.spawn(async { std::panic::panic_any("worker failed") }, 20);
        let mut inputs = vec![tasks.next().await, tasks.next().await];
        inputs.sort_unstable();
        assert_eq!(inputs, [Some(1), Some(20)]);
        assert_eq!(tasks.next().await, None);
    }
}
