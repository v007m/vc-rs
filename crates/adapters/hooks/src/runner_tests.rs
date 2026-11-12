use std::collections::BTreeMap;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use vc_interfaces::{Budget, HookCall, HookError, HookRunner, Limit};
use vc_model::{HookModule, HookModules, HookRef};
use vc_testkit::hook_runner_suite;

use super::{QuickJsHooks, START};
use crate::dispatch::Settings;

/// `stall` spins past any budget the interrupt handler does not enforce, `forever` never
/// returns, `busy` spins for a second and answers, and `ok` answers at once.
const JS: &str = concat!(
    "export function stall() { const end = Date.now() + 1500; while (Date.now() < end) {} }\n",
    "export function forever() { for (;;) {} }\n",
    "export function busy() { const end = Date.now() + 1000; while (Date.now() < end) {} return 'busy'; }\n",
    "export function ok() { return 'ok'; }\n",
);

/// A call of `export` in [`JS`] with `time` to run.
fn call(export: &str, time: Duration) -> HookCall {
    let modules = HookModules::new(BTreeMap::from([(
        "hooks/spin.ts".to_owned(),
        HookModule { js: JS.to_owned(), lines: vec![1, 2, 3, 4] },
    )]));
    let hook = HookRef { file: "hooks/spin.ts".into(), export: export.into() };
    HookCall::new(modules, hook, Value::Null).with_budget(Budget::new(time, 64 << 20))
}

/// What a call answered.
async fn value(runner: &QuickJsHooks, export: &str, time: Duration) -> Result<Value, HookError> {
    runner.call(call(export, time)).await.map(|output| output.value)
}

/// A watchdog that waits 100 ms past the budget, over runtimes whose interrupts are off.
const WATCHDOG: Settings = Settings { grace: Duration::from_millis(100), start: START, interrupts: false };

#[tokio::test]
async fn quickjs_hooks_meet_the_hook_runner_contract() {
    hook_runner_suite(&mut || Box::new(QuickJsHooks::new()) as Box<dyn HookRunner>).await;
}

#[tokio::test]
async fn quickjs_hooks_on_two_threads_meet_the_hook_runner_contract() {
    hook_runner_suite(&mut || Box::new(QuickJsHooks::with_threads(2)) as Box<dyn HookRunner>).await;
}

/// A hook stuck where the interrupt handler cannot reach it is abandoned by the watchdog,
/// and the next call runs on a new thread.
#[tokio::test]
async fn the_watchdog_restarts_a_stuck_hook_thread() {
    let runner = QuickJsHooks::start(WATCHDOG, 1).0;
    let started = Instant::now();
    let stalled = value(&runner, "stall", Duration::from_millis(50)).await;
    assert_eq!(stalled, Err(HookError::Overran { limit: Limit::Time, line: None }));
    assert!(started.elapsed() < Duration::from_millis(1000), "the watchdog did not wait for the hook");
    assert_eq!(value(&runner, "ok", Duration::from_secs(5)).await, Ok(json!("ok")));
}

/// With two runtimes, a call never waits for one that is busy: the busy call never returns
/// within its 20 s, so the second call answers first only on the other runtime.
#[tokio::test]
async fn a_call_never_waits_behind_a_busy_runtime() {
    let runner = QuickJsHooks::with_threads(2);
    let held = value(&runner, "forever", Duration::from_secs(20));
    tokio::pin!(held);
    tokio::select! {
        biased;
        answer = &mut held => panic!("the busy call ended first: {answer:?}"),
        answer = value(&runner, "ok", Duration::from_secs(5)) => assert_eq!(answer, Ok(json!("ok"))),
    }
}

/// A runtime the watchdog abandons (V5116) leaves the call in flight on the other one to
/// finish, and is replaced: both runtimes serve again afterwards.
#[tokio::test]
async fn a_stuck_runtime_leaves_the_other_serving_and_is_replaced() {
    let runner = QuickJsHooks::start(WATCHDOG, 2).0;
    let (stalled, busy) = tokio::join!(
        value(&runner, "stall", Duration::from_millis(50)),
        value(&runner, "busy", Duration::from_secs(5)),
    );
    assert_eq!(stalled, Err(HookError::Overran { limit: Limit::Time, line: None }));
    assert_eq!(busy, Ok(json!("busy")));
    let both =
        tokio::join!(value(&runner, "busy", Duration::from_secs(5)), value(&runner, "ok", Duration::from_secs(5)));
    assert_eq!(both, (Ok(json!("busy")), Ok(json!("ok"))));
}

/// Dropping the runner closes its queue: the dispatcher closes every lane, each supervisor
/// retires its hook thread, and the dispatcher ends once they all have.
#[tokio::test]
async fn dropping_the_runner_ends_every_thread() {
    let (runner, dispatcher) = QuickJsHooks::start(WATCHDOG, 4);
    let dispatcher = dispatcher.expect("the dispatcher thread started");
    let answers = tokio::join!(
        value(&runner, "ok", Duration::from_secs(5)),
        value(&runner, "ok", Duration::from_secs(5)),
        value(&runner, "ok", Duration::from_secs(5)),
        value(&runner, "ok", Duration::from_secs(5)),
    );
    assert_eq!(answers, (Ok(json!("ok")), Ok(json!("ok")), Ok(json!("ok")), Ok(json!("ok"))));
    drop(runner);
    let (ended, end) = mpsc::channel();
    thread::spawn(move || ended.send(dispatcher.join().is_ok()));
    assert_eq!(end.recv_timeout(Duration::from_secs(10)), Ok(true), "a thread was still blocked");
}
