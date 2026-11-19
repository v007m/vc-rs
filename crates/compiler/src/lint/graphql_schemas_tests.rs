use vc_diagnostics::{Code, Diagnostic};
use vc_model::{ContentHash, SourceFile, SourcePath};

use crate::testing::spots;
use crate::{CompileInput, Compiled};

const CONFIG: &str = "syntaxVersion: 2\nproject: shop\nvars: {baseUrl: 'http://x', limit: 3, role: ADMIN}\nprofiles:\n  - id: dev\n    default: true\n";

/// The accounts schema as SDL, and as the introspection result a server answers with.
const SDL: &str = include_str!("../../../parser/tests/graphql/accounts.graphql");
const INTROSPECTION: &str = include_str!("../../../parser/tests/graphql/accounts.json");

/// Each way the accounts schema is written: its file name and its text.
const SCHEMAS: [(&str, &str); 2] = [("schema.graphql", SDL), ("schema.json", INTROSPECTION)];

fn file(path: &str, text: &str) -> SourceFile {
    SourceFile { path: SourcePath::new(path).unwrap(), text: text.to_owned(), hash: ContentHash::new([0; 32]) }
}

/// The users apis `apis`, each called by a test, in a file whose `graphql.schema` is the file
/// `schema` (`None`: no schema), with the files `given`.
fn compile(apis: &str, schema: Option<(&str, &str)>, given: &[(&str, &str)]) -> Compiled {
    let ids = apis.lines().filter_map(|line| line.strip_prefix("  - id: "));
    let call = |id: &str| {
        let (head, tail) = id.split_at(1);
        format!("  - id: gets{}{tail}\n    call: users#{id}\n", head.to_uppercase())
    };
    let calls: String = ids.map(call).collect();
    let key = schema.map_or(String::new(), |(name, _)| format!("graphql:\n  schema: {{$file: {name}}}\n"));
    let api = format!("type: graphql\nbaseUrl: ${{var.baseUrl}}\n{key}apis:\n{apis}");
    let mut files = vec![
        file("vibranium.config.yaml", CONFIG),
        file("services/users/users.api.yaml", &api),
        file("services/users/crud.test.yaml", &format!("tests:\n{calls}")),
    ];
    let schema = schema.map(|(name, text)| (format!("services/users/{name}"), text));
    files.extend(schema.iter().map(|(path, text)| file(path, text)));
    files.extend(given.iter().map(|(path, text)| file(path, text)));
    let paths = files.iter().skip(3).map(|file| file.path.clone());
    let assets = Some(paths.collect());
    crate::compile(CompileInput { files, assets, ..CompileInput::default() })
}

/// One api `me` with the body `body`.
fn me(body: &str) -> String {
    format!("  - id: me\n    body:\n{body}")
}

/// The one diagnostic of `compiled`.
fn one(compiled: &Compiled) -> &Diagnostic {
    let [found] = compiled.diagnostics.iter().collect::<Vec<_>>()[..] else { panic!("{:?}", compiled.diagnostics) };
    found
}

#[test]
fn an_unknown_field_is_v2314_with_the_close_field_as_its_fix() {
    for schema in SCHEMAS {
        let compiled = compile(&me("      query: 'query Me { user(id: 1) { id nmae } }'\n"), Some(schema), &[]);
        assert_eq!(spots(&compiled), [(Code::V2314, 8, 43, 4)], "{}", schema.0);
        let found = one(&compiled);
        assert_eq!(found.message, "the document does not fit the schema: type `User` does not have a field `nmae`");
        assert_eq!(found.help, ["did you mean `name`?"]);
        assert_eq!(found.fixes.len(), 1);
        let far = compile(&me("      query: 'query Me { user(id: 1) { zzzzzz } }'\n"), Some(schema), &[]);
        assert_eq!(one(&far).help, ["expected one of `id`, `name`, `email`, `role`, `createdAt`"]);
    }
}

