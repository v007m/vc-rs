use indexmap::IndexMap;

use super::{Namespace, Part, Path, Reference, StatusPattern, Template, TemplateValue};
use crate::{Pos, SourceFileId, SourceSpan};

fn reference() -> Reference {
    let span = SourceSpan::new(SourceFileId::new(0), Pos::START, Pos::START);
    Reference { ns: Namespace::Var, path: Path::default(), args: Vec::new(), pipes: Vec::new(), span, root_span: span }
}

#[test]
fn only_a_lone_reference_preserves_type() {
    let lone = Template { parts: vec![Part::Ref(reference())] };
    assert!(lone.preserves_type());
    assert_eq!(lone.single_ref(), Some(&reference()));
    let mixed = Template { parts: vec![Part::Literal("u-".into()), Part::Ref(reference())] };
    assert!(!mixed.preserves_type());
    assert_eq!(mixed.single_ref(), None);
    assert!(!Template::default().preserves_type());
}

#[test]
fn a_value_gives_the_references_anywhere_inside_it_in_order() {
    let mut env = reference();
    env.ns = Namespace::Env;
    let read = |reference: &Reference| TemplateValue::String(Template { parts: vec![Part::Ref(reference.clone())] });
    let map = IndexMap::from([("a".to_owned(), read(&env)), ("b".to_owned(), TemplateValue::Null)]);
    let changes = Box::new(TemplateValue::List(vec![read(&reference()), TemplateValue::Map(map)]));
    let base = Template { parts: vec![Part::Literal("x".into()), Part::Ref(env)] };
    let value = TemplateValue::Extend { base, changes, remove: Vec::new() };
    let namespaces: Vec<Namespace> = value.references().iter().map(|reference| reference.ns).collect();
    assert_eq!(namespaces, [Namespace::Env, Namespace::Var, Namespace::Env]);
    assert_eq!(TemplateValue::Bool(true).references(), Vec::<&Reference>::new());
}

#[test]
fn only_a_template_without_references_is_literal() {
    let written = Template { parts: vec![Part::Literal("data/a.csv".into())] };
    assert_eq!(written.as_literal(), Some("data/a.csv"));
    assert_eq!(Template::default().as_literal(), Some(""));
    let mixed = Template { parts: vec![Part::Literal("u-".into()), Part::Ref(reference())] };
    assert_eq!(mixed.as_literal(), None);
}

#[test]
fn status_patterns_match_codes_and_classes() {
    assert!(StatusPattern::Code(201).matches(201));
    assert!(!StatusPattern::Code(201).matches(200));
    assert!(StatusPattern::Class(2).matches(299));
    assert!(!StatusPattern::Class(2).matches(300));
    assert!(!StatusPattern::Class(2).matches(199));
    assert_eq!(
        (StatusPattern::Class(2).to_string(), StatusPattern::Code(101).to_string()),
        ("2xx".into(), "101".into())
    );
}
