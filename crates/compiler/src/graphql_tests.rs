use vc_diagnostics::{Code, Diagnostic, Severity};
use vc_model::{BodySpec, ContentHash, SourceFile, SourcePath};

use crate::testing::spots;
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
fn compile(body: &str, files: Files<'_>) -> Compiled {
    project(&format!("  - id: me\n    body:\n{body}"), files)
}

/// The users apis `apis`, each called by a test, the files `given`, and the project's other files
/// `listed` next to them.
fn project(apis: &str, files: Files<'_>) -> Compiled {
    crate::compile(input(apis, files))
}

/// The input [`project`] compiles.
fn input(apis: &str, (given, listed): Files<'_>) -> CompileInput {
    let ids = apis.lines().filter_map(|line| line.strip_prefix("  - id: "));
    let call = |id: &str| {
        let (head, tail) = id.split_at(1);
        format!("  - id: gets{}{tail}\n    call: users#{id}\n", head.to_uppercase())
    };
    let calls: String = ids.map(call).collect();
    let mut files = vec![
        file("vibranium.config.yaml", CONFIG),
        file("services/users/users.api.yaml", &format!("type: graphql\nbaseUrl: ${{var.baseUrl}}\napis:\n{apis}")),
        file("services/users/crud.test.yaml", &format!("tests:\n{calls}")),
    ];
    files.extend(given.iter().map(|(path, text)| file(path, text)));
    let paths = given.iter().map(|(path, _)| *path).chain(listed.iter().copied());
    let assets = Some(paths.map(|path| SourcePath::new(path).unwrap()).collect());
    CompileInput { files, assets, ..CompileInput::default() }
}

