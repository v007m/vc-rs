use vc_diagnostics::{Code, Diagnostic};
use vc_model::{ContentHash, SourceFile, SourcePath};

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

/// The code, line, column and length of each diagnostic.
fn spots(compiled: &Compiled) -> Vec<(Code, u32, u32, usize)> {
    let spot = |found: &Diagnostic| {
        let span = found.primary.span;
        (found.code, span.start.line, span.start.column, span.byte_range().len())
    };
    compiled.diagnostics.iter().map(spot).collect()
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
fn an_input_object_with_a_templated_key_is_not_checked() {
    let query = "      query: 'mutation Me($in: NewUser!) { createUser(input: $in) { id } }'\n";
    for schema in SCHEMAS {
        let given =
            |variables: &str| compile(&me(&format!("{query}      variables: {variables}\n")), Some(schema), &[]);
        assert_eq!(spots(&given("{in: {name: x, nick: ADMIN}}")), [(Code::V2315, 9, 23, 22)], "{}", schema.0);
        let templated = given("{in: {name: x, '${var.role}': ADMIN}}");
        assert!(templated.diagnostics.is_empty(), "{}: {:?}", schema.0, templated.diagnostics);
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
        ] {
            let found = given(variables);
            let found: Vec<_> = found.diagnostics.iter().filter(|found| found.code == Code::V2315).collect();
            assert!(found.is_empty(), "{}: {variables}: {found:?}", schema.0);
        }
    }
}
