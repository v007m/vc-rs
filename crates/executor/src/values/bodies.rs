//! The bytes of the `$file`s request bodies send, read by the engine before the run so the
//! executor does no I/O (D272). Each is held by the action that sends the body and the JSON
//! pointer to the `$file` in it (D274), so the body itself is sent as written.

use std::collections::HashMap;
use std::sync::Arc;

use bytes::Bytes;
use vc_model::ActionId;

/// Why the engine did not read a `$file`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Unread {
    /// Its path was refused: it leads out of the project, or is not a project path.
    Refused(String),
    /// The file could not be read.
    Failed(String),
}

/// A `$file`'s bytes, or why it was not read.
type Read = Result<Bytes, Unread>;

/// What the engine read for one `$file`: its action and pointer, and its bytes or why not.
type BodyFile = ((ActionId, String), Result<Vec<u8>, Unread>);

/// Each `$file`'s bytes by the action that sends it and its pointer in the body, or why it was
/// not read.
#[derive(Clone, Debug, Default)]
pub struct BodyFiles(Arc<HashMap<ActionId, Pointers>>);

/// One action's `$file`s by their pointer in its body.
type Pointers = HashMap<String, Read>;

impl BodyFiles {
    /// The files the engine read, each by its action and pointer.
    #[must_use]
    pub fn new(files: impl IntoIterator<Item = BodyFile>) -> Self {
        let mut held: HashMap<ActionId, Pointers> = HashMap::new();
        for ((action, pointer), read) in files {
            held.entry(action).or_default().insert(pointer, read.map(Bytes::from));
        }
        Self(Arc::new(held))
    }

    /// The bytes of the `$file` at `pointer` in `action`'s body, or why it was not read; `None`
    /// when the run was not given it, as for a path that arrives in a `${…}` value.
    #[must_use]
    pub fn get(&self, action: ActionId, pointer: &str) -> Option<Result<&Bytes, &Unread>> {
        self.0.get(&action)?.get(pointer).map(Result::as_ref)
    }
}
