//! A `type: graphql` api file's schema (D347): its `graphql.schema` names an SDL file (`.graphql`,
//! `.graphqls`) or an introspection result (`.json`), which the engine reads for the compile
//! (D132). An SDL file is parsed at its per-file stage as any GraphQL file is, so a syntax error
//! is V1014 there, and a JSON file is read as any data file is; V1015 is a file that reads but is
//! not a schema.

use indexmap::IndexSet;
use vc_diagnostics::{Code, Diagnostic};
use vc_model::{FILE_KEY, FileKind, SourceFileId, SourceFiles, SourceSpan, resolve_path};
use vc_parser::{GraphqlProblem, GraphqlSchema, Node};

use crate::graphql::{self, Documents};
use crate::link::{self, Project, Source};

/// The `graphql` key that names the schema.
const SCHEMA: &str = "schema";

/// The suffix of an SDL file that holds only a schema.
pub const SDL_SUFFIX: &str = ".graphqls";

/// The suffixes of a schema file.
const SUFFIXES: [&str; 3] = [".graphql", SDL_SUFFIX, ".json"];

/// A schema file the compile was given.
pub enum SchemaFile<'a> {
    /// SDL: the file, and its text.
    Sdl(SourceFileId, &'a str),
    /// An introspection result, as its JSON tree.
    Introspection(&'a Node),
}

/// The schema file the api file `source` names, from the project root; `None` when it names
/// none, or not as a literal `$file` of a schema file.
pub fn path(source: &Source<'_>) -> Option<String> {
    resolve_path(&link::folder(source.path), literal(node(source)?)?)
}

/// V2302 when the `graphql.schema` of `source` is not a literal `$file` of a schema file.
pub fn check(source: &Source<'_>) -> Option<Diagnostic> {
    let node = node(source).filter(|node| literal(node).is_none())?;
    let message = "`graphql.schema` takes the literal `$file` path of a `.graphql`, `.graphqls` or `.json` file";
    let diagnostic = Diagnostic::new(Code::V2302, node.span, message).label("not a schema file");
    Some(diagnostic.help("name the schema file as written, such as `schema: {$file: schema.graphql}`"))
}

/// V1015 for each schema file the api files of `project` name that is not a schema: each file once,
/// however many name it; a file the compile was not given is left alone.
pub fn problems(project: &Project<'_>, documents: &Documents<'_>) -> Vec<Diagnostic> {
    let paths: IndexSet<String> = project.sources.iter().filter_map(path).collect();
    paths.iter().filter_map(|path| documents.schema(path)?.err()).map(|diagnostic| *diagnostic).collect()
}

/// Reads `file` as a schema, else V1015 where it is not one.
pub fn read(file: &SchemaFile<'_>, files: &SourceFiles) -> Result<GraphqlSchema, Box<Diagnostic>> {
    match *file {
        SchemaFile::Sdl(id, text) => sdl(id, text),
        SchemaFile::Introspection(root) => introspection(root, files),
    }
}

/// The SDL `text` of file `id` as a schema, else V1015 at its first operation or fragment.
fn sdl(id: SourceFileId, text: &str) -> Result<GraphqlSchema, Box<Diagnostic>> {
    vc_parser::parse_schema(text).map_err(|problems| {
        let first = problems.first();
        let span = graphql::file_span(id, text, &first.map_or(0..0, |problem| problem.at.clone()));
        let help = "write only type definitions (`type`, `input`, `enum`, …) in it; operations go in an api's `query`";
        Box::new(not_a_schema(span, first, help))
    })
}

/// The introspection result `root` as a schema, else V1015 at the value that is wrong.
fn introspection(root: &Node, files: &SourceFiles) -> Result<GraphqlSchema, Box<Diagnostic>> {
    vc_parser::schema_from_introspection(root).map_err(|problem| {
        let place = |offset| files.position(root.span.file, offset);
        let spot = place(problem.at.start).zip(place(problem.at.end));
        let span = spot.map_or(root.span, |(start, end)| SourceSpan::new(root.span.file, start, end));
        let help = "save the whole answer to an introspection query, `{\"data\": {\"__schema\": …}}`, or write the schema as SDL";
        Box::new(not_a_schema(span, Some(&problem), help))
    })
}

/// V1015 for `problem`, at `span`.
fn not_a_schema(span: SourceSpan, problem: Option<&GraphqlProblem>, help: &str) -> Diagnostic {
    let why = problem.map_or("", |problem| problem.message.as_str());
    let message = format!("the `graphql.schema` file is not a GraphQL schema: {why}");
    Diagnostic::new(Code::V1015, span, message).label("not part of a schema").help(help)
}

/// The `graphql.schema` of `source`, an api file.
fn node<'n>(source: &Source<'n>) -> Option<&'n Node> {
    let graphql = source.root.get("graphql").filter(|_| source.kind == FileKind::Api)?;
    graphql.get(SCHEMA)
}

/// The path `node` names when it is `{$file: <path>}`, literal, with a schema file's suffix.
fn literal(node: &Node) -> Option<&str> {
    let [only] = node.entries() else { return None };
    let path = only.value.as_str().filter(|path| only.key == FILE_KEY && !path.contains("${"))?;
    SUFFIXES.iter().any(|suffix| path.ends_with(suffix)).then_some(path)
}

#[cfg(test)]
#[path = "graphql_schema_tests.rs"]
mod tests;
