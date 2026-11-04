#![cfg(test)]

use std::ops::Range;

use serde_json::{Value, json};
use vc_diagnostics::{Code, Diagnostic};
use vc_model::{CelProgram, CelSource, Namespace, Pos, ScalarStyle, Segment, SourceFileId, SourceSpan, StringTable};
use vc_template::{Activation, CelCompiler, DEFAULT_STEP_LIMIT, ExprError, Roots, TemplateError};

fn source(text: &str) -> CelSource {
    let end = Pos::new(1, 1 + u32::try_from(text.chars().count()).unwrap(), text.len());
    CelSource {
        text: text.into(),
        span: SourceSpan::new(SourceFileId::new(0), Pos::START, end),
        style: ScalarStyle::Plain,
    }
}

fn check_in(roots: &Roots, text: &str) -> Result<CelProgram, Box<Diagnostic>> {
    CelCompiler::new().check(&source(text), roots, &mut StringTable::new()).map_err(|error| match error {
        TemplateError::Syntax(diagnostic) => diagnostic,
        other => panic!("{other:?}"),
    })
}

fn check(text: &str) -> Result<CelProgram, Box<Diagnostic>> {
    check_in(&Roots::new().with_call(), text)
}

fn error(text: &str) -> (Code, String, Range<usize>) {
    let diagnostic = *check(text).unwrap_err();
    (diagnostic.code, diagnostic.message, diagnostic.primary.span.byte_range())
}

fn eval_with(text: &str, activation: &Activation) -> Result<Value, ExprError> {
    let compiler = CelCompiler::new();
    let program = check(text).unwrap();
    compiler.prepare(&program).unwrap().eval(activation)
}

fn response(body: &Value) -> Activation {
    Activation::new().with(Namespace::Response, json!({"status": 200, "headers": {"etag": "x"}, "body": body}))
}

fn eval(text: &str, body: &Value) -> Result<Value, ExprError> {
    eval_with(
        text,
        &response(body).with(Namespace::Var, json!({"price": 2, "name": "ada", "big": u64::MAX, "ratio": 0.5})),
    )
}

fn key(name: &str) -> Segment {
    Segment::Key(name.into())
}

#[test]
fn check_finds_each_root_with_its_path_and_span() {
    let text = "response.body.items[0][\"id\"] == var.userId && size(request . url) > 0";
    let program = check(text).unwrap();
    let found: Vec<_> = program.refs.iter().map(|r| (r.ns, r.path.segments.clone(), r.span.byte_range())).collect();
    assert_eq!(
        found,
        [
            (Namespace::Response, vec![key("body"), key("items"), Segment::Index(0), key("id")], 0..28),
            (Namespace::Var, vec![key("userId")], 32..42),
            (Namespace::Request, vec![key("url")], 51..64),
        ]
    );
    assert_eq!(program.refs[1].root_span.byte_range(), 32..35);
    assert_eq!(program.source, source(text));
}

#[test]
fn check_skips_comprehension_vars_builtins_and_computed_indexes() {
    let program = check("response.body.items.all(i, type(i.id) == int && dyn(var.x) != null)").unwrap();
    let roots: Vec<_> = program.refs.iter().map(|r| r.ns).collect();
    assert_eq!(roots, [Namespace::Response, Namespace::Var]);
    let program = check("{var.k: request.url}.size() == 1").unwrap();
    let roots: Vec<_> = program.refs.iter().map(|r| r.ns).collect();
    assert_eq!(roots, [Namespace::Var, Namespace::Request]);
    let program = check("{'a': 1}.exists(k, var.list[k] == 1) || response.body[var.key].x").unwrap();
    let paths: Vec<_> = program.refs.iter().map(|r| r.path.segments.clone()).collect();
    assert_eq!(paths, [vec![key("list")], vec![key("body")], vec![key("key")]]);
    let program = check("response.body.items.map(i, i).filter(j, j > i)").unwrap_err();
    assert_eq!((program.code, program.primary.span.byte_range()), (Code::V2105, 44..45));
}

