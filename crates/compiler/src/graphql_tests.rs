use vc_diagnostics::{Code, Diagnostic};
use vc_model::{BodySpec, ContentHash, SourceFile, SourcePath};

use crate::{CompileInput, Compiled};

const CONFIG: &str =
    "syntaxVersion: 2\nproject: shop\nvars: {baseUrl: 'http://x'}\nprofiles:\n  - id: dev\n    default: true\n";

const DOCUMENT: &str = "services/users/me.graphql";

fn file(path: &str, text: &str) -> SourceFile {
    SourceFile { path: SourcePath::new(path).unwrap(), text: text.to_owned(), hash: ContentHash::new([0; 32]) }
}

/// Files the compile is given, and the paths of the project's other files.
type Files<'f> = (&'f [(&'f str, &'f str)], &'f [&'f str]);

/// The users apis with one `graphql` api whose `body` is `body`, the files `given`, and the
/// project's other files `listed` next to them.
fn compile(body: &str, (given, listed): Files<'_>) -> Compiled {
    let apis = format!("type: graphql\nbaseUrl: ${{var.baseUrl}}\napis:\n  - id: me\n    body:\n{body}");
    let mut files = vec![
        file("vibranium.config.yaml", CONFIG),
        file("services/users/users.api.yaml", &apis),
        file("services/users/crud.test.yaml", "tests:\n  - id: getsMe\n    call: users#me\n"),
    ];
    files.extend(given.iter().map(|(path, text)| file(path, text)));
    let paths = given.iter().map(|(path, _)| *path).chain(listed.iter().copied());
    let assets = Some(paths.map(|path| SourcePath::new(path).unwrap()).collect());
    crate::compile(CompileInput { files, assets, ..CompileInput::default() })
}

/// The one diagnostic `body` gives with a document file `text`.
fn one(body: &str, text: &str) -> Diagnostic {
    let compiled = compile(body, (&[(DOCUMENT, text)], &[]));
    let [found] = compiled.diagnostics.iter().collect::<Vec<_>>()[..] else { panic!("{:?}", compiled.diagnostics) };
    found.clone()
}

/// The code, line, column and length of each diagnostic.
fn spots(compiled: &Compiled) -> Vec<(Code, u32, u32, usize)> {
    let spot = |found: &Diagnostic| {
        let span = found.primary.span;
        (found.code, span.start.line, span.start.column, span.byte_range().len())
    };
    compiled.diagnostics.iter().map(spot).collect()
}

#[test]
fn an_inline_document_that_does_not_parse_is_v1014_at_its_spot_in_the_yaml() {
    let inline = compile("      query: 'query { user(id: ) { id } }'\n", (&[], &[]));
    assert_eq!(spots(&inline), [(Code::V1014, 6, 32, 1)]);
    let message = &inline.diagnostics.iter().next().unwrap().message;
    assert!(message.starts_with("the GraphQL document does not parse: expected"), "{message}");
    let block = compile("      query: |\n        query {\n          user(id: ) { id }\n        }\n", (&[], &[]));
    assert_eq!(spots(&block), [(Code::V1014, 8, 20, 1)]);
    let unclosed = compile("      query: |\n        query { me\n      operationName: X\n", (&[], &[]));
    assert_eq!(spots(&unclosed)[0].0, Code::V1014);
    assert_eq!(spots(&unclosed)[0].1, 7, "the end of the document is the end of the block");
    let folded = compile("      query: >\n        query { user(id: ) }\n", (&[], &[]));
    assert_eq!(spots(&folded)[0].0, Code::V1014);
    assert_eq!(spots(&folded)[0].1, 7, "a folded document is underlined as a whole");
}

#[test]
fn a_document_file_is_wanted_then_read_into_the_body() {
    let body = "      query: {$file: me.graphql}\n";
    let wanted = compile(body, (&[], &[DOCUMENT]));
    let paths: Vec<&str> = wanted.data_wanted.iter().map(|data| data.path.as_str()).collect();
    assert_eq!(paths, [DOCUMENT]);
    let text = "query Me { me { id } }\n";
    let read = compile(body, (&[(DOCUMENT, text)], &[]));
    assert!(read.diagnostics.is_empty() && read.data_wanted.is_empty(), "{:?}", read.diagnostics);
    let program = read.program.unwrap();
    let [BodySpec::Graphql(sent)] = &program.apis.values().map(|api| &api.body).collect::<Vec<_>>()[..] else {
        panic!("{:?}", program.apis)
    };
    assert_eq!(sent.document, text);
    let missing = compile(body, (&[], &[]));
    assert_eq!(spots(&missing).iter().map(|spot| spot.0).collect::<Vec<_>>(), [Code::V2004]);
}

#[test]
fn a_document_file_that_does_not_parse_is_v1014_in_the_file() {
    let found = one("      query: {$file: me.graphql}\n", "query Me {\n  me { id\n}\n");
    assert_eq!((found.code, found.primary.span.start.line, found.primary.span.start.column), (Code::V1014, 4, 1));
    assert_ne!(found.primary.span.file, one("      query: '{ x( }'\n", "{ me }").primary.span.file);
}

