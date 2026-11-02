use vc_diagnostics::TextEdit;
use vc_model::{Pos, SourceFileId, SourceSpan};
use vc_parser::{Document, ParserError, ScalarKind};

const FILE: SourceFileId = SourceFileId::new(0);

fn doc(text: &str) -> Result<Document, ParserError> {
    Document::parse(FILE, text)
}

/// Sets `key` of the root map to `value` and returns the new text.
fn set(text: &str, key: &str, value: &str) -> Result<String, ParserError> {
    let before = doc(text)?;
    let node = before.root().get(key).ok_or_else(|| ParserError::Unparsable(format!("no key `{key}`")))?;
    let edit = before.set_scalar(node, value)?;
    Ok(before.apply(&[edit])?.text().to_owned())
}

/// Inserts `key: value` into the map under `path` (the root when empty) and returns the new text.
fn insert(text: &str, path: &[&str], (key, value): (&str, &str)) -> Result<String, ParserError> {
    let before = doc(text)?;
    let map = path
        .iter()
        .try_fold(before.root(), |node, step| node.get(step))
        .ok_or_else(|| ParserError::Unparsable("no map".into()))?;
    let edit = before.insert_entry(map, key, value)?;
    Ok(before.apply(&[edit])?.text().to_owned())
}

#[test]
fn setting_a_scalar_keeps_comments_kind_and_quote_style() {
    let before = doc("id: getsUser # id\nstatus: 200\nnote: 'old'\n").unwrap();
    let status = before.set_scalar(before.root().get("status").unwrap(), "201").unwrap();
    let note = before.set_scalar(before.root().get("note").unwrap(), "see #212").unwrap();
    let after = before.apply(&[note, status]).unwrap();
    assert_eq!(after.text(), "id: getsUser # id\nstatus: 201\nnote: 'see #212'\n");
    assert_eq!(after.root().get("note").unwrap().as_str(), Some("see #212"));
    assert_eq!(set("note: 'old'\n", "note", "it's").unwrap(), "note: 'it''s'\n");
    assert_eq!(set("note: \"old\"\n", "note", "new").unwrap(), "note: \"new\"\n");
    assert_eq!(set("name: old\n", "name", "true").unwrap(), "name: \"true\"\n");
    assert_eq!(set("on: false\n", "on", "true").unwrap(), "on: true\n");
    assert_eq!(set("a: ~\n", "a", "5").unwrap(), "a: 5\n");
}

#[test]
fn a_kind_change_must_be_asked_for() {
    let not_an_int = ParserError::NotOfKind { value: "abc".into(), kind: "an integer" };
    assert_eq!(set("status: 200\n", "status", "abc").unwrap_err(), not_an_int);
    let before = doc("status: 200\nn: \"12\"\n").unwrap();
    let text = before.set_typed(before.root().get("status").unwrap(), "201", ScalarKind::String).unwrap();
    let int = before.set_typed(before.root().get("n").unwrap(), "13", ScalarKind::Int).unwrap();
    assert_eq!(before.apply(&[text, int]).unwrap().text(), "status: \"201\"\nn: 13\n");
}

#[test]
fn setting_an_empty_value_writes_after_its_colon() {
    assert_eq!(set("a:\nb: 1\n", "a", "x").unwrap(), "a: x\nb: 1\n");
    assert_eq!(set("a:  # why\nb: 1\n", "a", "x").unwrap(), "a: x  # why\nb: 1\n");
    assert_eq!(set("a :\n", "a", "12").unwrap(), "a : 12\n");
    let empty_item = doc("list:\n  -\n  - 1\n").unwrap();
    let item = &empty_item.root().get("list").unwrap().items()[0];
    assert!(matches!(empty_item.set_scalar(item, "x"), Err(ParserError::WrongNode { .. })));
}

#[test]
fn an_empty_document_is_a_null_root_that_edits_refuse() {
    for text in ["", "# header\n"] {
        let empty = doc(text).unwrap();
        assert!(matches!(empty.set_scalar(empty.root(), "x"), Err(ParserError::WrongNode { .. })), "{text:?}");
        assert!(matches!(empty.insert_entry(empty.root(), "a", "x"), Err(ParserError::WrongNode { .. })));
        assert_eq!(empty.text(), text);
    }
}

