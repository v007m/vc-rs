use serde::Deserialize;
use serde::de::IntoDeserializer;
use serde::de::value::Error;
use vc_model::SourceFileId;

use super::{GraphqlMismatch, GraphqlSchema, MAX_DEPTH, parse_schema};
use crate::{Node, Syntax, parse, schema_from_introspection};

/// The accounts schema as SDL.
const SDL: &str = include_str!("../tests/graphql/accounts.graphql");

/// The same schema as an introspection result.
const INTROSPECTION: &str = include_str!("../tests/graphql/accounts.json");

fn accounts() -> GraphqlSchema {
    parse_schema(SDL).unwrap()
}

/// The problems of `sdl`, each with the text it points at.
fn problems(sdl: &str) -> Vec<(String, &str)> {
    let problems = parse_schema(sdl).unwrap_err();
    problems.into_iter().map(|problem| (problem.message, &sdl[problem.at])).collect()
}

/// The one mismatch of `document`, with the text it points at.
fn mismatch<'d>(schema: &GraphqlSchema, document: &'d str) -> (GraphqlMismatch, &'d str) {
    let found = schema.check_document(document);
    assert_eq!(found.len(), 1, "{document}: {found:?}");
    let at = &document[found[0].at.clone()];
    (found[0].clone(), at)
}

/// The values of the YAML map `yaml`.
fn values(yaml: &str) -> Node {
    let (node, diagnostics) = parse(SourceFileId::new(0), Syntax::Yaml, yaml);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    node.unwrap()
}

/// The misfits of the YAML map `yaml` as the variables of `document`'s operation `name`.
fn misfits(document: &str, name: Option<&str>, yaml: &str) -> Vec<(usize, String)> {
    misfits_in(&accounts(), (document, name), yaml)
}

/// The misfits of the YAML map `yaml` as the variables of an operation of `schema`.
fn misfits_in(schema: &GraphqlSchema, operation: (&str, Option<&str>), yaml: &str) -> Vec<(usize, String)> {
    let node = values(yaml);
    let given: Vec<(&str, &Node)> = node.entries().iter().map(|entry| (entry.key.as_str(), &entry.value)).collect();
    schema.check_variables(operation, &given)
}

#[test]
fn a_schema_holds_type_definitions_only() {
    let sdl = "type Query { me: String }\n";
    assert_eq!(parse_schema(sdl).unwrap().sdl(), sdl);
    let found = problems("type Query { me: String }\nquery Me {\n  me\n}\nfragment F on Query { me }");
    let expected = [
        ("a schema document must not contain an operation definition".to_owned(), "query Me {"),
        ("a schema document must not contain a fragment definition".to_owned(), "fragment F on Query { me }"),
    ];
    assert_eq!(found, expected);
    assert_eq!(parse_schema("# nothing yet\n").unwrap_err()[0].at, 14..14);
    assert_eq!(parse_schema("type Query {").unwrap_err()[0].at, 12..12);
}

#[test]
fn a_schema_that_does_not_validate_is_reported_where_it_is_wrong() {
    let missing = problems("type Query { me: Missing }");
    assert_eq!(missing, [("cannot find type `Missing` in this document".to_owned(), "Missing")]);
    let twice = problems("type Query { a: Int }\ntype Query { b: Int }");
    assert_eq!(twice, [("the type `Query` is defined multiple times in the schema".to_owned(), "Query")]);
    let empty = problems("type Query { a: Int }\nenum Role");
    assert_eq!(empty, [("`Role` has no enum values".to_owned(), "enum Role")]);
    let no_query = parse_schema("type T { a: Int }").unwrap_err();
    assert_eq!(
        (no_query[0].message.as_str(), no_query[0].at.clone()),
        ("missing query root operation type in schema definition", 0..0)
    );
}

