//! The bytes of the `$file` bodies a run can send, read before it starts so the executor does
//! no I/O (D272).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use vc_compiler::FilePath;
use vc_executor::{BodyFiles, Unread};
use vc_interfaces::SourceProvider;
use vc_model::{ActionId, Program, SourcePath};

use crate::Engine;

/// Each file read, by its project path: its bytes or why it could not be read.
type Read = BTreeMap<SourcePath, Result<Vec<u8>, String>>;

impl Engine {
    /// The bytes of the literal `$file`s that `actions` send, each file read once on a blocking
    /// thread. A refused path, or a file that cannot be read, fails the requests that send it
    /// (V5111).
    pub(crate) async fn body_files(&self, program: &Program, actions: BTreeSet<ActionId>) -> BodyFiles {
        let found = vc_compiler::body_files(program, actions);
        let paths: BTreeSet<SourcePath> = found.iter().filter_map(|(_, path)| path.as_ref().ok().cloned()).collect();
        let read = read_all(Arc::clone(&self.sources), paths).await;
        BodyFiles::new(found.into_iter().map(|(slot, path)| (slot, bytes(&read, path))))
    }
}

/// `paths` read from `sources` on a blocking thread.
async fn read_all(sources: Arc<dyn SourceProvider>, paths: BTreeSet<SourcePath>) -> Read {
    if paths.is_empty() {
        return Read::new();
    }
    let wanted = paths.clone();
    let reading = tokio::task::spawn_blocking(move || {
        let read = |path: SourcePath| {
            let bytes = sources.read_bytes(&path).map_err(|error| error.to_string());
            (path, bytes)
        };
        paths.into_iter().map(read).collect::<Read>()
    });
    reading.await.unwrap_or_else(|error| {
        wanted.into_iter().map(|path| (path, Err(format!("the read was stopped: {error}")))).collect()
    })
}

/// What a `$file` at `path` sends: the bytes read, or why there are none.
fn bytes(read: &Read, path: FilePath) -> Result<Vec<u8>, Unread> {
    let path = path.map_err(Unread::Refused)?;
    match read.get(&path) {
        Some(Ok(bytes)) => Ok(bytes.clone()),
        Some(Err(why)) => Err(Unread::Failed(why.clone())),
        None => Err(Unread::Failed("it was not read".to_owned())),
    }
}
