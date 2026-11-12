use serde_json::{Number, json};
use vc_model::{
    Namespace, Part, Path, PipeCall, Pos, Program, Reference, Segment, SourceFileId, SourceSpan, Template,
    TemplateValue,
};

use super::{Shown, Var, literal, set};

fn nowhere() -> SourceSpan {
    SourceSpan::new(SourceFileId::new(0), Pos::START, Pos::START)
}

fn reference(ns: Namespace, keys: &[&str]) -> Reference {
    let segments = keys.iter().map(|key| Segment::Key((*key).to_owned())).collect();
    Reference {
        ns,
        path: Path { segments },
        args: Vec::new(),
        pipes: Vec::new(),
        span: nowhere(),
        root_span: nowhere(),
    }
}

fn template(parts: Vec<Part>) -> TemplateValue {
    TemplateValue::String(Template { parts })
}

fn var(name: &str, value: TemplateValue) -> Var {
    Var { name: name.to_owned(), value, origin: "test".to_owned(), saved: false, replaced: Vec::new() }
}

#[test]
fn plain_vars_are_filled_in_and_other_references_shown_as_written() {
    let program = Program::default();
    let pipe =
        PipeCall { name: "default".into(), args: vec![json!("x"), json!(2)], name_span: nowhere(), span: nowhere() };
    let piped = Reference { pipes: vec![pipe], ..reference(Namespace::Var, &["unset"]) };
    let fake = Reference { args: vec![json!(5)], ..reference(Namespace::Fake, &["lorem", "words"]) };
    let value = template(vec![
        Part::Literal("http://".into()),
        Part::Ref(reference(Namespace::Var, &["host"])),
        Part::Ref(reference(Namespace::Var, &["unset"])),
        Part::Ref(piped),
        Part::Ref(reference(Namespace::Response, &["body", "id"])),
        Part::Ref(reference(Namespace::Var, &["host", "name"])),
        Part::Ref(fake),
    ]);
    let vars = [var("host", literal("api"))];
    let shown = Shown { program: &program, vars: &vars };
    let expected =
        "http://api${var.unset}${var.unset | default('x', 2)}${response.body.id}${var.host.name}${fake.lorem.words(5)}";
    assert_eq!(shown.value(&value), expected);
}

#[test]
fn a_var_that_names_itself_stops_after_a_few_rounds() {
    let program = Program::default();
    let looped = template(vec![Part::Literal("a".into()), Part::Ref(reference(Namespace::Var, &["me"]))]);
    let vars = [var("me", looped.clone())];
    let shown = Shown { program: &program, vars: &vars };
    assert_eq!(shown.value(&looped), format!("{}${{var.me}}", "a".repeat(9)));
}

#[test]
fn values_of_every_kind_show_as_written() {
    let program = Program::default();
    let shown = Shown { program: &program, vars: &[] };
    let pairs = [("on".to_owned(), TemplateValue::Bool(true)), ("none".to_owned(), TemplateValue::Null)];
    let list = TemplateValue::List(vec![
        TemplateValue::Number(Number::from(7)),
        TemplateValue::Map(pairs.into_iter().collect()),
    ]);
    assert_eq!(shown.value(&list), "[7, {on: true, none: null}]");
}

#[test]
fn a_set_var_replaces_the_one_before_in_its_place() {
    let mut vars = Vec::new();
    set(&mut vars, var("a", literal("1")));
    set(&mut vars, var("b", literal("2")));
    set(&mut vars, Var { origin: "later".into(), ..var("a", literal("3")) });
    let shown: Vec<(&str, &str)> = vars.iter().map(|var| (var.name.as_str(), var.origin.as_str())).collect();
    assert_eq!(shown, [("a", "later"), ("b", "test")]);
    assert_eq!(Shown { program: &Program::default(), vars: &[] }.value(&vars[0].value), "3");
}

#[test]
fn a_set_var_keeps_the_values_it_replaced_latest_first() {
    let mut vars = Vec::new();
    set(&mut vars, var("a", literal("1")));
    set(&mut vars, Var { origin: "profile dev".into(), ..var("a", literal("2")) });
    set(&mut vars, Var { origin: "api users#get".into(), ..var("a", literal("3")) });
    let shown = Shown { program: &Program::default(), vars: &[] };
    let replaced: Vec<(String, &str)> =
        vars[0].replaced.iter().map(|(value, origin)| (shown.value(value), origin.as_str())).collect();
    assert_eq!(replaced, [("2".to_owned(), "profile dev"), ("1".to_owned(), "test")]);
}

fn piped(name: &str, pipe: &str) -> Part {
    let call = PipeCall { name: pipe.into(), args: Vec::new(), name_span: nowhere(), span: nowhere() };
    Part::Ref(Reference { pipes: vec![call], ..reference(Namespace::Var, &[name]) })
}

#[test]
fn a_known_var_is_shown_through_its_pipes() {
    let program = Program::default();
    let vars = [var("userId", literal("a b")), var("name", literal("Ada"))];
    let shown = Shown { program: &program, vars: &vars };
    let path = template(vec![Part::Literal("/users/".into()), piped("userId", "urlEncode"), piped("name", "upper")]);
    assert_eq!(shown.value(&path), "/users/a%20bADA");
    let list = template(vec![piped("name", "random")]);
    assert_eq!(shown.value(&list), "${var.name | random}", "a pipe that fails is shown as written");
}

#[test]
fn a_saved_var_is_shown_as_written() {
    let program = Program::default();
    let vars = [Var { saved: true, ..var("userId", literal("${response.body.id}")) }];
    let shown = Shown { program: &program, vars: &vars };
    let path = template(vec![Part::Literal("/users/".into()), Part::Ref(reference(Namespace::Var, &["userId"]))]);
    assert_eq!(shown.value(&path), "/users/${var.userId}");
}
