//! `vc new test` and `vc new file`, and what every `vc new` shares: an item appended to the
//! file its ID derives from, or that file created with its header (D139), with one brief
//! comment above the item, and an ID that breaks a convention refused with the one it
//! suggests (D87).

use vc_compiler::{camel_id, file_id, is_camel_id, is_file_id, schema_line};
use vc_diagnostics::TextEdit;
use vc_interfaces::source_file;
use vc_model::{FileKind, SourceFileId, SourcePath};
use vc_parser::{Document, write_text};

use crate::authoring::{Authoring, Named, Opened, item, refused};
use crate::create_load::load_body;
use crate::create_vars::path_vars;
use crate::edit::{Changes, FileChange, internal};
use crate::rename::item_ids;
use crate::{Engine, EngineError};

/// The reference the generated comments link to.
pub const DOCS: &str = "https://github.com/v007m/vc-rs/blob/master/docs/design/syntax-reference.md";

/// The reference section of each kind of file.
const SECTIONS: [(FileKind, &str); 4] = [
    (FileKind::Config, "project-config"),
    (FileKind::Api, "api-files"),
    (FileKind::Test, "tests"),
    (FileKind::Load, "load-files"),
];

/// What a `vc new` command adds, ready to write.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Added {
    /// The change, passed by the compile guard.
    pub changes: Changes,
    /// What was added: `users#deactivate`, `partnerToken` or `users.crud`.
    pub id: String,
    /// The file it was added to, relative to the project root.
    pub path: String,
    /// The line of its `- id:`, from 1; 1 for a new empty file.
    pub line: usize,
}

/// What `vc new test` asks for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built in other crates, shared across the workspace; every user is in this repo (D160)"
)]
pub struct NewTest {
    /// The test's full ID: `users.crud#listsUsers`.
    pub id: String,
    /// The api it calls: `users#list`.
    pub from: String,
    /// Whether to add the failure cases built from the api's body schema (D176).
    pub negative: bool,
    /// Whether to add a test calling the api with `auth: none` and expecting 401.
    pub auth: bool,
}

/// One item `vc new` adds.
pub struct NewItem<'t> {
    /// The kind of file it goes in, and the list there that holds it.
    pub kind: FileKind,
    pub list: &'static str,
    /// Its file ID and item ID.
    pub named: Named<'t>,
    /// Its YAML, the `id:` line first, unindented.
    pub yaml: String,
    /// The comment line above it, `#` included.
    pub comment: String,
    /// Root lines a new file gets before the list: `baseUrl: ${var.searchUrl}`.
    pub root: String,
    /// The other files the command changes.
    pub also: Vec<FileChange>,
    /// A second item added after it, with its comment: the `--auth` test.
    pub next: Option<(String, String)>,
}

impl<'t> NewItem<'t> {
    /// A test `named`, appended to the `tests` of its file, with nothing else changed.
    pub const fn test(named: Named<'t>, yaml: String, comment: String) -> Self {
        let (kind, list, next) = (FileKind::Test, "tests", None);
        Self { kind, list, named, yaml, comment, root: String::new(), also: Vec::new(), next }
    }
}

impl Added {
    /// What was added, as one JSON object: `{"added": …, "path": …, "line": …}`.
    #[must_use]
    pub fn json(&self) -> String {
        serde_json::json!({ "added": self.id, "path": self.path, "line": self.line }).to_string()
    }
}

impl Engine {
    /// The change that appends a test calling `request.from` to the test file its ID derives
    /// from, creating that file when there is none; with `request.auth`, a second test after it
    /// calls the api with `auth: none` and expects 401. Each var in the api's path that the
    /// config leaves unset is set, marked `# replace` (D262).
    ///
    /// # Errors
    ///
    /// [`EngineError::Edit`] for an ID that breaks a convention (with the one it suggests), an
    /// ID the file already has, an unknown api (with the close ones), `--negative` for an api
    /// with no body schema, or a change the guard refuses.
    pub fn new_test(&self, request: &NewTest) -> Result<Added, EngineError> {
        let named = checked(&request.id)?;
        let project = self.authoring()?;
        let api = called(&project, &request.from)?;
        if request.negative {
            let help = "drop `--negative` to add the happy-path test, then add each invalid body as an `examples` row";
            return Err(refused(
                format!("`--negative` cannot build bad-body cases for `{api}`: it has no request body schema"),
                help,
            ));
        }
        let id = named.item.unwrap_or_default();
        let vars = path_vars(&project, api)?;
        let yaml = format!("id: {id}\nname: {}\ncall: {api}{vars}", write_text(&sentence(id), false));
        let comment = format!("# {} · calls {api} · keys: {DOCS}#tests", request.id);
        let mut new = NewItem::test(named, yaml, comment);
        new.next = request.auth.then(|| anonymous(named.file, api, &vars));
        self.added(&project, new)
    }