#[test]
fn check_reads_step_ids_and_loop_roots_where_available() {
    let mut strings = StringTable::new();
    let login = strings.intern("login").unwrap();
    let roots = Roots::new().with_step(login).in_for_each().with_examples();
    let compiler = CelCompiler::new();
    let program =
        compiler.check(&source("login.body.token != '' && item.id == example.id"), &roots, &mut strings).unwrap();
    let found: Vec<_> = program.refs.iter().map(|r| r.ns).collect();
    assert_eq!(found, [Namespace::Step(login), Namespace::Item, Namespace::Example]);
    assert_eq!(check_in(&Roots::new(), "item.id").unwrap_err().code, Code::V2102);
}

#[test]
fn unavailable_roots_are_reported_as_for_templates() {
    let unavailable = check_in(&Roots::new(), "response.status == 200").unwrap_err();
    assert_eq!((unavailable.code, unavailable.primary.span.byte_range()), (Code::V2105, 0..15));
    let bare = check("body.id == 1").unwrap_err();
    assert_eq!(
        (bare.code, bare.message.as_str()),
        (Code::V2105, "`body` is not a root; did you mean `response.body`?")
    );
    let [fix] = &bare.fixes[..] else { panic!("expected one fix") };
    assert!(fix.certain);
    assert_eq!((fix.edits[0].new_text.as_str(), fix.edits[0].range.clone()), ("response.body", 0..4));
    assert_eq!(error("1 == 1 && status == 200").1, "`status` is not a root; did you mean `response.status`?");
}

#[test]
fn template_only_roots_are_not_available_in_cel() {
    for (text, root) in
        [("secret.key", "secret"), ("fake.uuid", "fake"), ("dataset.users", "dataset"), ("run.id", "run")]
    {
        let diagnostic = check(&format!("1 + {text}")).unwrap_err();
        assert_eq!(diagnostic.code, Code::V2105, "{text}");
        assert_eq!(diagnostic.message, format!("`{root}` is not available in CEL"));
        assert_eq!(diagnostic.primary.span.byte_range(), 4..4 + text.len());
        assert!(diagnostic.help[0].contains("var.<name>"));
    }
}

#[test]
fn syntax_errors_are_v2305_at_their_position() {
    let cases =
        [("response.body.", 14..14), ("1 +", 3..3), ("a\n  && (b", 9..9), ("'é' + )", 7..8), ("true &&\n  )", 10..11)];
    let found: Vec<_> = cases.iter().map(|(text, _)| (*text, error(text))).collect();
    for (text, (code, message, _)) in &found {
        assert_eq!(*code, Code::V2305, "{text}");
        assert!(message.starts_with("invalid CEL: ") && !message.contains("Syntax error"), "{message}");
    }
    let spans: Vec<_> = found.iter().map(|(text, (_, _, span))| (*text, span.clone())).collect();
    assert_eq!(spans, cases, "{found:?}");
}

#[test]
fn the_parser_internal_errors_are_never_shown() {
    for text in ["x[", "a.b(", "(1", "a ? b", "", "1 ++ 2 ++ 3", "é é", "a.", "1 < ", "-"] {
        let (code, message, _) = error(text);
        assert_eq!(code, Code::V2305, "{text}");
        assert!(!message.contains("Incomplete"), "{text}: {message}");
    }
}

#[test]
fn reserved_words_are_named() {
    assert_eq!(error("package + 1"), (Code::V2305, "invalid CEL: `package` is a reserved word".into(), 0..1));
    assert_eq!(error("var.a + _v_.b"), (Code::V2305, "`_v_` is reserved in CEL expressions".into(), 8..11));
}

