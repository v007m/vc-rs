use super::{Place, cursor};

/// The cursor where `^` is in `text`, with the `^` taken out.
fn at(text: &str) -> super::Cursor {
    let offset = text.find('^').unwrap();
    cursor(&text.replace('^', ""), offset)
}

#[test]
fn finds_the_path_through_maps_and_list_items() {
    let found = at("tests:\n  - id: a\n    call: x\n  - id: b\n    expect:\n      sta^\n");
    assert_eq!(found.path, ["tests", "-", "expect"]);
    assert_eq!((found.place, found.word.as_str()), (Place::Key, "sta"));
    assert_eq!(found.siblings, Vec::<String>::new());
    let item = at("tests:\n  - id: a\n    call: x\n  - id: b\n    na^\n    tags: [x]\n");
    assert_eq!(item.path, ["tests", "-"]);
    assert_eq!(item.siblings, ["id", "tags"], "the other item's keys are not this one's");
}

#[test]
fn reads_a_new_item_and_a_list_written_at_its_keys_column() {
    let new = at("apis:\n  - id: list\n  - ^\n");
    assert_eq!((new.path.as_slice(), new.word.as_str()), (&["apis".to_owned(), "-".to_owned()][..], ""));
    let compact = at("tests:\n- id: a\n- id: b\n  ca^\n");
    assert_eq!(compact.path, ["tests", "-"]);
    assert_eq!(compact.siblings, ["id"]);
    let blank = at("syntaxVersion: 1\nvars:\n  a: 1\n^\n");
    assert_eq!(blank.path, Vec::<String>::new());
    assert_eq!(blank.siblings, ["vars", "syntaxVersion"]);
}

#[test]
fn tells_a_value_and_a_template_from_a_key() {
    let value = at("tests:\n  - call: \"users#cr^eate\"\n");
    assert_eq!((value.place, value.word.as_str(), value.value.as_str()), (Place::Value, "users#cr", "users#create"));
    assert_eq!(value.key.as_deref(), Some("call"));
    let template = at("apis:\n  - path: /users/${var.us^\n");
    assert_eq!((template.place, template.word.as_str()), (Place::Template, "var.us"));
    let closed = at("apis:\n  - path: /users/${var.userId}/^x\n");
    assert_eq!(closed.place, Place::Value);
    let on_key = at("apis:\n  - met^hod: GET\n");
    assert_eq!((on_key.place, on_key.key.as_deref()), (Place::Key, Some("method")));
}

#[test]
fn skips_comments_block_text_and_offsets_inside_a_character() {
    let found = at("tests:\n  # a: b\n  - id: a\n    notes: |\n      x: y\n    ^\n");
    assert_eq!(found.path, ["tests", "-"]);
    assert_eq!(found.siblings, ["notes", "id"]);
    let text = "name: é";
    assert_eq!(cursor(text, text.len() - 1).place, Place::Value);
    assert_eq!(cursor(text, 99).word, "é");
}