    /// The change that creates the empty file of `kind` with the ID `id`, holding only its
    /// header; a load file also gets a `mix` of the project's first test and one stage.
    ///
    /// # Errors
    ///
    /// [`EngineError::Edit`] for the config, an ID that breaks the convention, a file that is
    /// already there (it is never overwritten), a load file in a project with no test, or a
    /// change the guard refuses.
    pub fn new_file(&self, kind: FileKind, id: &str) -> Result<Added, EngineError> {
        if kind == FileKind::Config {
            return Err(refused("`vc new file` makes api, test and load files", "`vc init` writes the config"));
        }
        if !is_file_id(id) {
            let help = format!("did you mean `{}`? A file ID is lower-case words joined with dots", file_id(id));
            return Err(refused(format!("`{id}` breaks the file ID convention"), &help));
        }
        let project = self.authoring()?;
        let path = derived_path(kind, id)?.to_string();
        let there = project.find(kind, id).map(|(_, file)| file.path.to_string());
        if let Some(there) =
            there.or_else(|| project.entries().any(|(_, file)| file.path.as_str() == path).then(|| path.clone()))
        {
            return Err(refused(
                format!("there is already a file at {there}"),
                "it is never overwritten; pick another ID",
            ));
        }
        let body = if kind == FileKind::Load { load_body(&project)? } else { String::new() };
        let after = format!("{}{body}", header(kind, id));
        let change = FileChange { path: path.clone(), before: None, after, moved_from: None };
        let changes =
            self.guarded(&project, Changes { files: vec![change], titles: vec![format!("Create {path}")] })?;
        Ok(Added { changes, id: id.to_owned(), path, line: 1 })
    }

    /// `new` added to its file, with the command's other changes, through the guard.
    pub(crate) fn added(&self, project: &Authoring, new: NewItem<'_>) -> Result<Added, EngineError> {
        let (change, line) = match project.find(new.kind, new.named.file) {
            Some(found) => append(&Authoring::open(found)?, &new)?,
            None => create(&new)?,
        };
        let change = match &new.next {
            Some((yaml, comment)) => then_item(change, &ListItem { list: new.list, yaml, comment })?,
            None => change,
        };
        let (id, path) = (format!("{}#{}", new.named.file, new.named.item.unwrap_or_default()), change.path.clone());
        let mut files = new.also;
        files.push(change);
        files.sort_by(|a, b| a.path.cmp(&b.path));
        let changes = self.guarded(project, Changes { files, titles: vec![format!("Add {id}")] })?;
        Ok(Added { changes, id, path, line })
    }
}

/// `text` as `<fileId>#<itemId>`, refused when either part breaks its convention.
pub fn checked(text: &str) -> Result<Named<'_>, EngineError> {
    let named = Named::parse(text);
    let Some(item) = named.item else {
        return Err(refused(
            format!("`{text}` names a file, not an item"),
            &format!("add the item's ID: `{text}#newItem`"),
        ));
    };
    let help = format!(
        "did you mean `{}#{}`? File IDs are dotted lower-case words, item IDs camelCase",
        file_id(named.file),
        camel_id(item)
    );
    match (is_file_id(named.file), is_camel_id(item)) {
        (true, true) => Ok(named),
        (false, _) => Err(refused(format!("`{}` breaks the file ID convention", named.file), &help)),
        (true, false) => Err(refused(format!("`{item}` breaks the ID convention"), &help)),
    }
}

/// `from` when it names an api, or the refusal with the close ones.
pub fn called<'f>(project: &Authoring, from: &'f str) -> Result<&'f str, EngineError> {
    let named = Named::parse(from);
    let found = project.find(FileKind::Api, named.file).map(Authoring::open).transpose()?;
    let known =
        found.as_ref().zip(named.item).is_some_and(|(opened, id)| item(opened.document.root(), "apis", id).is_some());
    if known { Ok(from) } else { Err(Authoring::unknown("api", from, &project.known(FileKind::Api, "apis"))) }
}