#[test]
fn literal_times_are_checked() {
    let (code, message, span) = error("timestamp('2024-02-30T00:00:00Z') < var.t");
    assert_eq!((code, message.as_str(), span), (Code::V2305, "`2024-02-30T00:00:00Z` is not a calendar date", 10..32));
    assert_eq!(error("1 < 2 || duration('1d') == x").1, "`1d` is not a duration such as `1h30m` or `1.5s`");
    assert_eq!(error("1 < 2 || duration('1d') == x").2, 18..22);
    assert!(check("timestamp(var.t) < timestamp('2024-01-01T00:00:00Z') + duration('1h') && var.t.timestamp()").is_ok());
}

#[test]
fn literal_regexes_are_checked() {
    let (code, message, span) = error("matches(var.name, '[a-')");
    assert_eq!((code, span), (Code::V2306, 18..23));
    assert!(message.starts_with("invalid regex in `matches()`: "), "{message}");
    assert_eq!(error("var.name.matches('(')").2, 17..20);
    assert!(check("var.name.matches('^a+$') && matches(var.name, var.pattern) && var.p.matches(var.name)").is_ok());
}

#[test]
fn evaluates_against_the_activation() {
    let cases = [
        ("response.status == 200", json!(true)),
        ("response.headers.etag + var.name", json!("xada")),
        ("var.price * 2", json!(4)),
        ("var.ratio * 2.0", json!(1.0)),
        ("[var.big, var.big > 1u]", json!([u64::MAX, true])),
        ("response.body.items.map(i, i.price).sum()", json!(5)),
        ("response.body.items.filter(i, i.price > 2).size()", json!(1)),
        ("{'b': 1, 'a': [true, null]}", json!({"a": [true, null], "b": 1})),
        ("{1: 'x', true: 'y'}", json!({"1": "x", "true": "y"})),
        ("[1u, 2.5, 'é'.size()]", json!([1, 2.5, 1])),
        ("var.name.matches('^a.a$') && matches('abc', 'b')", json!(true)),
        ("has(response.body.items) && !has(response.body.missing)", json!(true)),
    ];
    let body = json!({"items": [{"price": 2}, {"price": 3}]});
    for (text, expected) in cases {
        assert_eq!(eval(text, &body), Ok(expected), "{text}");
    }
}

#[test]
fn sum_adds_ints_exactly_and_doubles_otherwise() {
    let cases = [
        ("[].sum()", Ok(json!(0))),
        ("[1, 2, 3].sum()", Ok(json!(6))),
        ("[1, 2.5].sum()", Ok(json!(3.5))),
        ("[0.5, 2].sum()", Ok(json!(2.5))),
        ("[0.5, 0.25].sum()", Ok(json!(0.75))),
    ];
    for (text, expected) in cases {
        assert_eq!(eval(text, &json!(null)), expected, "{text}");
    }
    let overflow = eval("[9223372036854775807, 1].sum()", &json!(null)).unwrap_err().to_string();
    assert!(overflow.contains("integer overflow"), "{overflow}");
    let mixed = eval("[1, 'a'].sum()", &json!(null)).unwrap_err().to_string();
    assert!(mixed.contains("not of string"), "{mixed}");
    let mixed = eval("[1.5, 'a'].sum()", &json!(null)).unwrap_err().to_string();
    assert!(mixed.contains("not of string"), "{mixed}");
    let scalar = eval("'abc'.sum()", &json!(null)).unwrap_err().to_string();
    assert!(scalar.contains("takes a list of numbers"), "{scalar}");
}

