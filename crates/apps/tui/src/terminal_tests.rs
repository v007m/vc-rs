use tokio::sync::mpsc;

use super::{QUEUE, drain};
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
