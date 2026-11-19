use vc_diagnostics::Code;
use vc_model::{ContentHash, SourceFile, SourcePath};

use crate::{CompileInput, Compiled};

const CONFIG: &str =
    "syntaxVersion: 2\nproject: shop\nvars: {baseUrl: 'http://x'}\nprofiles:\n  - id: dev\n    default: true\n";

/// The accounts schema as SDL, and as the introspection result a server answers with.
const SDL: &str = include_str!("../../parser/tests/graphql/accounts.graphql");
const INTROSPECTION: &str = include_str!("../../parser/tests/graphql/accounts.json");

fn file(path: &str, text: &str) -> SourceFile {
    SourceFile { path: SourcePath::new(path).unwrap(), text: text.to_owned(), hash: ContentHash::new([0; 32]) }
}

/// A `graphql` api file of `service` whose `graphql.schema` is `schema`, with one api and a test
/// that calls it.
fn service(service: &str, schema: &str) -> [SourceFile; 2] {
    let api = format!(
        "type: graphql\nbaseUrl: ${{var.baseUrl}}\ngraphql:\n  schema: {schema}\napis:\n  - id: me\n    body:\n      query: 'query Me {{ users {{ id }} }}'\n"
    );
    let test = format!("tests:\n  - id: getsMe\n    call: {service}#me\n");
    [
        file(&format!("services/{service}/{service}.api.yaml"), &api),
        file(&format!("services/{service}/crud.test.yaml"), &test),
    ]
}

/// The users service with `graphql.schema` `schema`, the files `given`, and the project's other
/// files `listed`.
fn compile(schema: &str, given: &[(&str, &str)], listed: &[&str]) -> Compiled {
    let mut files = vec![file("vibranium.config.yaml", CONFIG)];
    files.extend(service("users", schema));
    crate::compile(with(files, given, listed))
}

/// `files` and the files `given`, with those and the files `listed` as the project's assets.
fn with(mut files: Vec<SourceFile>, given: &[(&str, &str)], listed: &[&str]) -> CompileInput {
    files.extend(given.iter().map(|(path, text)| file(path, text)));
    let paths = given.iter().map(|(path, _)| *path).chain(listed.iter().copied());
    let assets = Some(paths.map(|path| SourcePath::new(path).unwrap()).collect());
    CompileInput { files, assets, ..CompileInput::default() }
}

/// The code, line, column and length of each diagnostic.
fn spots(compiled: &Compiled) -> Vec<(Code, u32, u32, usize)> {
    let spot = |found: &vc_diagnostics::Diagnostic| {
        let span = found.primary.span;
        (found.code, span.start.line, span.start.column, span.byte_range().len())
    };
    compiled.diagnostics.iter().map(spot).collect()
}

#[test]
fn a_schema_file_is_wanted_then_read_as_sdl_or_as_an_introspection_result() {
    for (name, text) in
        [("schema.graphql", SDL), ("schema.gql", SDL), ("schema.graphqls", SDL), ("schema.json", INTROSPECTION)]
    {
        let (schema, path) = (format!("{{$file: {name}}}"), format!("services/users/{name}"));
        let wanted = compile(&schema, &[], &[&path]);
        let paths: Vec<&str> = wanted.data_wanted.iter().map(|data| data.path.as_str()).collect();
        assert_eq!(paths, [path.as_str()], "{name}");
        let read = compile(&schema, &[(&path, text)], &[]);
        assert!(read.diagnostics.is_empty() && read.data_wanted.is_empty(), "{name}: {:?}", read.diagnostics);
        assert!(read.program.is_some(), "{name}");
    }
}

#[test]
fn sdl_that_holds_an_operation_is_v1015_at_it() {
    let text = "type Query {\n  me: Int\n}\n\nquery Me { me }\n";
    let compiled = compile("{$file: schema.graphql}", &[("services/users/schema.graphql", text)], &[]);
    assert_eq!(spots(&compiled), [(Code::V1015, 5, 1, "query Me { me }".len())]);
    let found = compiled.diagnostics.iter().next().unwrap();
    assert_eq!(
        found.message,
        "the `graphql.schema` file is not a GraphQL schema: a schema document must not contain an operation definition"
    );
    assert!(found.help.iter().any(|help| help.contains("only type definitions")), "{found:?}");
}

#[test]
fn json_that_is_not_an_introspection_result_is_v1015_at_what_is_wrong() {
    let path = "services/users/schema.json";
    let openapi = compile("{$file: schema.json}", &[(path, "{\"openapi\": \"3.1.0\"}\n")], &[]);
    assert_eq!(spots(&openapi), [(Code::V1015, 1, 1, "{\"openapi\": \"3.1.0\"}".len())]);
    let message = &openapi.diagnostics.iter().next().unwrap().message;
    assert!(message.ends_with("it is not an introspection result: no `__schema` in it"), "{message}");
    let text = "{\"data\": {\"__schema\": {\n  \"types\": [{\"kind\": \"TABLE\", \"name\": \"T\"}]}}}\n";
    let kind = compile("{$file: schema.json}", &[(path, text)], &[]);
    assert_eq!(spots(&kind), [(Code::V1015, 2, 22, "\"TABLE\"".len())]);
}

