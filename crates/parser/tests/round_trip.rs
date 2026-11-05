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
//! - `op: renameRef`, `fileId`, `from`, `to`: rename every `call`, `test` and `cleanup.call`
//!   reference to the full ID `from` in the file `fileId`, keeping short references short.
//!
//! `at` is a dotted path from the root; a number steps into a list. Every result must parse
//! again, and every edit, as an `EditCommand`, must undo back to the original bytes.

use std::fs;
use std::path::{Path, PathBuf};

use vc_model::SourceFileId;
use vc_parser::{Document, Node, Syntax};

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

/// The node at the dotted path `at`.
fn node<'d>(doc: &'d Document, at: &str) -> Result<&'d Node, String> {
    let step = |node: &'d Node, step: &str| match step.parse::<usize>() {
        Ok(index) => node.items().get(index),
        Err(_) => node.get(step),
    };
    let mut steps = at.split('.').filter(|step| !step.is_empty());
    steps.try_fold(doc.root(), |at_node, s| step(at_node, s)).ok_or_else(|| format!("no node at `{at}`"))
}

/// Applies the case's edit to `before` and returns the new text.
fn edit(case: &Case) -> Result<String, String> {
    let before = yaml(case.before.clone())?;
    let target = node(&before, field(case, "at").unwrap_or_default())?;
    let made = match field(case, "op")? {
        "set" => before.set_scalar(target, field(case, "value")?),
        "insert" => before.insert_entry(target, field(case, "key")?, field(case, "value")?),
        other => return Err(format!("`{other}` is not built yet (M5)")),
    };
    let edit = made.map_err(|e| e.to_string())?;
    before.apply(&[edit]).map(|after| after.text().to_owned()).map_err(|e| e.to_string())
}

/// Applies the case's edit as an `EditCommand`, then its inverse, and returns the text.
fn edit_then_undo(case: &Case) -> Result<String, String> {
    Err(format!("{}: `EditCommand` and its inverse are not built yet (M5)", case.name))
}

/// Runs `check` on every case and lists the ones that went wrong.
fn failures(check: impl Fn(&Case) -> Result<(), String>) -> Vec<String> {
    cases().iter().filter_map(|case| check(case).err().map(|why| format!("{}: {why}", case.name))).collect()
}

#[test]
fn there_is_a_case_for_every_operation() {
    let ops: Vec<String> = cases().iter().filter_map(|case| field(case, "op").ok().map(str::to_owned)).collect();
    for op in ["set", "insert", "insertItem", "remove", "renameRef"] {
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
