//! The guard that keeps `vc import` and `vc sync` from writing a spec snapshot,
//! `openapi/<fileId>.yaml`, over a spec: the one read, or one another api file syncs from.

use vc_interfaces::SourceProvider;
use vc_model::{FileKind, SourcePath};
use vc_parser::Node;

use crate::EngineError;
use crate::authoring::{Authoring, refused};

/// The project's sources, and the spec each api file syncs from: its `source.openapi`.
pub struct SnapshotGuard<'g> {
    sources: &'g dyn SourceProvider,
    specs: Vec<(String, String)>,
}

impl<'g> SnapshotGuard<'g> {
    /// The guard over `project`'s api files, as `sources` finds them on disk.
    pub fn new(sources: &'g dyn SourceProvider, project: &Authoring) -> Self {
        let apis = project.entries().filter(|(_, file)| file.kind() == Some(FileKind::Api));
        let specs = apis
            .filter_map(|found| {
                let opened = Authoring::open(found).ok()?;
                let spec = opened.document.root().get("source").and_then(|source| source.get("openapi"));
                Some((FileKind::Api.file_id(found.1.path.as_str()), spec.and_then(Node::as_str)?.to_owned()))
            })
            .collect();
        Self { sources, specs }
    }

    /// Refuses an import or sync of the spec at `source` into `file_id` when writing its
    /// snapshot would change a spec: the snapshot path leads to another file (a link, or a file
    /// in another case on a file system that ignores case), `source` is a snapshot path or leads
    /// to one, or an api file syncs from the snapshot path.
    pub fn check(&self, source: &str, file_id: &str) -> Result<(), EngineError> {
        let snapshot = snapshot_path(file_id);
        if let Some(elsewhere) = self.real(&snapshot).filter(|at| *at != snapshot) {
            let reason = format!("{snapshot}, where `vc import` keeps the snapshot, leads to {elsewhere}");
            return Err(refused(reason, &format!("make `{snapshot}` a plain file of its own, or use another file ID")));
        }
        self.not_a_snapshot(source)?;
        let Some((other, _)) = self.specs.iter().find(|(_, spec)| self.leads_to(spec, &snapshot)) else {
            return Ok(());
        };
        let reason = format!(
            "{snapshot}, where `vc import` keeps the snapshot of `{file_id}`, is the spec `{other}` syncs from"
        );
        let help = format!("point `source.openapi` in `{other}` at a spec outside `openapi/`, or use another file ID");
        Err(refused(reason, &help))
    }

    /// Refuses a spec read from a snapshot path, or from a file that leads to one.
    fn not_a_snapshot(&self, source: &str) -> Result<(), EngineError> {
        if source.contains("://") {
            return Ok(());
        }
        let path = plain(source);
        let at = if is_snapshot(&path) { path.clone() } else { self.real(&path).unwrap_or_default() };
        let Some(owner) = owner(&at) else { return Ok(()) };
        let kept = format!("`vc import` keeps the snapshot `vc sync {owner}` merges against");
        let reason =
            if at == path { format!("{at} is where {kept}") } else { format!("{path} leads to {at}, where {kept}") };
        let help = format!(
            "move the spec out of `openapi/` (to `specs/{owner}.yaml`, say), then import it from there or point \
             `source.openapi` at it"
        );
        Err(refused(reason, &help))
    }

    /// Whether the spec at `spec`, as an api file names it, is the file at `snapshot`.
    fn leads_to(&self, spec: &str, snapshot: &str) -> bool {
        let path = plain(spec);
        !spec.contains("://") && (path == snapshot || self.real(&path).is_some_and(|real| real == snapshot))
    }

    /// Where the file at `path` really is, in the project; none when the provider cannot tell.
    fn real(&self, path: &str) -> Option<String> {
        let path = SourcePath::new(path).ok()?;
        self.sources.real(&path).map(|real| real.as_str().to_owned())
    }
}

/// Where the spec snapshot of the api file `file_id` is: `openapi/<fileId>.yaml`.
pub fn snapshot_path(file_id: &str) -> String {
    format!("openapi/{file_id}.yaml")
}

/// Whether `path` is a spec snapshot's: `openapi/<fileId>.yaml`.
pub fn is_snapshot(path: &str) -> bool {
    owner(path).is_some()
}

/// The file ID whose snapshot is at `path`.
fn owner(path: &str) -> Option<&str> {
    let id = path.strip_prefix("openapi/")?.strip_suffix(".yaml")?;
    Some(id).filter(|id| !id.is_empty() && !id.contains('/'))
}

/// `path` with forward slashes and without a leading `./`.
fn plain(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches("./").to_owned()
}

#[cfg(test)]
mod tests {
    use super::{is_snapshot, plain};

    #[test]
    fn a_snapshot_path_is_a_yaml_file_right_in_openapi() {
        assert!(is_snapshot("openapi/users.yaml") && is_snapshot("openapi/pay.refunds.yaml"));
        for other in ["openapi/a/b.yaml", "openapi/.yaml", "openapi/users.json", "specs/users.yaml", "openapi.yaml"] {
            assert!(!is_snapshot(other), "{other}");
        }
        assert_eq!(plain(".\\openapi\\users.yaml"), "openapi/users.yaml");
    }
}
