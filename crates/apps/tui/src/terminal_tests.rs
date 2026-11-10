use std::panic;
use std::sync::mpsc as std_mpsc;

use tokio::sync::mpsc;

use super::{PanicRestore, QUEUE, drain};
use crate::screen::Update;
use crate::{Tui, TuiOptions};

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

#[test]
fn a_panic_restores_the_terminal_before_the_hook_before_prints_it_and_the_hook_is_put_back() {
    let original = panic::take_hook();
    let (said, heard) = std_mpsc::channel();
    let printed = said.clone();
    panic::set_hook(Box::new(move |_| printed.send("printed").unwrap_or_default()));
    let hook = PanicRestore::install(move || said.send("restored").unwrap_or_default());
    assert!(panic::catch_unwind(|| panic!("a panic in the UI")).is_err());
    drop(hook);
    assert!(panic::catch_unwind(|| panic!("a panic after the UI")).is_err());
    panic::set_hook(original);
    let order: Vec<_> = heard.try_iter().collect();
    assert_eq!(order[..3], ["restored", "printed", "printed"], "{order:?}");
}

#[test]
fn a_panic_on_another_thread_leaves_the_terminal_to_the_ui() {
    let original = panic::take_hook();
    let (said, heard) = std_mpsc::channel();
    let printed = said.clone();
    panic::set_hook(Box::new(move |_| printed.send("printed").unwrap_or_default()));
    let hook = PanicRestore::install(move || said.send("restored").unwrap_or_default());
    assert!(std::thread::spawn(|| panic!("a panic in an engine task")).join().is_err());
    drop(hook);
    panic::set_hook(original);
    let order: Vec<_> = heard.try_iter().collect();
    assert_eq!(order, ["printed"], "{order:?}");
}
