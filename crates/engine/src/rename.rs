//! `vc rename`: an api or test ID, or a file ID, renamed with every reference to it (D87). A
//! new file ID moves the file to the path that ID derives (D139). Vars, auths and datasets are
//! renamed in `rename_name`.

use vc_compiler::{camel_id, file_id, is_camel_id, is_file_id};
use vc_diagnostics::TextEdit;
use vc_model::{FileKind, SourcePath};
use vc_parser::{Document, Node};

use crate::authoring::{Authoring, Named, Opened, item, refused};
use crate::edit::{Changes, FileChange, internal};
use crate::rename_name::renamed_name;
use crate::{Engine, EngineError};

/// The kinds of file whose items `vc rename` renames, each with the list that holds them.
const LISTS: [(FileKind, &str); 2] = [(FileKind::Api, "apis"), (FileKind::Test, "tests")];

/// The kinds of file `vc rename` moves.
const MOVABLE: [FileKind; 3] = [FileKind::Api, FileKind::Test, FileKind::Load];

/// The full IDs one rename changes: `(from, to)` each.
type Renames = [(String, String)];

impl Engine {
    /// The change that renames `from` to `to` and every reference to it: an api or test
    /// (`users#create` to `users#createUser`), a file (`users.crud` to `users.accounts`), which
    /// then moves to the path its new ID derives, or a var, auth or dataset written with its
    /// kind (`var.userId` to `var.accountId`, `auth.…`, `dataset.…`).
    ///
    /// # Errors
    ///
    /// [`EngineError::Edit`] for an unknown `from` (with the close ones), a `to` that breaks the
    /// naming convention (with the one it suggests), is taken or is in another file, or a change
    /// the guard refuses.
    pub fn rename(&self, from: &str, to: &str) -> Result<Changes, EngineError> {
        let project = self.authoring()?;
        let files = renamed(&project, from, to)?;
        self.guarded(&project, Changes { files, titles: vec![format!("Rename {from} to {to}")] })
    }
}

/// The changes that rename `from` to `to`, by the kind of ID `from` is.
fn renamed(project: &Authoring, from: &str, to: &str) -> Result<Vec<FileChange>, EngineError> {
    if let Some(files) = renamed_name(project, from, to) {
        return files;
    }
    match (Named::parse(from).item, Named::parse(to).item) {
        (Some(_), Some(_)) => rename_item(project, from, to),
        (None, None) => rename_file(project, from, to),
        _ => {
            let help = "rename an item to an item (`users#list` to `users#listAll`) or a file to a file \
                        (`users.crud` to `users.accounts`)";
            Err(refused(format!("`{from}` and `{to}` are not the same kind of ID"), help))
        }
    }
}

/// The changes that rename the item `from` to `to`, in its file and every file that refers to it.
fn rename_item(project: &Authoring, from: &str, to: &str) -> Result<Vec<FileChange>, EngineError> {
    let (old, new) = (Named::parse(from), Named::parse(to));
    let new_id = new.item.unwrap_or_default();
    if old.file != new.file {
        let help =
            format!("rename it to `{}#{new_id}`; `vc rename {} <file ID>` moves a whole file", old.file, old.file);
        return Err(refused(format!("`{to}` is in another file than `{from}`"), &help));
    }
    if !is_camel_id(new_id) {
        let help = format!("did you mean `{}#{}`? Item IDs are camelCase", new.file, camel_id(new_id));
        return Err(refused(format!("`{new_id}` breaks the ID convention"), &help));
    }
    let (opened, list) = defining(project, from)?;
    let defined = item(opened.document.root(), list, old.item.unwrap_or_default());
    let id_node =
        defined.and_then(|found| found.get("id")).ok_or_else(|| internal(format!("`{from}` went missing")))?;
    if item_ids(opened.document.root()).contains(&new_id) {
        return Err(refused(format!("`{to}` is already taken"), &format!("pick another ID, or rename `{to}` first")));
    }
    let renames = [(from.to_owned(), to.to_owned())];
    let mut edits = vec![opened.document.set_scalar(id_node, new_id).map_err(internal)?];
    edits.extend(renamed_refs(&opened.document, old.file, &renames)?);
    let mut files = vec![opened.change(&edits)?];
    files.extend(referrers(project, &opened.file.path, &renames)?);
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

/// The file that defines the item `from`, opened, and the list that holds it.
fn defining<'p>(project: &'p Authoring, from: &str) -> Result<(Opened<'p>, &'static str), EngineError> {
    let named = Named::parse(from);
    let wanted = named.item.unwrap_or_default();
    for (kind, list) in LISTS {
        let Some(found) = project.find(kind, named.file) else { continue };
        let opened = Authoring::open(found)?;
        if item(opened.document.root(), list, wanted).is_some() {
            return Ok((opened, list));
        }
    }
    let what = if project.find(FileKind::Test, named.file).is_some() { "test" } else { "api" };
    let known: Vec<String> = LISTS.iter().flat_map(|&(kind, list)| project.known(kind, list)).collect();
    Err(Authoring::unknown(what, from, &known))
}

