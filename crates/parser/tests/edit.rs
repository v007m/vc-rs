use vc_diagnostics::TextEdit;
use vc_model::{Pos, SourceFileId, SourceSpan};
use vc_parser::{Document, ParserError};

const FILE: SourceFileId = SourceFileId::new(0);

fn doc(text: &str) -> Result<Document, ParserError> {
    Document::parse(FILE, text)
}

#[test]
fn setting_a_scalar_keeps_comments_and_quotes_only_when_needed() {
    let before = doc("id: getsUser # id\nstatus: 200\nnote: 'old'\n").unwrap();
    let status = before.set_scalar(before.root().get("status").unwrap(), "201").unwrap();
    let note = before.set_scalar(before.root().get("note").unwrap(), "see #212").unwrap();
    let after = before.apply(&[note, status]).unwrap();
    assert_eq!(after.text(), "id: getsUser # id\nstatus: \"201\"\nnote: \"see #212\"\n");
    assert_eq!(after.root().get("note").unwrap().as_str(), Some("see #212"));
}

#[test]
fn inserting_an_entry_lines_up_with_the_map_keys() {
    let before = doc("api:\n  path: /users # list\n  method: GET\nname: x\n").unwrap();
    let edit = before.insert_entry(before.root().get("api").unwrap(), "auth", "none").unwrap();
    let after = before.apply(&[edit]).unwrap();
    assert_eq!(after.text(), "api:\n  path: /users # list\n  method: GET\n  auth: none\nname: x\n");
}

#[test]
fn inserting_into_a_file_without_a_final_newline_starts_a_line() {
    let before = doc("a: 1").unwrap();
    let edit = before.insert_entry(before.root(), "b", "true").unwrap();
    assert_eq!(before.apply(&[edit]).unwrap().text(), "a: 1\nb: \"true\"\n");
}

#[test]
fn edits_refuse_the_wrong_kind_of_node() {
    let before = doc("list: [1]\nflow: {a: 1}\nblock: |\n  x\n").unwrap();
    let wrong = |result: Result<TextEdit, ParserError>| matches!(result, Err(ParserError::WrongNode { .. }));
    assert!(wrong(before.set_scalar(before.root().get("list").unwrap(), "x")));
    assert!(wrong(before.set_scalar(before.root().get("block").unwrap(), "x")));
    assert!(wrong(before.insert_entry(before.root().get("flow").unwrap(), "b", "2")));
    assert!(wrong(before.insert_entry(before.root().get("list").unwrap(), "b", "2")));
}

#[test]
fn apply_rejects_bad_ranges_and_results_that_do_not_parse() {
    let before = doc("a: é\n").unwrap();
    let span = |start, end| SourceSpan::new(FILE, Pos::new(1, 1, start), Pos::new(1, 1, end));
    let outside = TextEdit::replace(span(3, 99), "x");
    assert_eq!(before.apply(&[outside]).unwrap_err(), ParserError::BadRange { start: 3, end: 99 });
    let splits_a_char = TextEdit::replace(span(4, 5), "x");
    assert!(matches!(before.apply(&[splits_a_char]), Err(ParserError::BadRange { .. })));
    let overlapping = [TextEdit::replace(span(0, 2), "b"), TextEdit::replace(span(1, 3), "c")];
    assert!(matches!(before.apply(&overlapping), Err(ParserError::BadRange { .. })));
    let breaks = TextEdit::replace(span(3, 5), "[");
    assert!(matches!(before.apply(&[breaks]), Err(ParserError::Unparsable(_))));
    assert!(matches!(Document::parse(FILE, "a: [\n"), Err(ParserError::Unparsable(_))));
}
