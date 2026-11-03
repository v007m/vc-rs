use tokio::sync::mpsc::Receiver;
use vc_model::{Event, Feed, Sequenced};

use super::EventBus;

fn tagged(index: usize) -> Event {
    Event::RunStarted { files: index, instances: 0 }
}

fn drain(feed: &mut Receiver<Feed>) -> Vec<Feed> {
    std::iter::from_fn(|| feed.try_recv().ok()).collect()
}

fn event(seq: u64, index: usize) -> Feed {
    Feed::Event(Sequenced { seq, event: tagged(index) })
}

#[tokio::test]
async fn numbers_every_event_for_every_subscriber() {
    let bus = EventBus::spawn();
    let (mut first, mut second) = (bus.subscribe(8), bus.subscribe(8));
    (1..=3).for_each(|index| bus.publish(tagged(index)));
    bus.flush().await;
    let expected = vec![event(1, 1), event(2, 2), event(3, 3)];
    assert_eq!(drain(&mut first), expected);
    assert_eq!(drain(&mut second), expected);
}

#[tokio::test]
async fn a_late_subscriber_sees_only_later_events_with_run_wide_numbers() {
    let bus = EventBus::spawn();
    bus.publish(tagged(1));
    let mut late = bus.subscribe(8);
    bus.publish(tagged(2));
    bus.flush().await;
    assert_eq!(drain(&mut late), vec![event(2, 2)]);
}

#[tokio::test]
async fn a_slow_subscriber_is_told_how_many_it_missed_before_its_next_event() {
    let bus = EventBus::spawn();
    let (mut slow, mut fast) = (bus.subscribe(2), bus.subscribe(16));
    (1..=5).for_each(|index| bus.publish(tagged(index)));
    bus.flush().await;
    assert_eq!(drain(&mut slow), vec![event(1, 1), event(2, 2)], "the buffer held two; three were dropped");
    assert_eq!(drain(&mut fast).len(), 5, "a slow subscriber never holds up the others");
    bus.publish(tagged(6));
    bus.flush().await;
    assert_eq!(drain(&mut slow), vec![Feed::Missed { count: 3 }, event(6, 6)]);
}

#[tokio::test]
async fn a_flush_delivers_the_missed_count_once_there_is_room() {
    let bus = EventBus::spawn();
    let mut slow = bus.subscribe(1);
    (1..=3).for_each(|index| bus.publish(tagged(index)));
    bus.flush().await;
    assert_eq!(drain(&mut slow), vec![event(1, 1)]);
    bus.flush().await;
    assert_eq!(drain(&mut slow), vec![Feed::Missed { count: 2 }]);
    bus.flush().await;
    assert_eq!(drain(&mut slow).len(), 0, "the count is delivered once");
}

#[tokio::test]
async fn a_dropped_subscriber_does_not_disturb_the_others() {
    let bus = EventBus::spawn();
    let (gone, mut kept) = (bus.subscribe(1), bus.subscribe(4));
    drop(gone);
    (1..=2).for_each(|index| bus.publish(tagged(index)));
    bus.flush().await;
    assert_eq!(drain(&mut kept), vec![event(1, 1), event(2, 2)]);
}
