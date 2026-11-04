#![cfg(test)]

use std::collections::HashMap;

use proptest::prelude::*;
use serde_json::{Value, json};
use vc_model::{
    CelProgram, CelSource, Namespace, Path, Pos, Reference, ScalarSource, ScalarStyle, Segment, SourceFileId,
    SourceSpan, StringTable, Template, TemplateValue,
};
use vc_template::{
    Activation, CelCompiler, Chance, PipeRegistry, RenderError, Renderer, Resolve, Roots, extend, parse, path_text,
    preview, select, to_text,
};

/// Always picks `pick`, wrapped to the bound.
struct Fixed(usize);

impl Chance for Fixed {
    fn below(&self, bound: usize) -> usize {
        self.0.checked_rem(bound).unwrap_or(0)
    }
}

/// The roots of one test render: `var`, `response`…
struct Values {
    roots: HashMap<Namespace, Value>,
    chance: Fixed,
}

impl Values {
    fn new(roots: &[(Namespace, Value)]) -> Self {
        Self { roots: roots.iter().cloned().collect(), chance: Fixed(0) }
    }

    fn var(value: &Value) -> Self {
        Self::new(&[(Namespace::Var, json!({ "v": value }))])
    }
}

impl Resolve for Values {
    fn resolve(&self, reference: &Reference) -> Result<Value, RenderError> {
        let root = self.roots.get(&reference.ns).ok_or_else(|| RenderError::invalid("no such root"))?;
        let label = format!("{:?}", reference.ns).to_lowercase();
        select(&label, root, &reference.path.segments)
    }

    fn evaluate(&self, program: &CelProgram) -> Result<Value, RenderError> {
        let activation = self.roots.iter().fold(Activation::new(), |a, (ns, value)| a.with(*ns, value.clone()));
        let expr = CelCompiler::new().prepare(program).map_err(|error| RenderError::invalid(error.to_string()))?;
        expr.eval(&activation).map_err(|error| RenderError::invalid(error.to_string()))
    }

    fn chance(&self) -> &dyn Chance {
        &self.chance
    }
}

fn span(text: &str) -> SourceSpan {
    let end = Pos::new(1, 1 + u32::try_from(text.chars().count()).unwrap(), text.len());
    SourceSpan::new(SourceFileId::new(0), Pos::START, end)
}

fn template(text: &str) -> Template {
    parse(text, ScalarSource { span: span(text), style: ScalarStyle::Plain }, &mut StringTable::new()).unwrap()
}

fn render_with(text: &str, values: &Values) -> Result<Value, RenderError> {
    Renderer::new(&PipeRegistry::builtin(), values).template(&template(text))
}

fn body() -> Values {
    let body = json!({
        "items": [{"id": 1, "tags": ["a", "b"]}, {"id": 2, "tags": ["c"]}],
        "first name": "Ada",
        "n": 3,
    });
    Values::new(&[(Namespace::Response, json!({ "body": body }))])
}

#[test]
fn paths_follow_keys_indexes_stars_and_quoted_keys() {
    let cases = [
        ("${response.body.items[0].id}", json!(1)),
        ("${response.body.items[-1].id}", json!(2)),
        ("${response.body.items[-2].tags[1]}", json!("b")),
        ("${response.body.items[*].id}", json!([1, 2])),
        ("${response.body.items[*].tags[0]}", json!(["a", "c"])),
        ("${response.body[\"first name\"]}", json!("Ada")),
        ("${response.body['first name']}", json!("Ada")),
        ("${response.body.n}", json!(3)),
    ];
    for (text, expected) in cases {
        assert_eq!(render_with(text, &body()), Ok(expected), "{text}");
    }
}

#[test]
fn a_missing_path_names_the_path_and_the_value_where_it_stops() {
    let items = r#"[{"id":1,"tags":["a","b"]},{"id":2,"tags":["c"]}]"#;
    let cases = [
        ("${response.body.items[5]}", format!("`response.body.items[5]` is missing: `response.body.items` is {items}")),
        (
            "${response.body.items[-3]}",
            format!("`response.body.items[-3]` is missing: `response.body.items` is {items}"),
        ),
        ("${response.body.n.x}", "`response.body.n.x` is missing: `response.body.n` is 3".to_owned()),
        ("${response.body.n[*]}", "`response.body.n[*]` is missing: `response.body.n` is 3".to_owned()),
        (
            "${response.body.items[*].name}",
            r#"`response.body.items[*].name` is missing: `response.body.items[*]` is {"id":1,"tags":["a","b"]}"#
                .to_owned(),
        ),
    ];
    for (text, message) in cases {
        assert_eq!(render_with(text, &body()), Err(RenderError::missing(message)), "{text}");
    }
}

