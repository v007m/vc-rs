//! `repeat` and `forEach` on an action: each turn runs the action's executor in a frame with its
//! own `iteration`, up to `parallel` at a time, and the turns' reports gather into one.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use tokio::task::{Id, JoinSet};
use vc_diagnostics::Code;
use vc_model::{ActionOutcome, Failure, Loop};

use super::{ActionExecutor, ActionRequest, request};
use crate::{ActionReport, Frame, Iteration, Render, Saved};

/// Runs `request`'s action once per turn of `spec` with `executor`. Once a turn fails no more
/// start; the action fails when the turns in flight finish, keeping their cleanups.
pub async fn iterate(executor: Arc<dyn ActionExecutor>, request: ActionRequest, spec: &Loop) -> ActionReport {
    let turns = match turns(&request, spec) {
        Ok(turns) => turns,
        Err(failure) => return ActionReport::failed(failure.at(request.action)),
    };
    let mut running = Running::new(executor, turns.len());
    for (slot, turn) in turns.into_iter().enumerate() {
        if !running.make_room(width(spec)).await {
            break;
        }
        running.start(slot, ActionRequest { frame: turned(&request.frame, turn), ..request.clone() });
    }
    gather(running.finish().await)
}

/// How many turns may run at once.
fn width(spec: &Loop) -> usize {
    let (Loop::Repeat { parallel, .. } | Loop::ForEach { parallel, .. }) = spec;
    usize::try_from(*parallel).unwrap_or(usize::MAX).max(1)
}

/// The loop's turns, in order; a `forEach` renders its items first.
fn turns(request: &ActionRequest, spec: &Loop) -> Result<Vec<Iteration>, Failure> {
    let items: Vec<Option<Value>> = match spec {
        Loop::Repeat { times, .. } => vec![None; usize::try_from(*times).unwrap_or(usize::MAX)],
        Loop::ForEach { items, .. } => match Render::new(request).and_then(|render| render.value(items)) {
            Ok(Value::Array(items)) => items.into_iter().map(Some).collect(),
            Ok(other) => return Err(not_a_list(&other)),
            Err(error) => return Err(request::failed(error)),
        },
    };
    let count = items.len();
    Ok(items.into_iter().enumerate().map(|(at, item)| Iteration { index: at + 1, count, item }).collect())
}

fn not_a_list(value: &Value) -> Failure {
    Failure::new(format!("`forEach` needs a list, got {value}")).with_code(Code::V5111.to_string())
}

/// The frame a turn runs in: its `iteration`, and no fakes drawn yet, so each turn draws its own.
fn turned(frame: &Frame, turn: Iteration) -> Frame {
    Frame { iteration: Some(Arc::new(turn)), fakes: Arc::default(), ..frame.clone() }
}

/// The turns in flight, and each finished turn's report in its slot.
struct Running {
    executor: Arc<dyn ActionExecutor>,
    tasks: JoinSet<ActionReport>,
    slot_of: HashMap<Id, usize>,
    slots: Vec<Option<ActionReport>>,
    failed: bool,
}

impl Running {
    fn new(executor: Arc<dyn ActionExecutor>, turns: usize) -> Self {
        let slots = std::iter::repeat_with(|| None).take(turns).collect();
        Self { executor, tasks: JoinSet::new(), slot_of: HashMap::new(), slots, failed: false }
    }

    /// Waits until fewer than `width` turns run; false once a turn has failed.
    async fn make_room(&mut self, width: usize) -> bool {
        while self.tasks.len() >= width && !self.failed {
            self.settle().await;
        }
        !self.failed
    }

    fn start(&mut self, slot: usize, turn: ActionRequest) {
        let executor = Arc::clone(&self.executor);
        let task = self.tasks.spawn(async move { executor.execute(turn).await });
        self.slot_of.insert(task.id(), slot);
    }

    /// Waits for the next turn to finish and puts its report in its slot; a turn that panicked
    /// fails.
    async fn settle(&mut self) {
        let Some(joined) = self.tasks.join_next_with_id().await else { return };
        let (id, report) =
            joined.unwrap_or_else(|error| (error.id(), ActionReport::failed(Failure::new(error.to_string()))));
        self.failed |= matches!(report.outcome, ActionOutcome::Failed(_));
        if let Some(place) = self.slot_of.get(&id).and_then(|slot| self.slots.get_mut(*slot)) {
            *place = Some(report);
        }
    }

    /// Waits for the turns in flight; the slots, with an empty one for each turn not started.
    async fn finish(mut self) -> Vec<Option<ActionReport>> {
        while !self.tasks.is_empty() {
            self.settle().await;
        }
        self.slots
    }
}

/// One report from the turns', in turn order: every cleanup and check, the last response, the
/// first failure, and, when every turn passed, each saved name as a list with one entry per turn.
fn gather(slots: Vec<Option<ActionReport>>) -> ActionReport {
    let mut gathered = ActionReport::passed();
    let mut lists = Saved::new();
    for (at, report) in slots.into_iter().enumerate() {
        let Some(report) = report else { continue };
        gathered.cleanups.extend(report.cleanups);
        gathered.checks.extend(report.checks);
        gathered.exchanges.extend(report.exchanges);
        gathered.warnings.extend(report.warnings);
        gathered.response = report.response.or(gathered.response);
        for (name, value) in report.saved {
            if let Value::Array(list) = lists.entry(name).or_insert_with(|| Value::Array(Vec::new())) {
                list.push(value);
            }
        }
        if let (ActionOutcome::Failed(failure), ActionOutcome::Passed) = (report.outcome, &gathered.outcome) {
            let message = format!("iteration {}: {}", at + 1, failure.message);
            gathered.outcome = ActionOutcome::Failed(Failure { message, ..failure });
        }
    }
    if gathered.outcome == ActionOutcome::Passed {
        gathered.saved = lists;
    }
    gathered
}

#[cfg(test)]
#[path = "looping_tests.rs"]
mod tests;
