use vc_diagnostics::{Code, Diagnostic, Diagnostics};
use vc_model::{Pos, SourceFileId, SourcePath, SourceSpan};
use vc_parser::{Node, ScalarKind, ScalarStyle, Syntax, Value, parse};

fn json(text: &str) -> (Option<Node>, Diagnostics) {
    parse(SourceFileId::new(0), Syntax::Json, text)
}

fn ok(text: &str) -> Option<Node> {
    let (root, diagnostics) = json(text);
    assert!(diagnostics.is_empty(), "{text:?}: {diagnostics:?}");
    root
}

/// The one diagnostic for `text`, which must drop the tree.
fn error(text: &str) -> Option<Diagnostic> {
    let (root, diagnostics) = json(text);
    assert!(root.is_none(), "{text:?}");
    let all: Vec<_> = diagnostics.iter().cloned().collect();
    assert_eq!(all.len(), 1, "{text:?}: {all:?}");
    all.into_iter().next()
}

type Place = (u32, u32, usize);

fn at(span: SourceSpan) -> (Place, Place) {
    let tuple = |p: Pos| (p.line, p.column, p.offset);
    (tuple(span.start), tuple(span.end))
}

fn kind(node: &Node) -> Option<ScalarKind> {
    node.as_scalar().map(|scalar| scalar.kind)
}