#[test]
fn values_inside_flow_collections_quote_flow_indicators() {
    let before = doc("tags: [a, b]\nname: n\n").unwrap();
    let edit = before.set_scalar(&before.root().get("tags").unwrap().items()[0], "x, y").unwrap();
    let after = before.apply(&[edit]).unwrap();
    assert_eq!(after.text(), "tags: [\"x, y\", b]\nname: n\n");
    assert_eq!(after.root().get("tags").unwrap().items().len(), 2);
    assert_eq!(
        set("m: {k: v}\n", "m", "x").unwrap_err(),
        ParserError::WrongNode { expected: "a plain or quoted scalar", found: "a map" }
    );
    assert_eq!(set("name: n\n", "name", "x, y").unwrap(), "name: x, y\n");
}

#[test]
fn inserting_after_a_block_scalar_keeps_the_next_line() {
    let nested = "api:\n  body: |\n    hi\nname: x\n";
    assert_eq!(insert(nested, &["api"], ("auth", "none")).unwrap(), "api:\n  body: |\n    hi\n  auth: none\nname: x\n");
    let deeper = "api:\n  body: |\n    hi\n  next: 1\n";
    assert_eq!(
        insert(deeper, &["api"], ("auth", "none")).unwrap(),
        "api:\n  body: |\n    hi\n  next: 1\n  auth: none\n"
    );
    assert_eq!(insert("a: 1\nbody: |\n  hi\n", &[], ("b", "x")).unwrap(), "a: 1\nbody: |\n  hi\nb: x\n");
    let kept = "a: 1\nbody: |+\n  hi\n\n";
    assert_eq!(insert(kept, &[], ("b", "x")).unwrap(), "a: 1\nbody: |+\n  hi\n\nb: x\n");
    let after_block = doc("api:\n  body: |\n    hi\n  b: 1\n").unwrap();
    let edit = after_block.insert_entry(after_block.root().get("api").unwrap(), "c", "x").unwrap();
    assert_eq!(edit.range.start, "api:\n  body: |\n    hi\n  b: 1\n".len());
}

#[test]
fn inserting_an_existing_key_is_refused() {
    let text = "api:\n  path: /users\n";
    assert_eq!(insert(text, &["api"], ("path", "/x")).unwrap_err(), ParserError::DuplicateKey("path".into()));
}

#[test]
fn nodes_from_another_or_an_older_document_are_refused() {
    let before = doc("a: 1\n").unwrap();
    let node = before.root().get("a").unwrap();
    let after = before.apply(&[before.set_scalar(node, "22").unwrap()]).unwrap();
    assert_eq!(after.set_scalar(node, "3").unwrap_err(), ParserError::BadRange { start: 3, end: 4 });
    assert!(matches!(after.insert_entry(before.root(), "b", "2"), Err(ParserError::BadRange { .. })));
    let other_file = Document::parse(SourceFileId::new(1), "a: 1\n").unwrap();
    assert!(matches!(other_file.set_scalar(node, "3"), Err(ParserError::BadRange { .. })));
    assert!(before.set_scalar(node, "3").is_ok());
    let same_span = before.apply(&[before.set_scalar(node, "2").unwrap()]).unwrap();
    assert_eq!(same_span.set_scalar(node, "3").unwrap_err(), ParserError::BadRange { start: 3, end: 4 });
}

#[test]
fn inserted_lines_use_the_file_line_ending() {
    assert_eq!(insert("a: 1\r\nb: 2\r\n", &[], ("c", "x")).unwrap(), "a: 1\r\nb: 2\r\nc: x\r\n");
    assert_eq!(insert("a: 1\r\nb: 2", &[], ("c", "x")).unwrap(), "a: 1\r\nb: 2\r\nc: x\r\n");
    let nested = "api:\r\n  path: /u # p\r\nname: x\r\n";
    assert_eq!(
        insert(nested, &["api"], ("auth", "none")).unwrap(),
        "api:\r\n  path: /u # p\r\n  auth: none\r\nname: x\r\n"
    );
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