#[test]
fn one_of_is_built_in_and_may_be_defined_again() {
    let input = "type Query { a(key: Key): Int }\ninput Key @oneOf { id: ID, email: String }\n";
    assert!(parse_schema(input).is_ok());
    assert!(parse_schema(&format!("{input}directive @oneOf on INPUT_OBJECT\n")).is_ok());
    let schema = parse_schema(input).unwrap();
    let operation = ("query Q($key: Key) { a(key: $key) }", None);
    assert_eq!(misfits_in(&schema, operation, "key: {id: '1'}\n"), []);
    let extra = misfits_in(&schema, operation, "key: {id: '1', name: x}\n");
    assert_eq!(extra, [(0, "Input object has key name not in type Key".to_owned())]);
}

#[test]
fn the_same_schema_from_sdl_and_from_introspection_checks_documents_alike() {
    let (json, _) = parse(SourceFileId::new(0), Syntax::Json, INTROSPECTION);
    let introspected = schema_from_introspection(&json.unwrap()).unwrap();
    let documents = [
        "query { user(id: 1) { id name role } }",
        "query { user(id: 1) { nmae } }",
        "query { user { id } }",
        "query Q($k: String) { user(id: $k) { id } }",
        "query { user(id: 1) { name { first } } }",
        "mutation { createUser(input: {name: \"a\", age: 3}) { id } }",
    ];
    for document in documents {
        assert_eq!(introspected.check_document(document), accounts().check_document(document), "{document}");
    }
    assert_eq!(accounts().check_document(documents[0]), []);
}

#[test]
fn an_unknown_field_lists_the_fields_of_its_type() {
    let users = ["id", "name", "email", "role", "createdAt"].map(str::to_owned);
    for document in [
        "query { user(id: 1) { nmae } }",
        "query { search(text: \"a\") { ... on User { nmae } } }",
        "query { ...F }\nfragment F on Query { user(id: 1) { id nmae } }",
    ] {
        let (found, at) = mismatch(&accounts(), document);
        assert_eq!((found.message.as_str(), at), ("type `User` does not have a field `nmae`", "nmae"), "{document}");
        assert_eq!(found.fields, users, "{document}");
    }
    let (found, _) = mismatch(&accounts(), "query { mee }");
    assert_eq!(found.fields, ["user", "users", "search"].map(str::to_owned));
    let (found, _) = mismatch(&accounts(), "query { search(text: \"a\") { id } }");
    assert!(found.fields.is_empty(), "a union has no fields: {found:?}");
}

#[test]
fn fields_are_listed_only_within_the_depth_bound() {
    let schema = parse_schema("type Query { me: Person }\ntype Person { id: ID, friend: Person }").unwrap();
    let nested = |depth: usize| format!("query {{ me {}{{ nmae }}{} }}", "{ friend ".repeat(depth), " }".repeat(depth));
    let (found, _) = mismatch(&schema, &nested(MAX_DEPTH - 2));
    assert_eq!(found.fields, ["id", "friend"].map(str::to_owned));
    let (found, _) = mismatch(&schema, &nested(MAX_DEPTH - 1));
    assert!(found.fields.is_empty(), "{found:?}");
}

#[test]
fn a_document_that_does_not_fit_the_schema_is_reported_where_it_is_wrong() {
    let cases = [
        ("query { user { id } }", "the required argument `Query.user(id:)` is not provided", "user { id }"),
        (
            "query Q($k: String) { user(id: $k) { id } }",
            "variable `$k` of type `String` cannot be used for argument `id` of type `ID!`",
            "id: $k",
        ),
        (
            "query { user(id: 1) { id name { first } } }",
            "field selection of scalar type `String` must not have subselections",
            "name { first }",
        ),
        ("query { user(id: 1) }", "interface, union and object types must have a subselection set", "user(id: 1)"),
        ("query Q($a: Int) { user(id: 1) { id } }", "unused variable: `$a`", "$a"),
    ];
    for (document, message, at) in cases {
        let (found, spot) = mismatch(&accounts(), document);
        assert_eq!((found.message.as_str(), spot), (message, at), "{document}");
        assert!(found.fields.is_empty(), "{document}");
    }
}

#[test]
fn two_operations_of_one_name_are_left_to_v2009() {
    let document = "query A { user(id: 1) { id } }\nquery A { user(id: 2) { id } }";
    assert_eq!(accounts().check_document(document), []);
}

