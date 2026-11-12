//! One playground command, start to end: the files the page sent, checked or run, with what the
//! CLI would print for the same command and its exit code.

use std::sync::Arc;

use crate::run::{self, Ran};
use vc_interfaces::MemorySources;

use crate::{Command, Request, Sink, check, print};

/// Carries out `request`, printing to `sink`; the exit code `vc` would give.
pub async fn execute(request: Request, sink: Arc<dyn Sink>) -> u8 {
    let Request { files, command, color, seed } = request;
    let project = MemorySources::from_files(files.into_iter().map(|(path, text)| (path, text.into_bytes())));
    match command {
        Command::Check => check::check(&project, sink.as_ref(), color)
            .unwrap_or_else(|error| print::error(sink.as_ref(), &error, color)),
        Command::Run => match run::run(&project, Arc::clone(&sink), (color, seed)).await {
            Ok(Ran { exit, seed, notices }) => {
                print::notes(sink.as_ref(), &notices, (exit, seed));
                exit
            }
            Err(error) => print::error(sink.as_ref(), &error, color),
        },
    }
}

#[cfg(test)]
#[path = "playground_tests.rs"]
mod tests;