/// The path a file of `kind` with the ID `id` derives (D139): its words as folders under
/// `services/` (`load/` for load files), and a one-word api ID in a folder of its own.
pub fn derived_path(kind: FileKind, id: &str) -> Result<SourcePath, EngineError> {
    let root = if kind == FileKind::Load { "load" } else { "services" };
    let path = if kind == FileKind::Api && !id.contains('.') {
        format!("{root}/{id}/{id}.api.yaml")
    } else {
        format!("{root}/{}.{}.yaml", id.replace('.', "/"), kind.name())
    };
    match SourcePath::new(&path) {
        Ok(path) if kind.file_id(path.as_str()) == id => Ok(path),
        _ => Err(refused(format!("`{id}` cannot be the ID of a {} file", kind.name()), "pick another file ID")),
    }
}

/// The header of a new file: its schema line, then what it is and where its keys are listed.
pub fn header(kind: FileKind, id: &str) -> String {
    let section = SECTIONS.iter().find(|(of, _)| *of == kind).map_or("", |(_, section)| section);
    format!("{}\n# {id} · {} file · keys: {DOCS}#{section}\n", schema_line(kind), kind.name())
}

/// The new file holding `new` alone, and the line of its `- id:`.
fn create(new: &NewItem<'_>) -> Result<(FileChange, usize), EngineError> {
    let path = derived_path(new.kind, new.named.file)?;
    let head = format!("{}{}{}:\n  {}\n", header(new.kind, new.named.file), new.root, new.list, new.comment);
    let line = head.lines().count() + 1;
    let after = format!("{head}{}", block("  ", &new.yaml));
    Ok((FileChange { path: path.to_string(), before: None, after, moved_from: None }, line))
}

/// `new` appended to the file `opened`, and the line of its `- id:`.
fn append(opened: &Opened<'_>, new: &NewItem<'_>) -> Result<(FileChange, usize), EngineError> {
    let id = new.named.item.unwrap_or_default();
    if item_ids(opened.document.root()).contains(&id) {
        let full = format!("{}#{id}", new.named.file);
        return Err(refused(
            format!("`{full}` already exists"),
            &format!("pick another ID, or `vc rename {full}` first"),
        ));
    }
    commented_item(opened, &ListItem { list: new.list, yaml: &new.yaml, comment: &new.comment })
}

/// The `--auth` test of a test file `file`: `api` called with `auth: none` and its path `vars`,
/// expecting 401, and its comment. Its ID is the api's with `RejectsAnonymous`:
/// `deactivateRejectsAnonymous`.
fn anonymous(file: &str, api: &str, vars: &str) -> (String, String) {
    let verb = Named::parse(api).item.unwrap_or_default();
    let id = format!("{verb}RejectsAnonymous");
    let name = write_text(&format!("{} needs a signed-in caller", sentence(verb)), false);
    let yaml = format!("id: {id}\nname: {name}\ncall: {api}\nauth: none{vars}\nexpect:\n  status: 401");
    (yaml, format!("# {file}#{id} · calls {api} with no auth · keys: {DOCS}#tests"))
}

/// `change` with `next` added to its list too, after the item `change` adds.
fn then_item(change: FileChange, next: &ListItem<'_>) -> Result<FileChange, EngineError> {
    let path = SourcePath::new(&change.path).map_err(internal)?;
    let file = source_file(path, change.after.clone().into_bytes()).map_err(internal)?;
    let (added, _) = commented_item(&Authoring::open((SourceFileId::new(0), &file))?, next)?;
    Ok(FileChange { after: added.after, ..change })
}

/// A list item to add: the list, the item's YAML and the comment on the line above it.
pub struct ListItem<'a> {
    pub list: &'a str,
    pub yaml: &'a str,
    pub comment: &'a str,
}

/// The change that adds `new` to its list in `opened`, and the line of its `- id:`.
pub fn commented_item(opened: &Opened<'_>, new: &ListItem<'_>) -> Result<(FileChange, usize), EngineError> {
    let ListItem { list, yaml, comment } = *new;
    let document = &opened.document;
    let root = document.root();
    let cannot = |error: vc_parser::ParserError| {
        refused(format!("cannot add to {}: {error}", opened.file.path), "add it by hand")
    };
    let edit = match root.get(list) {
        Some(items) => document.insert_item(items, yaml).map_err(cannot)?,
        None if root.entries().is_empty() && root.items().is_empty() => {
            appended(document, &format!("{list}:\n{}", block("  ", yaml)))
        }
        None => document.insert_yaml(root, list, &block("", yaml)).map_err(cannot)?,
    };
    let (edit, dash) = commented(edit, comment)?;
    let line = line_of(document, &edit, dash);
    Ok((opened.change(&[edit])?, line))
}