/// The one diagnostic `body` gives with a document file `text`.
fn one(body: &str, text: &str) -> Diagnostic {
    let compiled = compile(body, (&[(DOCUMENT, text)], &[]));
    let [found] = compiled.diagnostics.iter().collect::<Vec<_>>()[..] else { panic!("{:?}", compiled.diagnostics) };
    found.clone()
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
    assert_eq!(spots(&misspelt), [(Code::V2108, 7, 7, 9), (Code::V2109, 7, 19, 3)]);
    let unknown = misspelt.diagnostics.iter().nth(1).unwrap();
    let severities: Vec<Severity> = misspelt.diagnostics.iter().map(|found| found.severity).collect();
    assert_eq!(severities, [Severity::Error, Severity::Warning], "a server ignores an undeclared variable (D358)");
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

#[test]
fn a_file_query_map_takes_only_its_file() {
    let flow = compile("      query: {$file: me.graphql, note: x}\n", (&[(DOCUMENT, "query Me { me }\n")], &[]));
    assert_eq!(spots(&flow), [(Code::V2302, 6, 34, 4)]);
    assert!(flow.diagnostics.iter().next().unwrap().fixes.is_empty(), "a key that shares its line keeps it");
    let block = compile(
        "      query:\n        $file: me.graphql\n        note: x\n",
        (&[(DOCUMENT, "query Me { me }\n")], &[]),
    );
    let found = block.diagnostics.iter().next().unwrap();
    assert_eq!(
        (found.code, &found.message[..]),
        (Code::V2302, "a `query` `$file` map takes only `$file`, found `note`")
    );
    assert!(fixed(&block, found).ends_with("    body:\n      query:\n        $file: me.graphql\n"));
    let renamed = compile("      query: {path: me.graphql}\n", (&[], &[DOCUMENT]));
    assert!(renamed.data_wanted.is_empty(), "only `$file` names a document file");
    let templated = "services/users/${var.doc}.graphql";
    let wanted = compile("      query: {$file: '${var.doc}.graphql'}\n", (&[], &[templated]));
    assert!(wanted.data_wanted.is_empty(), "a templated path is never read");
}

#[test]
fn two_operations_of_one_name_are_v2009() {
    let twice = "      query: 'query A { a } query A { b }'\n";
    let named = compile(&format!("{twice}      operationName: A\n"), (&[], &[]));
    let found = named.diagnostics.iter().next().unwrap();
    assert_eq!(spots(&named), [(Code::V2009, 6, 29, 5)]);
    assert_eq!(found.message, "the document holds more than one operation named `A`");
    let codes: Vec<Code> = spots(&compile(twice, (&[], &[]))).iter().map(|spot| spot.0).collect();
    assert_eq!(codes, [Code::V2009, Code::V2009], "the name twice, and no `operationName`");
    let shared = "  - id: me\n    body: {query: {$file: me.graphql}, operationName: A}\n  - id: you\n    body: {query: {$file: me.graphql}, operationName: B}\n";
    let once = project(shared, (&[(DOCUMENT, "query A { a }\nquery A { b }\nquery B { c }\n")], &[]));
    assert_eq!(spots(&once).iter().map(|spot| (spot.0, spot.1)).collect::<Vec<_>>(), [(Code::V2009, 2)]);
    let pairs =
        compile("      query: 'query A{a} query A{b} query B{c} query B{d}'\n      operationName: A\n", (&[], &[]));
    let messages: Vec<&str> = pairs.diagnostics.iter().map(|found| &found.message[..]).collect();
    assert_eq!(messages.len(), 2, "each name once: {messages:?}");
    assert!(messages[0].ends_with("`A`") && messages[1].ends_with("`B`"), "{messages:?}");
}

#[test]
fn a_document_with_no_operation_is_v2009() {
    let found = one("      query: {$file: me.graphql}\n", "fragment Fields on User { id }\n");
    assert_eq!((found.code, &found.message[..]), (Code::V2009, "the document holds no operation (query or mutation)"));
    assert!(!found.help.iter().any(|help| help.contains("operationName")), "{:?}", found.help);
    let named =
        one("      query: {$file: me.graphql}\n      operationName: Fields\n", "fragment Fields on User { id }\n");
    assert_eq!(named.message, "the document holds no operation (query or mutation)", "even with `operationName`");
}

#[test]
fn a_document_file_offset_is_clamped_to_a_character_boundary() {
    let (id, text) = (vc_model::SourceFileId::new(3), "a\nZo\u{eb}");
    let at = |range: std::ops::Range<usize>| {
        let span = super::file_span(id, text, &range);
        (span.start.line, span.start.column, span.start.offset, span.end.column, span.end.offset)
    };
    assert_eq!(at(2..3), (2, 1, 2, 2, 3));
    assert_eq!(at(5..99), (2, 3, 4, 4, 6), "inside `\u{eb}` moves back to its start; past the text, to its end");
}

#[test]
fn a_document_file_is_read_once_by_its_cached_stage() {
    let (api, given) =
        ("  - id: me\n    body: {query: {$file: me.graphql}}\n", [(DOCUMENT, "query A { a }\nquery A { b }\n")]);
    let first = project(api, (&given, &[]));
    let cache = crate::CompileCache::from_bytes(&first.cache.to_bytes());
    let id = vc_model::SourceFileId::new(1);
    let unit = cache.unit(&SourcePath::new(DOCUMENT).unwrap(), &ContentHash::new([0; 32]), id).unwrap();
    assert_eq!(unit.document.as_ref().map(|document| document.operations.len()), Some(2), "stored in the bytes");
    let again = crate::compile(CompileInput { previous: cache, ..input(api, (&given, &[])) });
    assert_eq!(spots(&again), spots(&first), "the cached unit holds the operations");
    assert_eq!(spots(&first)[0].0, Code::V2009);
}

#[test]
fn several_operations_are_picked_by_name_or_named_first() {
    let help = |query: &str| compile(query, (&[], &[])).diagnostics.iter().next().unwrap().help.clone();
    assert_eq!(
        help("      query: 'query A { a } query B { b }'\n"),
        ["name the one to run under `body`, such as `operationName: A`"]
    );
    let anonymous = help("      query: '{ a } mutation { b }'\n");
    assert_eq!(anonymous, ["name the operations, such as `query GetUser`, and pick one with `operationName`"]);
}

#[test]
fn a_shared_document_is_named_once_and_left_to_its_author() {
    let shared =
        "  - id: me\n    body: {query: {$file: me.graphql}}\n  - id: you\n    body: {query: {$file: me.graphql}}\n";
    let compiled = project(shared, (&[(DOCUMENT, "{ me }\n")], &[]));
    let named: Vec<_> = compiled.diagnostics.iter().filter(|found| found.code == Code::V4010).collect();
    let [found] = named[..] else { panic!("{:?}", compiled.diagnostics) };
    assert!(found.fixes.is_empty() && found.help == ["name the operation, such as `query GetUser`"], "{found:?}");
}

#[test]
fn the_name_an_api_gives_its_operation_keeps_the_keyword() {
    let help = |apis: &str, text: &str| {
        let compiled = project(apis, (&[(DOCUMENT, text)], &[]));
        let found = compiled.diagnostics.iter().find(|found| found.code == Code::V4010).unwrap().clone();
        (found.help, found.fixes.first().map(|fix| fix.title.clone()))
    };
    let me = "  - id: me\n    body: {query: {$file: me.graphql}}\n";
    assert_eq!(
        help(me, "mutation { me }\n"),
        (vec!["name it after the api: `mutation Me`".to_owned()], Some("Name it Me".to_owned()))
    );
    assert_eq!(help(me, "{ me }\n").0, ["name it after the api: `query Me`"]);
    let snake = "  - id: get_me\n    body: {query: {$file: me.graphql}}\n";
    assert_eq!(help(snake, "{ me }\n").1.as_deref(), Some("Name it Get_me"));
    let dashed = "  - id: get-me\n    body: {query: {$file: me.graphql}}\n";
    assert_eq!(help(dashed, "{ me }\n").1, None, "`get-me` is no GraphQL name");
}

#[test]
fn a_document_placed_only_roughly_is_underlined_whole() {
    let folded = compile("      query: >-\n        query { user(id: ) }\n", (&[], &[]));
    assert_eq!(spots(&folded), [(Code::V1014, 7, 9, 21)], "a folded document is never placed exactly");
    let escaped = compile("      query: '''{ a( }'''\n", (&[], &[]));
    assert_eq!(spots(&escaped), [(Code::V1014, 6, 14, 12)], "an escaped one is not either");
}
