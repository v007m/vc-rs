use vc_model::ActionId;

use super::{CleanupCollector, CleanupSink};
use crate::{Frame, PendingCleanup};

fn cleanups(ids: &[u32]) -> Vec<PendingCleanup> {
    ids.iter().map(|&id| PendingCleanup::new(ActionId::new(id))).collect()
}

#[test]
fn without_a_sink_the_cleanups_come_back() {
    assert_eq!(CleanupSink::keep(None, cleanups(&[1, 2])), cleanups(&[1, 2]));
}

#[test]
fn a_collector_gives_back_what_was_handed_over_then_the_rest() {
    let mut collector = CleanupCollector::under(None);
    let frame = collector.frame(&Frame::default());
    assert_eq!(CleanupSink::keep(frame.sink.as_ref(), cleanups(&[1])), Vec::new());
    collector.hand(cleanups(&[2]));
    assert_eq!(collector.take(cleanups(&[3])), cleanups(&[1, 2, 3]));
    assert_eq!(collector.take(Vec::new()), Vec::new(), "taking empties it");
}

#[test]
fn a_collector_dropped_hands_what_is_left_to_its_parent() {
    let mut parent = CleanupCollector::under(None);
    let child = CleanupCollector::under(parent.frame(&Frame::default()).sink);
    child.hand(cleanups(&[4, 5]));
    drop(child);
    assert_eq!(parent.take(Vec::new()), cleanups(&[4, 5]));
}

#[test]
fn a_sink_whose_collector_is_gone_gives_the_cleanups_back() {
    let sink = CleanupCollector::under(None).frame(&Frame::default()).sink;
    assert_eq!(CleanupSink::keep(sink.as_ref(), cleanups(&[6])), cleanups(&[6]));
}
