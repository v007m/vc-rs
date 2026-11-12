//! M5 acceptance: every `parser::edit` operation round-trips byte for byte.
//!
//! Each folder under `tests/edits/` is one case: `before.yaml`, the edit in `op.yaml`, and the
//! exact text expected in `after.yaml`. `op.yaml` holds:
//!
//! - `op: set`, `at`, `value`: replace a scalar, keeping its quote style and comment.
//! - `op: insert`, `at`, `key`, `value`: add a map entry after the map's last entry.
//! - `op: insertItem`, `at`, `value`: add a list item after the last one, in house style
//!   (block YAML, 2-space indent, templates unquoted); a flow list of scalars stays flow.
//! - `op: remove`, `at`: remove a map entry or list item with its lines, and the comment lines
//!   directly above a list item.
//! - `op: moveItem`, `at`, `from`, `to`: move a list item, with the comment lines above it, to
//!   another place in its list; the lines between items keep their places.
//! - `op: renameRef`, `fileId`, `from`, `to`: rename every `call`, `test` and `cleanup.call`
//!   reference to the full ID `from` in the file `fileId`, keeping short references short.
//!
//! `at` is a dotted path from the root; a number steps into a list. Every result must parse
//! again, and every edit, as an `EditCommand`, must undo back to the original bytes.

#![cfg(test)]

use std::fs;
use std::path::{Path, PathBuf};

use vc_diagnostics::TextEdit;
use vc_model::SourceFileId;
use vc_parser::{Document, EditCommand, Node, Syntax};

const FILE: SourceFileId = SourceFileId::new(0);

/// One case folder, read.
struct Case {
    name: String,
    before: String,
    op: Document,
    after: String,
}

fn read(folder: &Path, file: &str) -> String {
    fs::read_to_string(folder.join(file)).unwrap_or_else(|e| panic!("{}/{file}: {e}", folder.display()))
}

fn yaml(text: String) -> Result<Document, String> {
    Document::parse(FILE, Syntax::Yaml, text).map_err(|e| e.to_string())
}

fn cases() -> Vec<Case> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/edits");
    let mut folders: Vec<PathBuf> = fs::read_dir(&root).unwrap().map(|entry| entry.unwrap().path()).collect();
    folders.sort();
    let case = |folder: PathBuf| Case {
        name: folder.file_name().unwrap_or_default().to_string_lossy().into_owned(),
        before: read(&folder, "before.yaml"),
        op: yaml(read(&folder, "op.yaml")).unwrap_or_else(|e| panic!("{}: op.yaml: {e}", folder.display())),
        after: read(&folder, "after.yaml"),
    };
    folders.into_iter().map(case).collect()
}

/// The text of `key` in the case's `op.yaml`.
fn field<'c>(case: &'c Case, key: &str) -> Result<&'c str, String> {
    case.op.root().get(key).and_then(Node::as_str).ok_or_else(|| format!("op.yaml has no `{key}`"))
}

/// The number at `key` in the case's `op.yaml`.
fn index(case: &Case, key: &str) -> Result<usize, String> {
    let text = case.op.root().get(key).and_then(Node::as_scalar).map(|scalar| scalar.text.as_str());
    text.and_then(|text| text.parse().ok()).ok_or_else(|| format!("op.yaml has no number `{key}`"))
}

/// The node at the dotted path `at`.
fn node<'d>(doc: &'d Document, at: &str) -> Result<&'d Node, String> {
    let step = |node: &'d Node, step: &str| {
        step.parse::<usize>().map_or_else(|_| node.get(step), |index| node.items().get(index))
    };
    let mut steps = at.split('.').filter(|step| !step.is_empty());
    steps.try_fold(doc.root(), step).ok_or_else(|| format!("no node at `{at}`"))
}

/// The case's edits on `before`.
/// The edits of a rename case, or `None` for another operation.
fn renames(case: &Case, before: &Document) -> Option<Result<Vec<TextEdit>, String>> {
    let names = || Ok::<_, String>((field(case, "from")?, field(case, "to")?));
    let made = match field(case, "op").ok()? {
        "renameRef" => names().and_then(|(from, to)| Ok(before.rename_refs(field(case, "fileId")?, from, to))),
        "renameVar" => names().map(|(from, to)| before.rename_var(from, to)),
        "renameDataset" => names().map(|(from, to)| before.rename_dataset(from, to)),
        "renameAuth" => names().map(|(from, to)| before.rename_auth(from, to)),
        _ => return None,
    };
    Some(made.and_then(|made| made.map_err(|e| e.to_string())))
}

fn edits(case: &Case, before: &Document) -> Result<Vec<TextEdit>, String> {
    if let Some(made) = renames(case, before) {
        return made;
    }
    let op = field(case, "op")?;
    let target = node(before, field(case, "at").unwrap_or_default())?;
    let made = match op {
        "set" => before.set_scalar(target, field(case, "value")?),
        "insert" => before.insert_entry(target, field(case, "key")?, field(case, "value")?),
        "insertItem" => before.insert_item(target, field(case, "value")?),
        "remove" => before.remove(target),
        "moveItem" => before.move_item(target, index(case, "from")?, index(case, "to")?),
        other => return Err(format!("no op `{other}`")),
    };
    made.map(|edit| vec![edit]).map_err(|e| e.to_string())
}

/// Applies the case's edit to `before` and returns the new text.
fn edit(case: &Case) -> Result<String, String> {
    let before = yaml(case.before.clone())?;
    let made = edits(case, &before)?;
    before.apply(&made).map(|after| after.text().to_owned()).map_err(|e| e.to_string())
}

/// Applies the case's edit as an `EditCommand`, then its inverse, and returns the text.
fn edit_then_undo(case: &Case) -> Result<String, String> {
    let before = yaml(case.before.clone())?;
    let command = EditCommand::new(edits(case, &before)?);
    let (after, undo) = command.apply(&before).map_err(|e| e.to_string())?;
    let (back, _) = undo.apply(&after).map_err(|e| e.to_string())?;
    Ok(back.text().to_owned())
}

/// Runs `check` on every case and lists the ones that went wrong.
fn failures(check: impl Fn(&Case) -> Result<(), String>) -> Vec<String> {
    cases().iter().filter_map(|case| check(case).err().map(|why| format!("{}: {why}", case.name))).collect()
}

#[test]
fn there_is_a_case_for_every_operation() {
    let ops: Vec<String> = cases().iter().filter_map(|case| field(case, "op").ok().map(str::to_owned)).collect();
    for op in
        ["set", "insert", "insertItem", "remove", "moveItem", "renameRef", "renameVar", "renameDataset", "renameAuth"]
    {
        assert!(ops.iter().any(|seen| seen == op), "no case for `{op}`");
    }
}

#[test]
fn every_edit_gives_the_expected_bytes() {
    let wrong = failures(|case| {
        let text = edit(case)?;
        if text == case.after { Ok(()) } else { Err(format!("got\n{text}\nexpected\n{}", case.after)) }
    });
    assert!(wrong.is_empty(), "{}", wrong.join("\n\n"));
}

#[test]
fn every_edit_undoes_to_the_original_bytes() {
    let wrong = failures(|case| {
        let text = edit_then_undo(case)?;
        if text == case.before { Ok(()) } else { Err(format!("undo gave\n{text}")) }
    });
    assert!(wrong.is_empty(), "{}", wrong.join("\n\n"));
}

#[test]
fn every_expected_text_parses() {
    let wrong = failures(|case| yaml(case.after.clone()).map(|_| ()));
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
