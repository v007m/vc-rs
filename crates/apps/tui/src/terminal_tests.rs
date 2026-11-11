use std::panic::{self, PanicHookInfo};
use std::sync::mpsc::{self as std_mpsc, Sender};

use tokio::sync::mpsc;

use super::{PanicRestore, QUEUE, drain, tell_held};
use crate::screen::Update;
use crate::{Tui, TuiOptions};

/// A process's panic hook.
type Hook = Box<dyn Fn(&PanicHookInfo<'_>) + Send + Sync>;

/// A panic hook that says `printed` for this file's own panics (`a panic …`), not for those of
/// tests running beside it.
fn printing(printed: Sender<&'static str>) -> Hook {
    Box::new(move |info| {
        if info.payload().downcast_ref::<&str>().is_some_and(|said| said.starts_with("a panic ")) {
            printed.send("printed").unwrap_or_default();
        }
    })
}

/// Each case replaces the process's panic hook, so they run one after another in one test.
#[test]
fn the_panic_hook_restores_the_terminal_and_holds_or_prints_each_panic() {
    a_panic_restores_the_terminal_before_the_hook_before_prints_it_and_the_hook_is_put_back();
    a_panic_on_another_thread_leaves_the_terminal_to_the_ui_and_is_held_to_be_told_after_it();
    once_a_panic_on_the_ui_thread_restored_the_terminal_a_panic_on_another_thread_is_printed();
}

#[test]
fn a_burst_of_updates_is_applied_before_the_next_frame_a_queue_at_a_time() {
    let mut tui = Tui::new(TuiOptions::default());
    let (sender, mut updates) = mpsc::channel(QUEUE * 2);
    for _ in 0..QUEUE + 2 {
        sender.try_send(Update::RunFailed("not reachable".to_owned())).unwrap();
    }
    drain(&mut tui, &mut updates);
    assert_eq!(updates.len(), 2, "a queue's worth applied, so a frame still comes between bursts");
    drain(&mut tui, &mut updates);
    assert!(updates.is_empty());
}

fn a_panic_restores_the_terminal_before_the_hook_before_prints_it_and_the_hook_is_put_back() {
    let original = panic::take_hook();
    let (said, heard) = std_mpsc::channel();
    let printed = said.clone();
    panic::set_hook(printing(printed));
    let hook = PanicRestore::install(move || said.send("restored").unwrap_or_default());
    assert!(panic::catch_unwind(|| panic!("a panic in the UI")).is_err());
    drop(hook);
    assert!(panic::catch_unwind(|| panic!("a panic after the UI")).is_err());
    panic::set_hook(original);
    let order: Vec<_> = heard.try_iter().collect();
    assert_eq!(order[..3], ["restored", "printed", "printed"], "{order:?}");
}

fn a_panic_on_another_thread_leaves_the_terminal_to_the_ui_and_is_held_to_be_told_after_it() {
    let original = panic::take_hook();
    let (said, heard) = std_mpsc::channel();
    let printed = said.clone();
    panic::set_hook(printing(printed));
    let hook = PanicRestore::install(move || said.send("restored").unwrap_or_default());
    let engine = std::thread::Builder::new().name("engine".to_owned());
    assert!(engine.spawn(|| panic!("a panic in an engine task")).unwrap().join().is_err());
    let mut told = Vec::new();
    tell_held(&hook.held, &mut told);
    drop(hook);
    panic::set_hook(original);
    let order: Vec<_> = heard.try_iter().collect();
    assert!(order.is_empty(), "neither restored nor printed over the UI: {order:?}");
    let told = String::from_utf8(told).unwrap_or_default();
    assert!(told.contains("thread 'engine' panicked at "), "{told}");
    assert!(told.contains(":\na panic in an engine task\n"), "{told}");
}

fn once_a_panic_on_the_ui_thread_restored_the_terminal_a_panic_on_another_thread_is_printed() {
    let original = panic::take_hook();
    let (said, heard) = std_mpsc::channel();
    let printed = said.clone();
    panic::set_hook(printing(printed));
    let hook = PanicRestore::install(move || said.send("restored").unwrap_or_default());
    assert!(panic::catch_unwind(|| panic!("a panic in the UI")).is_err());
    assert!(std::thread::spawn(|| panic!("a panic in an engine task")).join().is_err());
    drop(hook);
    panic::set_hook(original);
    let order: Vec<_> = heard.try_iter().collect();
    assert_eq!(order, ["restored", "printed", "printed"], "{order:?}");
}