#[test]
fn paths_print_as_written_and_previews_are_cut() {
    let path = [Segment::Key("a".into()), Segment::Key("first name".into()), Segment::Index(-1), Segment::All];
    assert_eq!(path_text(&path), r#".a["first name"][-1][*]"#);
    assert_eq!(path_text(&[Segment::Key("_x9".into()), Segment::Key("9x".into())]), r#"._x9["9x"]"#);
    assert_eq!(path_text(&[Segment::Key(String::new())]), r#"[""]"#);
    let long = Value::String("é".repeat(200));
    let shown = preview(&long);
    assert_eq!(shown.chars().count(), 121);
    assert!(shown.ends_with('…') && shown.starts_with("\"é"));
    assert_eq!(preview(&json!([1])), "[1]");
    assert_eq!(select("x", &json!({"a": 1}), &[]), Ok(json!({"a": 1})));
}

#[test]
fn text_joins_literals_with_values_as_text() {
    let values = Values::new(&[(Namespace::Var, json!({"n": 5, "o": {"a": 1}, "s": "x", "z": null}))]);
    assert_eq!(render_with("id-${var.n}-${var.o}-${var.s}-${var.z}", &values), Ok(json!("id-5-{\"a\":1}-x-null")));
    assert_eq!(render_with("$${var.n}", &values), Ok(json!("${var.n}")));
    assert_eq!(render_with("${var.o}", &values), Ok(json!({"a": 1})));
    assert_eq!(render_with("", &values), Ok(json!("")));
    assert_eq!(to_text(&json!(true)), "true");
}

#[test]
fn pipes_convert_values() {
    let cases = [
        ("number", json!("42.5"), json!(42.5)),
        ("number", json!(" 7 "), json!(7)),
        ("number", json!(3), json!(3)),
        ("int", json!("42"), json!(42)),
        ("int", json!(42.9), json!(42)),
        ("int", json!("-3.7"), json!(-3)),
        ("int", json!(u64::MAX), json!(u64::MAX)),
        ("bool", json!("true"), json!(true)),
        ("bool", json!(" false"), json!(false)),
        ("bool", json!(true), json!(true)),
        ("string", json!(5), json!("5")),
        ("string", json!({"a": 1}), json!("{\"a\":1}")),
        ("string", json!("x"), json!("x")),
        ("json", json!("{\"a\":[1]}"), json!({"a": [1]})),
        ("jsonString", json!("a"), json!("\"a\"")),
        ("jsonString", json!([1, null]), json!("[1,null]")),
        ("base64", json!("hi"), json!("aGk=")),
        ("fromBase64", json!("aGk="), json!("hi")),
        ("lower", json!("AbC"), json!("abc")),
        ("upper", json!("AbC"), json!("ABC")),
        ("trim", json!("  a b \n"), json!("a b")),
        ("urlEncode", json!("a b&c/é~-._"), json!("a%20b%26c%2F%C3%A9~-._")),
        ("default('x')", json!(null), json!("x")),
        ("default('x')", json!("y"), json!("y")),
        ("default(0)", json!(false), json!(false)),
        ("take(2)", json!([1, 2, 3]), json!([1, 2])),
        ("take(5)", json!([1]), json!([1])),
        ("trim | upper", json!("  ab "), json!("AB")),
        ("jsonString | json", json!({"a": [true]}), json!({"a": [true]})),
    ];
    for (pipes, input, expected) in cases {
        let text = format!("${{var.v | {pipes}}}");
        assert_eq!(render_with(&text, &Values::var(&input)), Ok(expected), "{pipes} on {input}");
    }
}

#[test]
fn random_and_shuffle_follow_the_chance() {
    let mut values = Values::var(&json!([1, 2, 3]));
    values.chance = Fixed(1);
    assert_eq!(render_with("${var.v | random}", &values), Ok(json!(2)));
    assert_eq!(render_with("${var.v | shuffle}", &values), Ok(json!([1, 3, 2])));
    values.chance = Fixed(0);
    assert_eq!(render_with("${var.v | shuffle}", &values), Ok(json!([2, 3, 1])));
    values.chance = Fixed(4);
    assert_eq!(render_with("${var.v | shuffle}", &values), Ok(json!([3, 1, 2])));
    assert_eq!(render_with("${var.v | shuffle}", &Values::var(&json!([]))), Ok(json!([])));
}

#[test]
fn a_failed_pipe_names_the_pipe_the_value_and_why() {
    let cases = [
        ("number", json!("abc"), "pipe `number` cannot take \"abc\": the text is not a number"),
        ("number", json!(true), "pipe `number` cannot take true: it takes text or a number"),
        ("int", json!("1e30"), "pipe `int` cannot take \"1e30\": the number is too large for a whole number"),
        ("bool", json!("yes"), "pipe `bool` cannot take \"yes\": the text is not `true` or `false`"),
        ("bool", json!(1), "pipe `bool` cannot take 1: it takes `true` or `false`"),
        ("lower", json!(5), "pipe `lower` cannot take 5: it takes text"),
        ("random", json!([]), "pipe `random` cannot take []: the list is empty"),
        ("shuffle", json!("a"), "pipe `shuffle` cannot take \"a\": it takes a list"),
        ("fromBase64", json!([0xff]), "pipe `fromBase64` cannot take [255]: it takes text"),
        ("fromBase64", json!("/w=="), "pipe `fromBase64` cannot take \"/w==\": the decoded bytes are not UTF-8 text"),
    ];
    for (pipes, input, message) in cases {
        let text = format!("${{var.v | {pipes}}}");
        assert_eq!(render_with(&text, &Values::var(&input)), Err(RenderError::invalid(message)), "{pipes}");
    }
    let bad_json = render_with("${var.v | json}", &Values::var(&json!("{"))).unwrap_err().message;
    assert!(bad_json.starts_with("pipe `json` cannot take \"{\": the text is not JSON: "), "{bad_json}");
    let bad_base64 = render_with("${var.v | fromBase64}", &Values::var(&json!("!!"))).unwrap_err().message;
    assert!(bad_base64.starts_with("pipe `fromBase64` cannot take \"!!\": the text is not base64: "), "{bad_base64}");
}

#[test]
fn only_default_stands_in_for_a_missing_value() {
    let values = Values::var(&json!(1));
    assert_eq!(render_with("${var.gone | default('x') | upper}", &values), Ok(json!("X")));
    let missing = render_with("${var.gone | upper}", &values).unwrap_err();
    assert!(missing.missing);
    assert_eq!(missing.message, "`var.gone` is missing: `var` is {\"v\":1}");
    assert!(!render_with("${env.x | default('x')}", &values).unwrap_err().missing);
    assert!(!render_with("${var.v | lower}", &values).unwrap_err().missing);
}

#[test]
fn an_unknown_pipe_fails_at_run_time() {
    let renderer = PipeRegistry::default();
    let values = Values::var(&json!(1));
    let error = Renderer::new(&renderer, &values).template(&template("${var.v | trim}")).unwrap_err();
    assert_eq!(error, RenderError::invalid("unknown pipe `trim`"));
}

fn key(name: &str) -> Segment {
    Segment::Key(name.into())
}

fn string(text: &str) -> TemplateValue {
    TemplateValue::String(template(text))
}

#[test]
fn values_render_lists_maps_cel_and_extend() {
    let base = json!({
        "name": "a",
        "address": {"city": "x", "zip": "1"},
        "tags": [1, 2],
        "items": [{"id": 1, "secret": "s"}, {"id": 2, "secret": "t"}],
    });
    let values = Values::new(&[(Namespace::Var, json!({"body": base, "price": 2}))]);
    let changes = TemplateValue::Map(
        [
            ("address".to_owned(), TemplateValue::Map(std::iter::once(("city".to_owned(), string("y"))).collect())),
            ("tags".to_owned(), TemplateValue::List(vec![TemplateValue::Number(3.into()), TemplateValue::Null])),
            ("extra".to_owned(), TemplateValue::Null),
            ("on".to_owned(), TemplateValue::Bool(true)),
        ]
        .into_iter()
        .collect(),
    );
    let remove = vec![
        Path { segments: vec![key("name")] },
        Path { segments: vec![key("items"), Segment::All, key("secret")] },
        Path { segments: vec![key("tags"), Segment::Index(-1)] },
        Path { segments: vec![key("nothing"), key("here")] },
    ];
    let extended = TemplateValue::Extend { base: template("${var.body}"), changes: Box::new(changes), remove };
    let rendered = Renderer::new(&PipeRegistry::builtin(), &values).value(&extended);
    let expected = json!({
        "address": {"city": "y", "zip": "1"},
        "tags": [3],
        "items": [{"id": 1}, {"id": 2}],
        "extra": null,
        "on": true,
    });
    assert_eq!(rendered, Ok(expected));
    let source = CelSource { text: "var.price * 2".into(), span: span("var.price * 2"), style: ScalarStyle::Plain };
    let program = CelCompiler::new().check(&source, &Roots::new(), &mut StringTable::new()).unwrap();
    assert_eq!(Renderer::new(&PipeRegistry::builtin(), &values).value(&TemplateValue::Cel(program)), Ok(json!(4)));
}

#[test]
fn extend_merges_deeply_and_removes_what_it_finds() {
    let base = json!({"a": {"b": 1, "c": [1, 2, 3]}, "d": [{"e": 1}, 5], "f": 1});
    let changes = json!({"a": {"b": null, "g": 2}, "f": {"h": 1}});
    let remove = [
        Path { segments: vec![key("a"), key("c"), Segment::Index(0)] },
        Path { segments: vec![key("a"), key("c"), Segment::Index(7)] },
        Path { segments: vec![key("d"), Segment::Index(0), key("e")] },
        Path { segments: vec![key("d"), Segment::Index(-1), key("e")] },
        Path { segments: vec![key("d"), Segment::Index(-9), key("e")] },
        Path { segments: vec![key("f"), key("h"), key("i")] },
    ];
    let expected = json!({"a": {"b": null, "c": [2, 3], "g": 2}, "d": [{}, 5], "f": {"h": 1}});
    assert_eq!(extend(base, changes, &remove), expected);
    let cleared = extend(
        json!({"l": [1, 2], "m": {"n": 1}}),
        json!({}),
        &[Path { segments: vec![key("l"), Segment::All] }, Path { segments: vec![key("m"), Segment::All] }],
    );
    assert_eq!(cleared, json!({"l": [], "m": {"n": 1}}));
    assert_eq!(extend(json!([1]), json!({"a": 1}), &[]), json!({"a": 1}));
}

fn json_value() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(Value::from),
        "[a-z ]{0,8}".prop_map(Value::String),
    ];
    leaf.prop_recursive(3, 16, 4, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..4).prop_map(Value::Array),
            prop::collection::btree_map("[a-z]{1,4}", inner, 0..4)
                .prop_map(|map| Value::Object(map.into_iter().collect())),
        ]
    })
}