#[test]
fn a_literal_value_that_does_not_fit_its_variable_type_is_a_misfit() {
    let document = "query Q($limit: Int, $roles: [Role!], $in: NewUser, $on: Boolean, $id: ID!) \
                    { users(limit: $limit, roles: $roles) { id } }";
    let found = misfits(
        document,
        None,
        "limit: '3'\nroles: [ADMIN, BOSS]\nin: {name: x, age: 3}\non: 'yes'\nid: null\nunknown: 1\n",
    );
    let expected = [
        (0, "could not coerce variable limit: \"3\" to type Int"),
        (1, "could not coerce variable roles: \"BOSS\" to type Role"),
        (2, "Input object has key age not in type NewUser"),
        (3, "could not coerce variable on: \"yes\" to type Boolean"),
        (4, "null value for variable id of non-null type ID!"),
    ];
    let expected: Vec<(usize, String)> =
        expected.iter().map(|(index, message)| (*index, (*message).to_owned())).collect();
    assert_eq!(found, expected);
}

#[test]
fn a_literal_value_that_fits_is_no_misfit() {
    let document = "query Q($limit: Int, $roles: [Role!], $in: NewUser, $on: Boolean, $id: ID) \
                    { users(limit: $limit, roles: $roles) { id } }";
    let yaml = "limit: 3\nroles: [ADMIN]\nin: {name: x, role: ADMIN}\non: True\nid: null\n";
    assert_eq!(misfits(document, None, yaml), []);
    assert_eq!(misfits(document, None, "on: false\n"), []);
}

#[test]
fn a_float_variable_takes_a_number_and_a_boolean_takes_its_kind() {
    let schema = parse_schema("type Query { f(at: Float, on: Boolean): Int }").unwrap();
    let operation = ("query Q($at: Float, $on: Boolean) { f(at: $at, on: $on) }", None);
    assert_eq!(misfits_in(&schema, operation, "at: 1.5\non: true\n"), []);
    assert_eq!(misfits_in(&schema, operation, "at: 2\non: FALSE\n"), []);
    let wrong = misfits_in(&schema, operation, "at: x\non: 1.5\n");
    let wrong: Vec<usize> = wrong.into_iter().map(|(index, _)| index).collect();
    assert_eq!(wrong, [0, 1]);
}

#[test]
fn a_variable_of_a_type_the_schema_lacks_is_left_to_v2314() {
    let document = "query Q($at: Float) { users { id } }";
    assert_eq!(misfits(document, None, "at: x\n"), []);
}

#[test]
fn a_value_with_no_json_form_is_left_alone() {
    let document = "query Q($at: Float, $limit: Int, $roles: [Role!]) { users(limit: $limit, roles: $roles) { id } }";
    assert_eq!(misfits(document, None, "at: .inf\nlimit: 0x1F\nroles: [.nan]\n"), []);
}

#[test]
fn the_variables_are_those_of_the_operation_picked() {
    let document = "query A($limit: Int) { users(limit: $limit) { id } }\nquery B($limit: String) { search(text: $limit) { ... on User { id } } }";
    assert_eq!(
        misfits(document, Some("B"), "limit: 3\n"),
        [(0, "could not coerce variable limit: 3 to type String".to_owned())]
    );
    assert_eq!(misfits(document, Some("A"), "limit: 3\n"), []);
    assert_eq!(misfits(document, None, "limit: '3'\n"), [], "no operation is picked");
    assert_eq!(misfits(document, Some("C"), "limit: '3'\n"), []);
}

#[test]
fn a_schema_is_read_back_from_its_sdl() {
    let read = GraphqlSchema::deserialize(SDL.into_deserializer());
    let read: Result<GraphqlSchema, Error> = read;
    assert_eq!(read.unwrap(), accounts());
    let broken: Result<GraphqlSchema, Error> = GraphqlSchema::deserialize("type T".into_deserializer());
    assert_eq!(broken.unwrap_err().to_string(), "the SDL is not a schema");
}