#[test]
fn a_document_the_schema_rejects_is_v2314_where_it_is_wrong() {
    let cases = [
        ("query Me { user { id } }", "the required argument `Query.user(id:)` is not provided", 26, "user { id }"),
        (
            "query Me($k: String) { user(id: $k) { id } }",
            "variable `$k` of type `String` cannot be used for argument `id` of type `ID!`",
            43,
            "id: $k",
        ),
        (
            "query Me { user(id: 1) { id name { first } } }",
            "field selection of scalar type `String` must not have subselections",
            43,
            "name { first }",
        ),
    ];
    for schema in SCHEMAS {
        for (document, message, column, at) in cases {
            let body = format!("      query: '{document}'\n      variables: {{k: a}}\n");
            let compiled = compile(
                &me(&body.replace("{k: a}", if document.contains("$k") { "{k: a}" } else { "{}" })),
                Some(schema),
                &[],
            );
            let found = one(&compiled);
            assert_eq!(found.message, format!("the document does not fit the schema: {message}"), "{document}");
            assert_eq!(spots(&compiled), [(Code::V2314, 8, column, at.len())], "{document}");
            assert_eq!(found.help, ["write the document as the schema in `graphql.schema` has it"]);
        }
    }
}

#[test]
fn a_document_file_is_checked_once_against_each_schema() {
    let apis = "  - id: me\n    body:\n      query: {$file: me.graphql}\n  - id: meAgain\n    body:\n      query: {$file: me.graphql}\n";
    let given = [("services/users/me.graphql", "query Me {\n  user(id: 1) { nmae }\n}\n")];
    let compiled = compile(apis, Some(SCHEMAS[0]), &given);
    assert_eq!(spots(&compiled), [(Code::V2314, 2, 17, 4)]);
}

#[test]
fn without_a_schema_a_document_is_not_checked_against_one() {
    for schema in [None, Some(("schema.json", "{\"__schema\": {}}\n"))] {
        let compiled = compile(&me("      query: 'query Me { user { nmae } }'\n"), schema, &[]);
        assert!(!spots(&compiled).iter().any(|(code, ..)| *code == Code::V2314), "{:?}", compiled.diagnostics);
    }
}

#[test]
fn a_literal_value_of_another_type_is_v2315_at_the_value() {
    let query =
        "      query: 'query Me($limit: Int, $roles: [Role!]) { users(limit: $limit, roles: $roles) { id } }'\n";
    for schema in SCHEMAS {
        let given =
            |variables: &str| compile(&me(&format!("{query}      variables: {variables}\n")), Some(schema), &[]);
        let quoted = given("{limit: '3'}");
        assert_eq!(spots(&quoted), [(Code::V2315, 9, 26, 3)], "{}", schema.0);
        assert_eq!(
            one(&quoted).message,
            "the value of `$limit` does not fit the type the operation declares: could not coerce variable limit: \"3\" to type Int"
        );
        assert_eq!(spots(&given("{limit: 3, roles: [ADMIN, BOSS]}")), [(Code::V2315, 9, 36, 13)]);
        let escaped = given("{limit: '$${var.limit}'}");
        assert_eq!(spots(&escaped), [(Code::V2315, 9, 26, 15)], "an escape is literal: {}", schema.0);
        for fits in ["{limit: 3}", "{limit: '${var.limit}'}", "{roles: ['${var.role}']}", "{limit: 3, roles: [ADMIN]}"]
        {
            assert!(given(fits).diagnostics.is_empty(), "{fits}: {:?}", given(fits).diagnostics);
        }
    }
}

#[test]
fn a_value_is_checked_only_against_a_schema() {
    let body = "      query: 'query Me($limit: Int) { users(limit: $limit) { id } }'\n      variables: {limit: '3'}\n";
    assert!(compile(&me(body), None, &[]).diagnostics.is_empty());
}

#[test]
fn a_key_is_literal_text_as_lowered_and_is_checked() {
    let query = "      query: 'mutation Me($in: NewUser!) { createUser(input: $in) { id } }'\n";
    for schema in SCHEMAS {
        let given =
            |variables: &str| compile(&me(&format!("{query}      variables: {variables}\n")), Some(schema), &[]);
        assert_eq!(spots(&given("{in: {name: x, nick: ADMIN}}")), [(Code::V2315, 9, 23, 22)], "{}", schema.0);
        let keyed = given("{in: {name: x, '${var.role}': ADMIN}}");
        assert_eq!(spots(&keyed), [(Code::V2315, 9, 23, 31)], "{}", schema.0);
    }
}