#[test]
fn an_sdl_file_that_does_not_parse_is_v1014_only() {
    let compiled = compile("{$file: schema.graphqls}", &[("services/users/schema.graphqls", "type Query {\n")], &[]);
    let codes: Vec<Code> = spots(&compiled).into_iter().map(|(code, ..)| code).collect();
    assert_eq!(codes, [Code::V1014]);
}

#[test]
fn a_schema_file_named_by_several_api_files_is_reported_once() {
    let mut files = vec![file("vibranium.config.yaml", CONFIG)];
    files.extend(service("users", "{$file: ../schema.json}"));
    files.extend(service("orders", "{$file: ../schema.json}"));
    let compiled = crate::compile(with(files, &[("services/schema.json", "[]\n")], &[]));
    assert_eq!(spots(&compiled), [(Code::V1015, 1, 1, 2)]);
}

#[test]
fn a_schema_that_is_not_the_literal_file_of_a_schema_is_v2302() {
    let schemas = [
        "schema.graphql",
        "{$file: schema.yaml}",
        "{$file: '${var.name}.json'}",
        "{$file: s.json, x: 1}",
        "{$file: 's#1.json'}",
    ];
    for schema in schemas {
        let compiled = compile(schema, &[], &[]);
        let found: Vec<_> = compiled.diagnostics.iter().filter(|found| found.code == Code::V2302).collect();
        let [found] = found[..] else { panic!("{schema}: {:?}", compiled.diagnostics) };
        assert!(found.message.starts_with("`graphql.schema` takes the literal `$file` path"), "{schema}");
        assert!(compiled.data_wanted.is_empty(), "{schema}");
    }
}

#[test]
fn sdl_that_does_not_validate_is_v1015_at_the_part_at_fault() {
    let text = "type Query {\n  me: Person\n}\n";
    let compiled = compile("{$file: schema.gql}", &[("services/users/schema.gql", text)], &[]);
    assert_eq!(spots(&compiled), [(Code::V1015, 2, 7, "Person".len())]);
    let message = &compiled.diagnostics.iter().next().unwrap().message;
    assert!(message.ends_with("cannot find type `Person` in this document"), "{message}");
}

#[test]
fn an_introspected_type_with_nothing_in_it_is_v1015_at_the_type() {
    let text = "{\"data\": {\"__schema\": {\"queryType\": {\"name\": \"Query\"}, \"types\": [\n  {\"kind\": \"OBJECT\", \"name\": \"Query\", \"fields\": null}]}}}\n";
    let compiled = compile("{$file: schema.json}", &[("services/users/schema.json", text)], &[]);
    let line = "  {\"kind\": \"OBJECT\", \"name\": \"Query\", \"fields\": null}";
    assert_eq!(spots(&compiled), [(Code::V1015, 2, 3, line.len() - 2)]);
}

#[test]
fn a_schema_under_datasets_is_no_dataset() {
    let schema = "{$file: ../../datasets/schema.json}";
    let compiled = compile(schema, &[("datasets/schema.json", INTROSPECTION)], &[]);
    assert!(compiled.diagnostics.is_empty(), "{:?}", compiled.diagnostics);
    let rows = compile(schema, &[("datasets/schema.json", INTROSPECTION), ("datasets/rows.json", "{}\n")], &[]);
    assert_eq!(spots(&rows).into_iter().map(|(code, ..)| code).collect::<Vec<_>>(), [Code::V1002]);
}

#[test]
fn a_schema_is_read_once_and_kept_with_its_file() {
    for (path, good, bad) in [("schema.graphqls", SDL, "type T"), ("schema.json", INTROSPECTION, "{\"__schema\": {}}")]
    {
        let path = format!("services/users/{path}");
        let schema = format!("{{$file: {}}}", &path["services/users/".len()..]);
        let first = compile(&schema, &[(&path, good)], &[]);
        assert!(first.diagnostics.is_empty(), "{path}: {:?}", first.diagnostics);
        let mut input = with(vec![file("vibranium.config.yaml", CONFIG)], &[(&path, bad)], &[]);
        input.files.extend(service("users", &schema));
        input.files.push(file("services/users/views.test.yaml", "tests:\n  - id: getsMeAgain\n    call: users#me\n"));
        input.previous = first.cache;
        let kept = crate::compile(input);
        assert!(kept.diagnostics.is_empty(), "{path}: the stored schema was used: {:?}", kept.diagnostics);
    }
}