proptest! {
    #[test]
    fn a_lone_reference_keeps_its_type(value in json_value()) {
        prop_assert_eq!(render_with("${var.v}", &Values::var(&value)), Ok(value));
    }

    #[test]
    fn text_around_a_reference_makes_text(value in json_value()) {
        let expected = Value::String(format!("<{}>", to_text(&value)));
        prop_assert_eq!(render_with("<${var.v}>", &Values::var(&value)), Ok(expected));
    }

    #[test]
    fn json_and_base64_pipes_round_trip(value in json_value(), text in "\\PC{0,20}") {
        prop_assert_eq!(render_with("${var.v | jsonString | json}", &Values::var(&value)), Ok(value));
        let text = Value::String(text);
        prop_assert_eq!(render_with("${var.v | base64 | fromBase64}", &Values::var(&text)), Ok(text));
    }

    #[test]
    fn take_and_shuffle_keep_elements(items in prop::collection::vec(any::<i64>(), 0..8), pick in 0usize..9) {
        let mut values = Values::var(&Value::from(items.clone()));
        values.chance = Fixed(pick);
        let shuffled = render_with("${var.v | shuffle}", &values).unwrap();
        let mut sorted: Vec<i64> = serde_json::from_value(shuffled).unwrap();
        sorted.sort_unstable();
        let mut expected = items.clone();
        expected.sort_unstable();
        prop_assert_eq!(sorted, expected);
        let taken = render_with("${var.v | take(3)}", &values).unwrap();
        prop_assert_eq!(taken, Value::from(items.into_iter().take(3).collect::<Vec<_>>()));
    }
}