#[test]
fn reads_objects_lists_and_scalars_into_the_tree() {
    let root = ok(r#"{"a":-0,"d":"x\"y\u00e9","e":{},"f":[1, "two", null],"h i":true,"g":-2.5E+3}"#).unwrap();
    let keys: Vec<_> = root.entries().iter().map(|e| e.key.as_str()).collect();
    assert_eq!(keys, ["a", "d", "e", "f", "h i", "g"]);
    assert_eq!(root.get("d").unwrap().as_str(), Some("x\"yé"));
    assert_eq!(root.get("d").unwrap().as_scalar().unwrap().style, ScalarStyle::DoubleQuoted);
    assert_eq!(root.get("e").unwrap().value, Value::Map(Vec::new()));
    let f = root.get("f").unwrap();
    assert_eq!(
        f.items().iter().map(kind).collect::<Vec<_>>(),
        [Some(ScalarKind::Int), Some(ScalarKind::String), Some(ScalarKind::Null)]
    );
    assert_eq!(kind(root.get("a").unwrap()), Some(ScalarKind::Int));
    assert_eq!(kind(root.get("h i").unwrap()), Some(ScalarKind::Bool));
    assert_eq!(root.get("g").unwrap().as_scalar().unwrap().text, "-2.5E+3");
    assert_eq!(kind(root.get("g").unwrap()), Some(ScalarKind::Float));
}

#[test]
fn json_spans_cover_the_whole_value_quotes_and_brackets_included() {
    let root = ok(r#"{"a":-0,"d":"x\"y\u00e9","e":{},"f":[1, "two", null],"h i":true,"g":-2.5E+3}"#).unwrap();
    let f = root.get("f").unwrap();
    assert_eq!(at(root.get("d").unwrap().span), ((1, 13, 12), (1, 25, 24)));
    assert_eq!(at(root.entries()[1].key_span), ((1, 9, 8), (1, 12, 11)));
    assert_eq!(at(f.span), ((1, 37, 36), (1, 53, 52)));
    assert_eq!(at(root.span), ((1, 1, 0), (1, 77, 76)));
    assert_eq!(ok("[1]\n").unwrap().items().len(), 1);
    assert_eq!(ok(" \"top\" ").unwrap().as_str(), Some("top"));
}

#[test]
fn spans_count_characters_for_columns_and_bytes_for_offsets_across_crlf() {
    let root = ok("{\r\n  \"név\": \"ő\",\r\n  \"b\": [1, \"é\"]\r\n}\r\n").unwrap();
    assert_eq!(at(root.entries()[0].key_span), ((2, 3, 5), (2, 8, 11)));
    assert_eq!(at(root.get("név").unwrap().span), ((2, 10, 13), (2, 13, 17)));
    assert_eq!(at(root.entries()[1].key_span), ((3, 3, 22), (3, 6, 25)));
    assert_eq!(at(root.get("b").unwrap().span), ((3, 8, 27), (3, 16, 36)));
    assert_eq!(at(root.get("b").unwrap().items()[1].span), ((3, 12, 31), (3, 15, 35)));
    assert_eq!(at(root.span), ((1, 1, 0), (4, 2, 39)));
    let with_bom = ok("\u{feff}{\"a\": 1}").unwrap();
    assert_eq!(at(with_bom.entries()[0].key_span), ((1, 2, 4), (1, 5, 7)));
}

#[test]
fn escapes_and_surrogate_pairs_are_decoded() {
    let root = ok(r#"["\ud83d\ude00", "\"\\\/\b\f\n\r\t", "\u00E9\u0041", "😀"]"#).unwrap();
    let texts: Vec<_> = root.items().iter().map(|item| item.as_str().unwrap()).collect();
    assert_eq!(texts, ["😀", "\"\\/\u{8}\u{c}\n\r\t", "éA", "😀"]);
}

#[test]
fn a_lone_surrogate_is_v1001_on_its_escape() {
    for text in [r#"["\ud83d"]"#, r#"["\ude00"]"#, r#"["\ud83dx"]"#, r#"["\ud83d\u0041"]"#, r#"["\ud83d\ud83d"]"#] {
        let error = error(text).unwrap();
        assert_eq!((error.code, at(error.primary.span)), (Code::V1001, ((1, 3, 2), (1, 9, 8))), "{text}");
        assert!(error.message.contains("half of a surrogate pair"), "{}", error.message);
        assert!(error.help[0].contains(r"\ud83d\ude00"));
    }
}

#[test]
fn trailing_commas_are_v1001_on_the_comma() {
    for (text, column) in [("[1,]", 3), ("{\"a\": 1 , }", 9), ("[[],\n]", 4)] {
        let error = error(text).unwrap();
        assert_eq!((error.code, error.primary.span.start.column), (Code::V1001, column), "{text:?}");
        assert_eq!(error.help, ["remove the comma after the last item"]);
    }
}

#[test]
fn syntax_errors_are_v1001_with_a_precise_span_and_message() {
    let cases = [
        (r#"{"a" 1}"#, 5..6, "expected `:`, found `1`"),
        ("{a: 1}", 1..2, "expected a key in double quotes, found `a`"),
        ("{'a': 1}", 1..2, "expected a key in double quotes, found `'`"),
        ("[1 2]", 3..4, "expected `,` or `]`, found `2`"),
        (r#"{"a":1 "b":2}"#, 7..8, "expected `,` or `}`, found `\"`"),
        ("[1", 2..2, "the file ends before `,` or `]`"),
        (r#"{"a":"#, 5..5, "the file ends before a value"),
        (r#"{"a":}"#, 5..6, "expected a value, found `}`"),
        ("[,1]", 1..2, "expected a value, found `,`"),
        ("[1,,2]", 3..4, "expected a value, found `,`"),
        ("1 2", 2..3, "expected the end of the file, found `2`"),
        ("// note\n1", 0..1, "expected a value, found `/`"),
        ("+1", 0..1, "expected a value, found `+`"),
        ("True", 0..4, "`True` is not a JSON value"),
        ("[yes]", 1..4, "`yes` is not a JSON value"),
        ("01", 0..2, "not a valid number"),
        ("[1.]", 1..4, "not a valid number"),
        ("-", 0..1, "not a valid number"),
        ("1e+x", 0..4, "not a valid number"),
        (r#""abc"#, 0..1, "the string is never closed"),
        ("\"a\nb\"", 2..3, "a line break inside a string"),
        ("\"a\tb\"", 2..3, "the control character `\\t` inside a string"),
        (r#""\x""#, 1..3, r"`\x` is not a JSON escape"),
        (r#""\"#, 1..2, r"`\` is not a JSON escape"),
        (r#""\u12g4""#, 1..6, r"`\u` needs four hex digits"),
        (r#""\u12"#, 1..5, r"`\u` needs four hex digits"),
        (r#""\ud83d\u12""#, 7..12, r"`\u` needs four hex digits"),
    ];
    for (text, range, message) in cases {
        let error = error(text).unwrap();
        assert_eq!(error.code, Code::V1001, "{text:?}");
        assert_eq!(error.primary.span.byte_range(), range, "{text:?}");
        assert_eq!(error.message, format!("not valid JSON: {message}"), "{text:?}");
        assert_eq!(error.help.len(), 1, "{text:?}");
        assert_eq!(error.primary.text, "here");
    }
}

#[test]
fn blank_files_are_a_null_root_as_in_yaml() {
    for text in ["", " \r\n\t", "\u{feff}"] {
        let root = ok(text).unwrap();
        assert_eq!(kind(&root), Some(ScalarKind::Null));
        assert_eq!(root, parse(SourceFileId::new(0), Syntax::Yaml, "").0.unwrap());
    }
}

#[test]
fn every_duplicate_key_is_v1006_and_the_first_one_is_kept() {
    let (root, diagnostics) = json(r#"{"a": 1, "b": 2, "a": [3, {"x": 1}], "b": 4}"#);
    let root = root.unwrap();
    let codes: Vec<_> = diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(codes, [Code::V1006, Code::V1006]);
    let first = diagnostics.iter().next().unwrap();
    assert_eq!(at(first.primary.span), ((1, 18, 17), (1, 21, 20)));
    assert_eq!(at(first.secondary[0].span), ((1, 2, 1), (1, 5, 4)));
    assert_eq!(root.entries().len(), 2);
    assert_eq!(root.get("a").unwrap().as_scalar().unwrap().text, "1");
    assert_eq!(at(root.span).1, (1, 45, 44));
}

#[test]
fn nesting_deeper_than_the_limit_is_v1011() {
    let lists = format!("{}{}", "[".repeat(129), "]".repeat(129));
    let error = error(&lists).unwrap();
    assert_eq!((error.code, at(error.primary.span)), (Code::V1011, ((1, 129, 128), (1, 130, 129))));
    let objects = format!("{}1{}", r#"{"a":"#.repeat(200), "}".repeat(200));
    assert_eq!(error_code(&objects), Some(Code::V1011));
    assert!(json(&format!("{}{}", "[".repeat(128), "]".repeat(128))).0.is_some());
}

fn error_code(text: &str) -> Option<Code> {
    error(text).map(|error| error.code)
}

#[test]
fn json_files_are_found_by_suffix() {
    let syntax = |path| Syntax::of(&SourcePath::new(path).unwrap());
    assert_eq!(syntax("schemas/user.json"), Syntax::Json);
    assert_eq!(syntax(".json"), Syntax::Json);
    for path in ["services/users/users.api.yaml", "datasets/cities.json.yaml", "vibranium.config.yaml", "json"] {
        assert_eq!(syntax(path), Syntax::Yaml, "{path}");
    }
}

/// A node with its spans and styles left out, so trees from different syntaxes compare.
#[derive(Debug, PartialEq)]
enum Plain {
    Scalar(String, ScalarKind),
    Seq(Vec<Self>),
    Map(Vec<(String, Self)>),
}

fn plain(node: &Node) -> Plain {
    match &node.value {
        Value::Scalar(scalar) => Plain::Scalar(scalar.text.clone(), scalar.kind),
        Value::Seq(items) => Plain::Seq(items.iter().map(plain).collect()),
        Value::Map(entries) => Plain::Map(entries.iter().map(|e| (e.key.clone(), plain(&e.value))).collect()),
    }
}

#[test]
fn json_and_yaml_for_the_same_data_give_the_same_tree() {
    let json_text = r#"{
  "id": "getsUser",
  "retries": 2,
  "ratio": -0.5e3,
  "on": true,
  "none": null,
  "tags": ["a b", "😀", "é\n"],
  "empty": {"list": [], "map": {}},
  "nested": [{"k": 1}, [false]]
}"#;
    let yaml_text = "id: getsUser\nretries: 2\nratio: -0.5e3\non: true\nnone: null\n\
        tags:\n  - a b\n  - 😀\n  - \"é\\n\"\nempty:\n  list: []\n  map: {}\nnested:\n  - k: 1\n  - [false]\n";
    let (yaml, diagnostics) = parse(SourceFileId::new(0), Syntax::Yaml, yaml_text);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(plain(&ok(json_text).unwrap()), plain(&yaml.unwrap()));
}