#[test]
fn operation_name_picks_one_query_or_mutation_of_the_document() {
    let two = "      query: |\n        query ListUsers { users { id } }\n        mutation AddUser { add }\n";
    assert_eq!(
        spots(&compile(two, (&[], &[])))[..].iter().map(|spot| (spot.0, spot.1, spot.2)).collect::<Vec<_>>(),
        [(Code::V2009, 7, 9)]
    );
    assert!(compile(&format!("{two}      operationName: AddUser\n"), (&[], &[])).diagnostics.is_empty());
    let misspelt = compile(&format!("{two}      operationName: ListUser\n"), (&[], &[]));
    let found = misspelt.diagnostics.iter().next().unwrap();
    assert_eq!((found.code, found.primary.span.start.line), (Code::V2009, 9));
    assert_eq!(found.fixes[0].title, "Replace with ListUsers");
    let watched = compile("      query: 'subscription OnUser { user { id } }'\n", (&[], &[]));
    assert_eq!(spots(&watched).iter().map(|spot| (spot.0, spot.1)).collect::<Vec<_>>(), [(Code::V2009, 6)]);
    let filed = "      query: {$file: me.graphql}\n";
    let text = "query A { a }\nquery B { b }\n";
    assert_eq!((one(filed, text).code, one(filed, text).primary.span.start.line), (Code::V2009, 6));
    let named = compile(&format!("{filed}      operationName: B\n"), (&[(DOCUMENT, text)], &[]));
    assert!(named.diagnostics.is_empty(), "{:?}", named.diagnostics);
}

/// The text of each file a diagnostic's first fix edits, once edited.
fn fixed(compiled: &Compiled, found: &Diagnostic) -> String {
    let [edit] = &found.fixes[0].edits[..] else { panic!("{:?}", found.fixes) };
    let mut text = compiled.files.get(edit.file).unwrap().text.clone();
    text.replace_range(edit.range.clone(), &edit.new_text);
    text
}

#[test]
fn each_needed_variable_has_a_value_and_each_value_a_variable() {
    let query = "      query: 'query Q($id: ID!, $limit: Int!) { a }'\n";
    assert_eq!(spots(&compile(query, (&[], &[]))), [(Code::V2108, 5, 5, 4), (Code::V2108, 5, 5, 4)]);
    let given = |variables: &str| compile(&format!("{query}      variables: {variables}\n"), (&[], &[]));
    assert!(given("{id: '1', limit: 2}").diagnostics.is_empty());
    let misspelt = given("{ids: '1', limit: 2}");
    assert_eq!(spots(&misspelt), [(Code::V2108, 7, 7, 9), (Code::V2108, 7, 19, 3)]);
    let unknown = misspelt.diagnostics.iter().nth(1).unwrap();
    assert!(fixed(&misspelt, unknown).contains("{id: '1', limit: 2}"));
    let defaulted = compile("      query: 'query Q($id: ID! = 1, $tag: String) { a }'\n", (&[], &[]));
    assert!(defaulted.diagnostics.is_empty(), "{:?}", defaulted.diagnostics);
    let picked = "      query: 'query A { a } query B($id: ID!) { b }'\n      operationName: A\n";
    assert!(compile(picked, (&[], &[])).diagnostics.is_empty());
    assert_eq!(spots(&compile(&picked.replace(": A", ": B"), (&[], &[])))[0].0, Code::V2108);
}

#[test]
fn variables_written_as_one_template_are_not_checked() {
    let whole = compile("      query: 'query Q($id: ID!) { a }'\n      variables: ${var.filters}\n", (&[], &[]));
    assert!(!whole.diagnostics.iter().any(|found| found.code == Code::V2108), "{:?}", whole.diagnostics);
}

#[test]
fn an_anonymous_operation_is_named_after_its_api() {
    let shorthand = compile("      query: '{ me { id } }'\n", (&[], &[]));
    let found = shorthand.diagnostics.iter().next().unwrap();
    assert_eq!(spots(&shorthand), [(Code::V4010, 6, 15, 1)]);
    assert!(fixed(&shorthand, found).contains("query: 'query Me { me { id } }'"));
    let keyword = compile("      query: |\n        query {\n          me { id }\n        }\n", (&[], &[]));
    let found = keyword.diagnostics.iter().next().unwrap();
    assert!(fixed(&keyword, found).contains("\n        query Me {\n"), "{}", fixed(&keyword, found));
    let filed = compile("      query: {$file: me.graphql}\n", (&[(DOCUMENT, "mutation { me }\n")], &[]));
    let found = filed.diagnostics.iter().next().unwrap();
    assert_eq!(fixed(&filed, found), "mutation Me { me }\n");
    let two = compile("      query: 'query { a } query B { b }'\n      operationName: B\n", (&[], &[]));
    assert_eq!(
        two.diagnostics.iter().map(|found| (found.code, found.fixes.len())).collect::<Vec<_>>(),
        [(Code::V4010, 0)]
    );
}
