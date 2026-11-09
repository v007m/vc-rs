//! What the collection importers share: a request in one neutral form, its `{{name}}` variables
//! as template text, the draft api files the requests become and the `--flow` test calling them.
//! Each importer only reads its own file into [`Call`]s and its variables into [`Values`].

mod access;
mod draft;
mod flow;
mod payload;
mod request;
mod vars;

pub use access::{path, plain};
pub use draft::draft;
pub use flow::flow;
pub use request::{Call, Credential, Payload};
pub use vars::Values;

/// `collection Shop · 7 requests in 2 folders`.
pub fn summary(name: &str, requests: usize, folders: usize) -> String {
    let plural = |count: usize, word: &str| format!("{count} {word}{}", if count == 1 { "" } else { "s" });
    let folders = if folders == 0 { String::new() } else { format!(" in {}", plural(folders, "folder")) };
    format!("collection {name} · {}{folders}", plural(requests, "request"))
}