#[test]
fn comprehensions_stop_at_the_step_limit() {
    let body = json!({"items": [1, 2, 3], "map": {"a": 1, "b": 2}});
    let nested = "response.body.items.map(i, response.body.items.map(j, i * j)).size()";
    let entries = "{'k': response.body.items.map(i, i)}.size() + [response.body.items.filter(i, i > 1)].size()";
    let cases = [
        ("response.body.items.all(i, i > 0)", 3, Ok(json!(true))),
        ("response.body.items.all(i, i > 0)", 2, Err(ExprError::StepLimit(2))),
        ("response.body.map.exists(k, k == 'b')", 2, Ok(json!(true))),
        ("response.body.map.exists(k, k == 'b')", 1, Err(ExprError::StepLimit(1))),
        (nested, 12, Ok(json!(3))),
        (nested, 11, Err(ExprError::StepLimit(11))),
        (entries, 6, Ok(json!(2))),
        (entries, 5, Err(ExprError::StepLimit(5))),
        ("response.body.items.size()", 0, Ok(json!(3))),
        ("[1, 2].map(a, [1, 2].map(b, [1, 2].map(c, a + b + c))).size()", 6, Err(ExprError::StepLimit(6))),
    ];
    for (text, limit, expected) in cases {
        assert_eq!(eval_with(text, &response(&body).with_step_limit(limit)), expected, "{text} within {limit}");
    }
    assert_eq!(response(&body).step_limit(), DEFAULT_STEP_LIMIT);
}

#[test]
fn evaluation_errors_are_reported() {
    let missing = |text| eval_with(text, &Activation::new());
    assert_eq!(missing("var.x"), Err(ExprError::MissingRoot("var".into())));
    assert_eq!(missing("response.status"), Err(ExprError::MissingRoot("response".into())));
    assert_eq!(eval("b'abc'", &json!(null)), Err(ExprError::Unrepresentable("a bytes value".into())));
    let unrepresentable = ["1.0 / 0.0 * 0.0", "[b'a']", "{'a': b'a'}"];
    let failing = ["response.body.missing", "1 / 0", "var.price * 1.5", "var.name.matches(var.name + '[')", "nope(1)"];
    let kinds: Vec<_> = unrepresentable
        .iter()
        .chain(&failing)
        .map(|text| eval(text, &json!({})).map_err(|error| kind(&error)))
        .collect();
    let mut expected: Vec<Result<Value, &str>> = vec![Err("unrepresentable"); 3];
    expected.extend(vec![Err("eval"); 5]);
    assert_eq!(kinds, expected);
}

fn kind(error: &ExprError) -> &'static str {
    match error {
        ExprError::Eval(_) => "eval",
        ExprError::StepLimit(_) => "step limit",
        ExprError::MissingRoot(_) => "missing root",
        ExprError::Unrepresentable(_) => "unrepresentable",
        _ => "other",
    }
}

#[test]
fn prepared_expressions_bind_each_root_once() {
    let mut strings = StringTable::new();
    let login = strings.intern("login").unwrap();
    let roots = Roots::new().with_step(login).with_call();
    let compiler = CelCompiler::new();
    let program =
        compiler.check(&source("login.body.token + var.a + var.b + login.status"), &roots, &mut strings).unwrap();
    let expr = compiler.prepare(&program).unwrap();
    let debug = format!("{expr:?}");
    assert!(debug.contains("\"_v_\", Var") && debug.contains("\"login\", Step"), "{debug}");
    let activation = Activation::new()
        .with(Namespace::Step(login), json!({"body": {"token": "t"}, "status": "!"}))
        .with(Namespace::Var, json!({"a": "-", "b": "+"}));
    assert_eq!(expr.eval(&activation), Ok(json!("t-+!")));
    assert_eq!(expr.eval(&activation), Ok(json!("t-+!")));
    assert!(format!("{compiler:?}").starts_with("CelCompiler"));
}

#[test]
fn prepare_rejects_programs_check_would_reject() {
    let program = CelProgram { source: source("1 +"), refs: Vec::new() };
    assert!(matches!(CelCompiler::default().prepare(&program), Err(TemplateError::Syntax(_))));
    let program = CelProgram { source: source("timestamp('x')"), refs: Vec::new() };
    assert!(matches!(CelCompiler::new().prepare(&program), Err(TemplateError::Syntax(_))));
}
