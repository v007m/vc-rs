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
