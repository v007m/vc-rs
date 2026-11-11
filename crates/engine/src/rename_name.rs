//! `vc rename var.<name>|auth.<id>|dataset.<id>`: a var, an auth or a dataset renamed with
//! every reference to it (D87).

use vc_compiler::{camel_id, is_camel_id};
use vc_diagnostics::TextEdit;
use vc_model::FileKind;
use vc_parser::{Document, Node, ParserError};

use crate::EngineError;
use crate::authoring::{Authoring, Opened, item, refused};
use crate::edit::{FileChange, internal};
use crate::format::is_generated;

/// The edits that rename `from` to `to` in one document.
type Renames = fn(&Document, &str, &str) -> Result<Vec<TextEdit>, ParserError>;

/// A kind of name `vc rename` renames, written `<prefix>.<name>`: a registry entry.
struct NameKind {
    /// The word before the name: `var`, `auth` or `dataset`.
    prefix: &'static str,
    /// Where the config lists its definitions, each with an `id`: the keys to the list.
    listed: &'static [&'static str],
    /// Names it can never take.
    reserved: &'static [&'static str],
    /// The help for a name it does not know.
    unknown_help: &'static str,
    /// Whether a name keeps its convention.
    valid: fn(&str) -> bool,
    /// Whether the comments `vc new` writes name it (`# partnerToken · bearer`).
    commented: bool,
    /// The edits that rename `from` to `to` in one document: references and, for a var, the
    /// keys that define it.
    edits: Renames,
}

/// Every kind of name `vc rename` renames besides items and files.
const KINDS: [NameKind; 3] = [
    NameKind {
        prefix: "var",
        listed: &[],
        reserved: &[],
        unknown_help: "a var is a key under `vars` or `save`, used as `${var.<name>}`",
        valid: is_camel_id,
        commented: false,
        edits: Document::rename_var,
    },
    NameKind {
        prefix: "auth",
        listed: &["auth"],
        reserved: &["none"],
        unknown_help: "the config's `auth` list defines the auths",
        valid: is_camel_id,
        commented: true,
        edits: Document::rename_auth,
    },
    NameKind {
        prefix: "dataset",
        listed: &["datasets", "files"],
        reserved: &[],
        unknown_help: "only a dataset listed under the config's `datasets.files` is renamed here; move a file \
                       under `datasets/` by hand, and `vc check` shows each reference to fix",
        valid: is_dataset_id,
        commented: false,
        edits: Document::rename_dataset,
    },
];

/// The changes that rename the var, auth or dataset `from` (`var.userId`) to `to`
/// (`var.accountId`); `None` when `from` names none of them.
pub fn renamed_name(project: &Authoring, from: &str, to: &str) -> Option<Result<Vec<FileChange>, EngineError>> {
    let (prefix, old) = from.split_once('.')?;
    let kind = KINDS.iter().find(|kind| kind.prefix == prefix)?;
    Some(kind.checked(old, to).and_then(|new| kind.renamed(project, old, new)))
}

impl NameKind {
    /// The new name in `to`, refused when it is of another kind, breaks the convention, is
    /// reserved or is `old` itself.
    fn checked<'t>(&self, old: &str, to: &'t str) -> Result<&'t str, EngineError> {
        let Some(new) = to.strip_prefix(self.prefix).and_then(|rest| rest.strip_prefix('.')) else {
            let help = format!("write the new name as `{}.<name>`", self.prefix);
            return Err(refused(format!("`{to}` is not a {} name", self.prefix), &help));
        };
        if !(self.valid)(new) {
            let help = format!("did you mean `{}.{}`? Names are camelCase", self.prefix, camel_id(new));
            return Err(refused(format!("`{new}` breaks the naming convention"), &help));
        }
        if self.reserved.contains(&new) || new == old {
            return Err(refused(format!("`{to}` cannot be a new {} name", self.prefix), "pick another name"));
        }
        Ok(new)
    }

    /// The change to every definition file that defines or refers to `old`, once `old` is
    /// known and `new` is free.
    fn renamed(&self, project: &Authoring, old: &str, new: &str) -> Result<Vec<FileChange>, EngineError> {
        if !self.defined(project, old)? {
            let reason = format!("there is no {} `{old}`", self.prefix);
            return Err(refused(reason, self.unknown_help));
        }
        if self.used(project, new)? {
            let help = format!("pick another name, or rename `{}.{new}` first", self.prefix);
            return Err(refused(format!("`{}.{new}` is already taken", self.prefix), &help));
        }
        let mut files = Vec::new();
        for found in project.entries().filter(|(_, file)| file.kind().is_some()) {
            let opened = Authoring::open(found)?;
            let mut edits = self.file_edits(&opened, old, new)?;
            if self.commented {
                edits.extend(opened.document.rename_in_comments((old, new), &is_generated));
            }
            if !edits.is_empty() {
                files.push(opened.change(&edits)?);
            }
        }
        Ok(files)
    }

    /// The edits in one file: the config's definition, and every reference.
    fn file_edits(&self, opened: &Opened<'_>, old: &str, new: &str) -> Result<Vec<TextEdit>, EngineError> {
        let document = &opened.document;
        let mut edits = (self.edits)(document, old, new).map_err(internal)?;
        if let Some(id) = self.definition(opened, old) {
            edits.push(document.set_scalar(id, new).map_err(internal)?);
        }
        Ok(edits)
    }

    /// The `id` that defines `name` in the config's list, when `opened` is the config.
    fn definition<'d>(&self, opened: &'d Opened<'_>, name: &str) -> Option<&'d Node> {
        let (list, maps) = self.listed.split_last()?;
        if opened.file.kind() != Some(FileKind::Config) {
            return None;
        }
        let parent = maps.iter().try_fold(opened.document.root(), |node, key| node.get(key))?;
        item(parent, list, name)?.get("id")
    }

    /// Whether `name` is defined: in the config's list, or for a var anywhere it is a key or
    /// used.
    fn defined(&self, project: &Authoring, name: &str) -> Result<bool, EngineError> {
        if self.listed.is_empty() {
            return self.used(project, name);
        }
        let config = project.entries().find(|(_, file)| file.kind() == Some(FileKind::Config));
        let opened = Authoring::open(config.ok_or_else(|| internal("a project without its config"))?)?;
        Ok(self.definition(&opened, name).is_some())
    }

    /// Whether some definition file defines or refers to `name`.
    fn used(&self, project: &Authoring, name: &str) -> Result<bool, EngineError> {
        for found in project.entries().filter(|(_, file)| file.kind().is_some()) {
            let opened = Authoring::open(found)?;
            if !self.file_edits(&opened, name, name)?.is_empty() {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

/// Whether `id` is a dataset ID: camelCase words joined with dots (`users.admins`).
fn is_dataset_id(id: &str) -> bool {
    id.split('.').all(is_camel_id)
}