/// The changes that move the file `from` to the path of the ID `to`, renaming its items' full
/// IDs everywhere they are referred to.
fn rename_file(project: &Authoring, from: &str, to: &str) -> Result<Vec<FileChange>, EngineError> {
    if !is_file_id(to) {
        let help = format!("did you mean `{}`? A file ID is lower-case words joined with dots", file_id(to));
        return Err(refused(format!("`{to}` breaks the file ID convention"), &help));
    }
    let Some((kind, found)) = MOVABLE.into_iter().find_map(|kind| project.find(kind, from).map(|found| (kind, found)))
    else {
        let known: Vec<String> = project.entries().filter_map(|(_, file)| file.path.file_id()).collect();
        return Err(Authoring::unknown("file", from, &known));
    };
    let path = moved_path(kind, &found.1.path, to)?;
    if project.entries().any(|(_, file)| file.path == path) {
        return Err(refused(format!("there is already a file at {path}"), "pick another file ID"));
    }
    let opened = Authoring::open(found)?;
    let ids = item_ids(opened.document.root());
    let renames: Vec<(String, String)> = ids.iter().map(|id| (format!("{from}#{id}"), format!("{to}#{id}"))).collect();
    let mut edits = renamed_refs(&opened.document, to, &renames)?;
    if let Some(id) = opened.document.root().get("id") {
        edits.push(opened.document.set_scalar(id, to).map_err(internal)?);
    }
    let moved =
        FileChange { path: path.to_string(), moved_from: Some(found.1.path.to_string()), ..opened.change(&edits)? };
    let mut files = vec![moved];
    files.extend(referrers(project, &found.1.path, &renames)?);
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

/// Where a file of `kind` at `old` goes when its ID becomes `to`: the same root folder, a
/// folder per dotted word, and an api file named like its folder kept so (D139, D142).
fn moved_path(kind: FileKind, old: &SourcePath, to: &str) -> Result<SourcePath, EngineError> {
    let old = old.as_str();
    let root = ["services/", "load/"].into_iter().find(|root| old.starts_with(root)).unwrap_or_default();
    let mut words: Vec<&str> = to.split('.').collect();
    let below = old.strip_prefix(root).unwrap_or(old).split('/').count();
    if below > kind.file_id(old).split('.').count()
        && let Some(&last) = words.last()
    {
        words.push(last);
    }
    let path = format!("{root}{}.{}.yaml", words.join("/"), kind.name());
    match SourcePath::new(&path) {
        Ok(path) if kind.file_id(path.as_str()) == to => Ok(path),
        _ => Err(refused(format!("`{to}` cannot be the ID of a {} file", kind.name()), "move the file by hand")),
    }
}

/// The ID of every item in a list at the top of `root`: the file's one namespace.
pub fn item_ids(root: &Node) -> Vec<&str> {
    let items = root.entries().iter().flat_map(|entry| entry.value.items());
    items.filter_map(|item| item.get("id")?.as_str()).collect()
}

/// The edits that rename every reference in `document`, the file `file_id`, by `renames`.
fn renamed_refs(document: &Document, file_id: &str, renames: &Renames) -> Result<Vec<TextEdit>, EngineError> {
    let mut edits = Vec::new();
    for (from, to) in renames {
        edits.extend(document.rename_refs(file_id, from, to).map_err(internal)?);
    }
    Ok(edits)
}

/// The change to each file but `own` that refers to an ID in `renames`.
fn referrers(project: &Authoring, own: &SourcePath, renames: &Renames) -> Result<Vec<FileChange>, EngineError> {
    let mut files = Vec::new();
    for found in project.entries().filter(|(_, file)| &file.path != own && file.kind().is_some()) {
        let opened = Authoring::open(found)?;
        let edits = renamed_refs(&opened.document, &found.1.path.file_id().unwrap_or_default(), renames)?;
        if !edits.is_empty() {
            files.push(opened.change(&edits)?);
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use vc_model::{FileKind, SourcePath};

    use super::moved_path;

    fn moved(kind: FileKind, old: &str, to: &str) -> Option<String> {
        moved_path(kind, &SourcePath::new(old).ok()?, to).ok().map(|path| path.to_string())
    }

    #[test]
    fn a_moved_file_keeps_its_root_folder_and_layout() {
        let test = FileKind::Test;
        assert_eq!(
            moved(test, "services/users/crud.test.yaml", "users.accounts").as_deref(),
            Some("services/users/accounts.test.yaml")
        );
        assert_eq!(
            moved(test, "services/users/crud.test.yaml", "billing").as_deref(),
            Some("services/billing.test.yaml")
        );
        let api = FileKind::Api;
        assert_eq!(
            moved(api, "services/users/users.api.yaml", "people").as_deref(),
            Some("services/people/people.api.yaml")
        );
        assert_eq!(moved(api, "services/users.api.yaml", "people").as_deref(), Some("services/people.api.yaml"));
        assert_eq!(
            moved(FileKind::Load, "load/peak.load.yaml", "busy.peak").as_deref(),
            Some("load/busy/peak.load.yaml")
        );
    }

    #[test]
    fn refuses_an_id_the_new_path_would_not_derive() {
        assert_eq!(moved(FileKind::Api, "services/users.api.yaml", "users.users"), None);
    }
}
