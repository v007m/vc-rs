use vc_model::SourceFileId;

use super::{MAX_WRAPPERS, schema_from_introspection};
use crate::{GraphqlProblem, Node, Syntax, parse, parse_schema};

/// An introspection result of the accounts schema, with the built-ins a server lists.
const INTROSPECTION: &str = include_str!("../tests/graphql/accounts.json");

/// The same schema written as SDL, as the converter writes it.
const SDL: &str = include_str!("../tests/graphql/accounts.graphql");

fn json(text: &str) -> Node {
    let (node, diagnostics) = parse(SourceFileId::new(0), Syntax::Json, text);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    node.unwrap()
}

/// The problem converting `text` gives, and the text it points at.
fn problem(text: &str) -> (String, String) {
    let GraphqlProblem { message, at, .. } = schema_from_introspection(&json(text)).unwrap_err();
    (message, text[at].to_owned())
}

/// An introspection result whose only type is a `Query` with one field `f` of type `ty`.
fn one_field(ty: &str) -> String {
    let field = format!(r#"{{"name": "f", "args": [], "type": {ty}}}"#);
    format!(
        r#"{{"__schema": {{"queryType": {{"name": "Query"}}, "types": [{{"kind": "OBJECT", "name": "Query", "fields": [{field}]}}]}}}}"#
    )
}

/// A type reference of `depth` wrappers, alternating `NON_NULL` and `LIST`, around `Int`.
fn wrapped(depth: usize) -> String {
    let kinds = ["NON_NULL", "LIST"].iter().cycle().take(depth);
    let inner = r#"{"kind": "SCALAR", "name": "Int", "ofType": null}"#.to_owned();
    kinds.fold(inner, |inner, kind| format!(r#"{{"kind": "{kind}", "name": null, "ofType": {inner}}}"#))
}

#[test]
fn writes_an_introspection_result_as_the_sdl_of_the_same_schema() {
    let schema = schema_from_introspection(&json(INTROSPECTION)).unwrap();
    assert_eq!(schema.sdl(), SDL);
    assert_eq!(parse_schema(SDL).unwrap(), schema);
}

#[test]
fn reads_the_schema_with_or_without_the_data_wrapper() {
    let bare = one_field(r#"{"kind": "SCALAR", "name": "Int", "ofType": null}"#);
    let wrapped = format!(r#"{{"data": {bare}}}"#);
    let expected = "schema {\n  query: Query\n}\n\ntype Query {\n  f: Int\n}\n";
    assert_eq!(schema_from_introspection(&json(&bare)).unwrap().sdl(), expected);
    assert_eq!(schema_from_introspection(&json(&wrapped)).unwrap().sdl(), expected);
}

#[test]
fn nests_type_references_up_to_the_limit() {
    let deepest = schema_from_introspection(&json(&one_field(&wrapped(MAX_WRAPPERS)))).unwrap();
    let expected = format!("{}Int{}", "[".repeat(MAX_WRAPPERS / 2), "!]".repeat(MAX_WRAPPERS / 2));
    assert!(deepest.sdl().contains(&format!("f: {expected}\n")), "{}", deepest.sdl());
    let text = one_field(&wrapped(MAX_WRAPPERS + 1));
    let (message, at) = problem(&text);
    assert_eq!(message, "the type nests more than 16 `NON_NULL` and `LIST` wrappers");
    assert!(at.starts_with(r#"{"kind": "NON_NULL""#), "{at}");
}

#[test]
fn a_json_file_that_is_not_an_introspection_result_points_at_what_is_wrong() {
    let cases = [
        (
            r#"{"openapi": "3.1.0"}"#,
            "it is not an introspection result: no `__schema` in it",
            r#"{"openapi": "3.1.0"}"#,
        ),
        (r#"{"data": null}"#, "it is not an introspection result: no `__schema` in it", r#"{"data": null}"#),
        (r#"{"__schema": {"types": 3}}"#, "`__schema` has no `types` list", r#"{"types": 3}"#),
        (r#"{"__schema": {"types": [{"kind": "OBJECT"}]}}"#, "expected `name` here", r#"{"kind": "OBJECT"}"#),
        (r#"{"__schema": {"types": [{"kind": "TABLE", "name": "T"}]}}"#, "`TABLE` is not a type kind", r#""TABLE""#),
        (
            r#"{"__schema": {"types": [{"kind": "OBJECT", "name": "my type"}]}}"#,
            "expected a GraphQL name here",
            r#""my type""#,
        ),
        (r#"{"__schema": {"types": [{"kind": 1, "name": "T"}]}}"#, "`kind` must be text", "1"),
        (
            r#"{"__schema": {"types": [{"kind": "SCALAR", "name": "U", "specifiedByURL": 3}]}}"#,
            "`specifiedByURL` must be text",
            "3",
        ),
    ];
    for (text, message, at) in cases {
        assert_eq!(problem(text), (message.to_owned(), at.to_owned()), "{text}");
    }
}

#[test]
fn a_type_reference_must_end_in_a_named_type() {
    let (message, at) = problem(&one_field(r#"{"kind": "NON_NULL", "name": null, "ofType": null}"#));
    assert_eq!((message.as_str(), at.as_str()), ("expected `kind` here", "null"));
    let (message, _) = problem(&one_field(r#"{"kind": "LIST", "name": null}"#));
    assert_eq!(message, "expected `ofType` here");
}

#[test]
fn types_that_do_not_make_a_schema_are_reported_at_the_schema() {
    let input = r#"{"kind": "INPUT_OBJECT", "name": "I", "inputFields": [{"name": "a", "type": {"kind": "SCALAR", "name": "Int"}, "defaultValue": "{"}]}"#;
    let text = format!(r#"{{"__schema": {{"types": [{input}]}}}}"#);
    let (message, at) = problem(&text);
    assert!(message.starts_with("its types do not make a schema: "), "{message}");
    assert!(at.starts_with(r#"{"types""#), "{at}");
}

#[test]
fn leaves_out_what_every_schema_has() {
    let text = r#"{"__schema": {"types": [
        {"kind": "SCALAR", "name": "Boolean"}, {"kind": "OBJECT", "name": "__Type", "fields": []},
        {"kind": "SCALAR", "name": "Url"}, {"kind": "OBJECT", "name": "Query", "fields": [{"name": "u", "args": [], "type": {"kind": "SCALAR", "name": "Url"}}]}],
        "directives": [{"name": "include", "locations": ["FIELD"], "args": []}, {"name": "specifiedBy", "locations": ["SCALAR"]}]}}"#;
    assert_eq!(schema_from_introspection(&json(text)).unwrap().sdl(), "\nscalar Url\n\ntype Query {\n  u: Url\n}\n");
}

#[test]
fn keeps_the_schema_description() {
    let text = one_field(r#"{"kind": "SCALAR", "name": "Int"}"#).replacen(
        r#"{"queryType""#,
        r#"{"description": "The shop", "queryType""#,
        1,
    );
    let sdl = schema_from_introspection(&json(&text)).unwrap().sdl().to_owned();
    assert!(sdl.starts_with("\"The shop\"\nschema {\n  query: Query\n}\n"), "{sdl}");
}

#[test]
fn a_type_with_nothing_in_it_is_reported_at_the_type() {
    let cases = [
        (r#"{"kind": "OBJECT", "name": "T", "fields": null}"#, "fields"),
        (r#"{"kind": "INTERFACE", "name": "T", "fields": []}"#, "fields"),
        (r#"{"kind": "UNION", "name": "T", "possibleTypes": []}"#, "possibleTypes"),
        (r#"{"kind": "ENUM", "name": "T", "enumValues": null}"#, "enumValues"),
        (r#"{"kind": "INPUT_OBJECT", "name": "T", "inputFields": []}"#, "inputFields"),
    ];
    for (ty, key) in cases {
        let text = one_field(r#"{"kind": "SCALAR", "name": "Int"}"#).replacen(
            r#""types": ["#,
            &format!(r#""types": [{ty}, "#),
            1,
        );
        let message = format!("`{key}` lists nothing here; a schema needs at least one");
        assert_eq!(problem(&text), (message, ty.to_owned()), "{ty}");
    }
    let directive = r#"{"name": "d", "locations": [], "args": []}"#;
    let text = one_field(r#"{"kind": "SCALAR", "name": "Int"}"#).replacen(
        "]}}",
        &format!(r#"], "directives": [{directive}]}}}}"#),
        1,
    );
    let (message, at) = problem(&text);
    assert_eq!(
        (message.as_str(), at.as_str()),
        ("`locations` lists nothing here; a schema needs at least one", directive)
    );
}

#[test]
fn a_type_reference_must_be_of_a_type_kind() {
    let (message, at) = problem(&one_field(r#"{"kind": "TABLE", "name": "Int"}"#));
    assert_eq!((message.as_str(), at.as_str()), ("`TABLE` is not a type kind", r#""TABLE""#));
    let (message, at) = problem(&one_field(r#"{"kind": "LIST", "ofType": {"kind": "ROW", "name": "Int"}}"#));
    assert_eq!((message.as_str(), at.as_str()), ("`ROW` is not a type kind", r#""ROW""#));
}
