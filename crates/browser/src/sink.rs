//! Where the playground's output goes: stdout and stderr in the WebAssembly build, a channel in
//! the tests.

use std::fmt::Debug;

/// The stream a piece of output goes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Stream {
    /// What `vc` prints to stdout: the report and the diagnostics.
    Out,
    /// What `vc` prints to stderr: warnings and refusals.
    Err,
}

/// Takes the output as it is printed, so the page shows a run as it goes.
pub trait Sink: Send + Sync + Debug {
    /// Prints `text`, as it is, to `stream`.
    fn print(&self, stream: Stream, text: &str);

    /// Prints `text` and a newline to `stream`.
    fn line(&self, stream: Stream, text: &str) {
        self.print(stream, &format!("{text}\n"));
    }
}