/// `text` added at the end of `document`, after a line ending when it has none, in its line
/// endings.
fn appended(document: &Document, text: &str) -> TextEdit {
    let all = document.text();
    let newline = if all.contains("\r\n") { "\r\n" } else { "\n" };
    let lead = if all.is_empty() || all.ends_with('\n') { "" } else { newline };
    let new_text = format!("{lead}{}", text.replace('\n', newline));
    TextEdit { file: document.root().span.file, range: all.len()..all.len(), new_text }
}

/// `edit`, which adds a list item, with `comment` on a line of its own above the item's dash,
/// lined up with it; and where that dash is in its new text.
fn commented(edit: TextEdit, comment: &str) -> Result<(TextEdit, usize), EngineError> {
    let text = &edit.new_text;
    let dash = text.find("- ").ok_or_else(|| internal("a new list item without its dash"))?;
    let start = text.get(..dash).and_then(|before| before.rfind('\n')).map_or(0, |at| at + 1);
    let indent = text.get(start..dash).unwrap_or_default();
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let (head, tail) = text.split_at_checked(start).ok_or_else(|| internal("a new list item cut mid-character"))?;
    let new_text = format!("{head}{indent}{comment}{newline}{tail}");
    let dash = dash + indent.len() + comment.len() + newline.len();
    Ok((TextEdit { new_text, ..edit }, dash))
}

/// The line, from 1, that the byte `dash` of `edit`'s new text lands on in `document`.
fn line_of(document: &Document, edit: &TextEdit, dash: usize) -> usize {
    let before = document.text().get(..edit.range.start).unwrap_or_default();
    let added = edit.new_text.get(..dash).unwrap_or_default();
    before.matches('\n').count() + added.matches('\n').count() + 1
}

/// The item `yaml` as block-list lines after `indent`: `- ` before the first, two spaces
/// before the rest.
pub fn block(indent: &str, yaml: &str) -> String {
    let mut out = String::with_capacity(yaml.len() + 4);
    for (at, line) in yaml.lines().enumerate() {
        out.extend([indent, if at == 0 { "- " } else { "  " }, line, "\n"]);
    }
    out
}

/// A camelCase ID as a sentence: `listsUsers` becomes `Lists users`.
pub fn sentence(id: &str) -> String {
    let mut out = String::with_capacity(id.len() + 4);
    for (at, c) in id.chars().enumerate() {
        match (at, c.is_uppercase()) {
            (0, _) => out.extend(c.to_uppercase()),
            (_, true) => {
                out.push(' ');
                out.extend(c.to_lowercase());
            }
            (_, false) => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use vc_model::FileKind;

    use super::{block, derived_path, sentence};

    fn path(kind: FileKind, id: &str) -> Option<String> {
        derived_path(kind, id).ok().map(|path| path.to_string())
    }

    #[test]
    fn a_file_id_derives_its_path() {
        assert_eq!(path(FileKind::Api, "users").as_deref(), Some("services/users/users.api.yaml"));
        assert_eq!(path(FileKind::Api, "payments.refunds").as_deref(), Some("services/payments/refunds.api.yaml"));
        assert_eq!(path(FileKind::Test, "users.crud").as_deref(), Some("services/users/crud.test.yaml"));
        assert_eq!(path(FileKind::Load, "checkout.peak").as_deref(), Some("load/checkout/peak.load.yaml"));
        assert_eq!(path(FileKind::Api, "users.users"), None, "it would collapse to `users`");
    }

    #[test]
    fn an_item_is_written_as_block_list_lines() {
        assert_eq!(block("  ", "id: a\npath: /a"), "  - id: a\n    path: /a\n");
        assert_eq!(block("", "id: a"), "- id: a\n");
    }

    #[test]
    fn a_test_id_reads_as_a_sentence() {
        assert_eq!(sentence("listsUsers"), "Lists users");
        assert_eq!(sentence("rejectsBadEmail"), "Rejects bad email");
    }
}