#[test]
fn a_value_with_a_cel_or_an_extend_in_it_is_not_checked() {
    let query = "      query: 'mutation Me($in: NewUser!) { createUser(input: $in) { id } }'\n";
    for schema in SCHEMAS {
        let given =
            |variables: &str| compile(&me(&format!("{query}      variables: {variables}\n")), Some(schema), &[]);
        for variables in [
            "{in: {$cel: '{\"name\": \"x\"}'}}",
            "{in: {name: {$cel: '\"x\"'}}}",
            "{in: {$extend: {name: x}, nick: ADMIN}}",
            "{in: [{$extend: {name: x}}]}",
            "{in: {$remove: [role]}}",
        ] {
            let found = given(variables);
            let found: Vec<_> = found.diagnostics.iter().filter(|found| found.code == Code::V2315).collect();
            assert!(found.is_empty(), "{}: {variables}: {found:?}", schema.0);
        }
    }
}

#[test]
fn a_cel_beside_other_keys_is_a_key_and_is_checked() {
    let query = "      query: 'mutation Me($in: NewUser!) { createUser(input: $in) { id } }'\n";
    for schema in SCHEMAS {
        let variables = "{in: {$cel: x, name: y}}";
        let compiled = compile(&me(&format!("{query}      variables: {variables}\n")), Some(schema), &[]);
        assert_eq!(spots(&compiled), [(Code::V2315, 9, 23, 18)], "{}", schema.0);
    }
}

#[test]
fn an_unused_fragment_beside_an_operation_is_v2314_at_the_fragment() {
    let body = "      query: 'query Me { user(id: 1) { id } } fragment F on User { id }'\n";
    for schema in SCHEMAS {
        let compiled = compile(&me(body), Some(schema), &[]);
        assert_eq!(spots(&compiled), [(Code::V2314, 8, 47, "fragment F on User { id }".len())], "{}", schema.0);
        assert_eq!(
            one(&compiled).message,
            "the document does not fit the schema: fragment `F` must be used in an operation"
        );
    }
}

#[test]
fn an_anonymous_operation_beside_a_named_one_is_one_problem_with_one_code() {
    let query = "      query: '{ user(id: 1) { id } } query Me { user(id: 2) { id } }'\n";
    for schema in SCHEMAS {
        let errors = |compiled: &Compiled| {
            let unnamed = |(code, ..): &(Code, u32, u32, usize)| *code == Code::V4010;
            spots(compiled).into_iter().filter(|spot| !unnamed(spot)).collect::<Vec<_>>()
        };
        let picked = compile(&me(&format!("{query}      operationName: Me\n")), Some(schema), &[]);
        assert_eq!(errors(&picked), [(Code::V2314, 8, 15, "{ user(id: 1) { id } }".len())], "{}", schema.0);
        let codes = |compiled: &Compiled| errors(compiled).into_iter().map(|(code, ..)| code).collect::<Vec<_>>();
        assert_eq!(codes(&compile(&me(query), Some(schema), &[])), [Code::V2009], "{}", schema.0);
        let listed = compile(&me(&format!("{query}      operationName: [Me]\n")), Some(schema), &[]);
        assert_eq!(codes(&listed), [Code::V2302], "{}", schema.0);
    }
}

#[test]
fn a_document_with_no_operation_is_v2009_alone() {
    for fragments in ["fragment F on User { id }", "fragment F on Nobody { id }", "fragment F on User { nope }"] {
        for schema in SCHEMAS {
            let compiled = compile(&me(&format!("      query: '{fragments}'\n")), Some(schema), &[]);
            let codes: Vec<Code> = compiled.diagnostics.iter().map(|found| found.code).collect();
            assert_eq!(codes, [Code::V2009], "{fragments}: {}", schema.0);
        }
    }
}
