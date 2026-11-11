//! A sink that captures the output, for the tests.

use std::sync::mpsc::{Receiver, Sender, channel};

use crate::{Sink, Stream};

/// Sends each piece of output down a channel.
#[derive(Debug)]
pub struct ChannelSink(Sender<(Stream, String)>);

impl Sink for ChannelSink {
    fn print(&self, stream: Stream, text: &str) {
        let _sent = self.0.send((stream, text.to_owned()));
    }
}

/// The output a [`ChannelSink`] captured.
pub struct Captured(Receiver<(Stream, String)>);

impl Captured {
    /// A sink, and what it captures.
    pub fn sink() -> (ChannelSink, Self) {
        let (send, receive) = channel();
        (ChannelSink(send), Self(receive))
    }

    /// Every piece printed so far, in order.
    pub fn take(&self) -> Vec<(Stream, String)> {
        self.0.try_iter().collect()
    }

    /// What was printed to `stream` so far, joined.
    pub fn text(pieces: &[(Stream, String)], stream: Stream) -> String {
        pieces.iter().filter(|(to, _)| *to == stream).map(|(_, text)| text.as_str()).collect()
    }
}
