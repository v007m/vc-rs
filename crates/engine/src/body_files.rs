//! The bytes of the `$file` bodies a run can send, read before it starts so the executor does
//! no I/O (D272).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bytes::Bytes;
use vc_compiler::{FilePath, Unreadable};
use vc_executor::{BodyFiles, Unread};
use vc_interfaces::SourceProvider;
use vc_model::{ActionId, Plan, Program, SourcePath};

use crate::Engine;

/// Each file read, by its project path: its bytes or why it could not be read.
type Read = BTreeMap<SourcePath, Result<Bytes, String>>;

impl Engine {
    /// The bytes of the `$file`s that `actions` send, each file read once on a blocking thread.
    /// A templated or refused path, or a file that cannot be read, fails the requests that send
    /// it (V5111).
    pub(crate) async fn body_files(&self, program: &Program, actions: BTreeSet<ActionId>) -> BodyFiles {
        let found = vc_compiler::body_files(program, actions);
        let paths: BTreeSet<SourcePath> = found.iter().filter_map(|(_, path)| path.as_ref().ok().cloned()).collect();
        let read = read_all(Arc::clone(&self.sources), paths).await;
        BodyFiles::new(found.into_iter().map(|(slot, path)| (slot, bytes(&read, path))))
    }
}

/// The actions whose bodies a run of `plan` can send: those it reaches and, when the program
/// caches, every api's cleanup, which the sweep of an expired entry can run (D272).
pub fn reached(program: &Program, plan: &Plan) -> BTreeSet<ActionId> {
    let mut reached = vc_planner::bodies(program, plan);
    if caches(program) {
        reached.extend(vc_planner::bodies_from(program, &cleanups(program)));
    }
    reached
}

/// Whether any action of `program` keeps its result in the cross-run cache.
pub fn caches(program: &Program) -> bool {
    program.actions.iter().any(|action| action.cache.is_some())
}

/// Every api's cleanup action.
pub fn cleanups(program: &Program) -> Vec<ActionId> {
    program.apis.values().filter_map(|api| api.cleanup).collect()
}

/// `paths` read from `sources` on a blocking thread.
async fn read_all(sources: Arc<dyn SourceProvider>, paths: BTreeSet<SourcePath>) -> Read {
    if paths.is_empty() {
        return Read::new();
    }
    let wanted = paths.clone();
    let reading = tokio::task::spawn_blocking(move || {
        let read = |path: SourcePath| {
            let bytes = sources.read_bytes(&path).map(Bytes::from).map_err(|error| error.to_string());
            (path, bytes)
        };
        paths.into_iter().map(read).collect::<Read>()
    });
    reading.await.unwrap_or_else(|error| {
        wanted.into_iter().map(|path| (path, Err(format!("the read was stopped: {error}")))).collect()
    })
}

/// What a `$file` at `path` sends: the bytes read, or why there are none.
fn bytes(read: &Read, path: FilePath) -> Result<Bytes, Unread> {
    let path = path.map_err(|why| match why {
        Unreadable::Templated => Unread::Templated,
        Unreadable::Refused(why) => Unread::Refused(why),
        _ => Unread::Refused("an internal error: the compile gave an unknown reason".to_owned()),
    })?;
    match read.get(&path) {
        Some(Ok(bytes)) => Ok(bytes.clone()),
        Some(Err(why)) => Err(Unread::Failed(why.clone())),
        None => Err(Unread::Failed("it was not read".to_owned())),
    }
}
