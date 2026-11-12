//! A free port for a demo a test starts, shared by the test binaries that include this file with
//! `#[path]`.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::net::TcpListener;

/// The first port handed out: above the sample's 7421.
const FIRST: u16 = 10_000;

/// How many ports are handed out: those under every OS's own range (Linux's starts at 32768,
/// macOS's and Windows' at 49152).
const COUNT: u16 = 22_768;

/// A port nothing listens on now, for a demo the test starts on it.
///
/// A port the OS picks (`bind :0`) comes from its own range, where any other test's server or
/// outgoing connection may take it before the demo binds it. This one is below that range, and
/// the search starts at a port drawn from this process and test thread, so tests side by side
/// (also in other checkouts) look at different ports first.
pub fn free_port() -> u16 {
    let mut drawn = DefaultHasher::new();
    (std::process::id(), std::thread::current().name()).hash(&mut drawn);
    let start = drawn.finish() % u64::from(COUNT);
    let port = |step: u64| FIRST + u16::try_from((start + step) % u64::from(COUNT)).unwrap();
    (0..u64::from(COUNT)).map(port).find(|port| TcpListener::bind(("127.0.0.1", *port)).is_ok()).unwrap()
}
