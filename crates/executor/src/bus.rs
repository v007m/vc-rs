//! The event bus (observer): a fan-out task numbers every event and hands it to each subscriber.

use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::{self, Receiver, Sender, UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;
use vc_model::{Event, Feed, Sequenced};

enum Message {
    Subscribe(Sender<Feed>),
    Publish(Event),
    Flush(oneshot::Sender<()>),
}

/// A handle to the event bus.
///
/// Producers never wait: publishing goes to the fan-out task, which gives each event its run-wide sequence number and offers it to every subscriber's bounded
/// buffer. A full buffer drops the event; the subscriber is told how many it missed
/// ([`Feed::Missed`]) before its next event, and reloads them from the history store.
#[derive(Clone, Debug)]
pub struct EventBus {
    inbox: UnboundedSender<Message>,
}

impl std::fmt::Debug for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Message")
    }
}

impl EventBus {
    /// Starts the fan-out task, which ends when the last handle is dropped. Call it from within
    /// a Tokio runtime.
    #[must_use]
    pub fn spawn() -> Self {
        let (inbox, messages) = mpsc::unbounded_channel();
        tokio::spawn(fan_out(messages));
        Self { inbox }
    }

    /// A subscriber with room for `capacity` events (at least one). It receives every event
    /// published after this call.
    #[must_use]
    pub fn subscribe(&self, capacity: usize) -> Receiver<Feed> {
        let (feed, receiver) = mpsc::channel(capacity.max(1));
        self.inbox.send(Message::Subscribe(feed)).ok();
        receiver
    }

    /// Publishes `event`.
    pub fn publish(&self, event: Event) {
        self.inbox.send(Message::Publish(event)).ok();
    }

    /// Waits until every event published so far has been offered to the subscribers, and any
    /// missed count they are owed has been delivered where there is room.
    pub async fn flush(&self) {
        let (done, flushed) = oneshot::channel();
        if self.inbox.send(Message::Flush(done)).is_ok() {
            flushed.await.ok();
        }
    }
}

struct Subscriber {
    feed: Sender<Feed>,
    missed: u64,
}

impl Subscriber {
    /// Delivers the missed count, if any. False while the buffer has no room for it.
    fn settle(&mut self) -> bool {
        if self.missed == 0 {
            return true;
        }
        let count = self.missed;
        self.feed.try_send(Feed::Missed { count }).map(|()| self.missed = 0).is_ok()
    }

    /// Offers `event`, after any missed count; counts it as missed when there is no room.
    fn deliver(&mut self, event: &Sequenced) {
        let room = self.settle() && !matches!(self.feed.try_send(Feed::Event(event.clone())), Err(TrySendError::Full(_)));
        if !room {
            self.missed += 1;
        }
    }
}

async fn fan_out(mut messages: UnboundedReceiver<Message>) {
    let mut seq = 0;
    let mut subscribers: Vec<Subscriber> = Vec::new();
    while let Some(message) = messages.recv().await {
        match message {
            Message::Subscribe(feed) => subscribers.push(Subscriber { feed, missed: 0 }),
            Message::Publish(event) => {
                seq += 1;
                let event = Sequenced { seq, event };
                for subscriber in &mut subscribers {
                    subscriber.deliver(&event);
                }
            }
            Message::Flush(done) => {
                for subscriber in &mut subscribers {
                    subscriber.settle();
                }
                done.send(()).ok();
            }
        }
        subscribers.retain(|subscriber| !subscriber.feed.is_closed());
    }
}

#[cfg(test)]
#[path = "bus_tests.rs"]
mod tests;
