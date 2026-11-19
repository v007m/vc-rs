//! A `type: graphql` api file's schema (D347): its `graphql.schema` names an SDL file (`.graphql`,
//! `.gql`, `.graphqls`) or an introspection result (`.json`), which the engine reads for the
//! compile (D132). Each file's per-file stage reads what it holds as a schema once, which the
//! compile cache keeps ([`from_text`], [`from_tree`]); an SDL file that does not parse is V1014
//! there, and V1015 is a file that reads but is not a schema.

use std::borrow::Cow;

use indexmap::IndexSet;
use vc_diagnostics::{Code, Diagnostic};
use vc_model::{FileKind, SourceFile, SourceFileId, SourceFiles, SourceSpan, resolve_path};
use vc_parser::{GraphqlDocument, GraphqlProblem, GraphqlSchema, Node};

use crate::graphql::{self, Documents};
use crate::link::{self, Project, Source};

/// The `graphql` key that names the schema.
const SCHEMA: &str = "schema";

/// The suffix of an SDL file that holds only a schema.
pub const SDL_SUFFIX: &str = ".graphqls";

/// The suffixes of a schema file.
const SUFFIXES: [&str; 4] = [".graphql", ".gql", SDL_SUFFIX, ".json"];

/// What a file holds as a schema, else where it is not one.
pub type Read = Result<GraphqlSchema, Vec<GraphqlProblem>>;

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
    resolve_path(&link::folder(source.path), graphql::literal_file(node(source)?, is_schema)?)
}

/// V2302 when the `graphql.schema` of `source` is not a literal `$file` of a schema file.
pub fn check(source: &Source<'_>) -> Option<Diagnostic> {
    let node = node(source).filter(|node| graphql::literal_file(node, is_schema).is_none())?;
    let message =
        "`graphql.schema` takes the literal `$file` path of a `.graphql`, `.gql`, `.graphqls` or `.json` file";
    let diagnostic = Diagnostic::new(Code::V2302, node.span, message).label("not a schema file");
    Some(diagnostic.help("name the schema file as written, such as `schema: {$file: schema.graphql}`"))
}

/// V1015 for each schema file the api files of `project` name that is not a schema: each file once,
/// however many name it; a file the compile was not given is left alone.
pub fn problems(project: &Project<'_>, documents: &Documents<'_>) -> Vec<Diagnostic> {
    let paths: IndexSet<String> = project.sources.iter().filter_map(path).collect();
    paths.iter().filter_map(|path| documents.schema(path)?.err()).cloned().collect()
}

/// The per-file stage of a GraphQL text file: what it holds as a schema, when it parses as
/// `document` and holds no operation.
pub fn from_text(source: &SourceFile, document: Option<&GraphqlDocument>) -> Option<Read> {
    document?.operations.is_empty().then(|| vc_parser::parse_schema(&source.text))
}

/// The per-file stage of a data file: what it holds as a schema, when it holds a `__schema`.
pub fn from_tree(root: &Node) -> Option<Read> {
    root.get("data").unwrap_or(root).get("__schema")?;
    Some(introspection(root))
}

/// The schema in `file`, from what its per-file stage `cached` when it read it as one; else
/// V1015 where it is not one.
pub fn read<'a>(
    file: &SchemaFile<'_>,
    cached: Option<&'a Read>,
    files: &SourceFiles,
) -> Result<Cow<'a, GraphqlSchema>, Box<Diagnostic>> {
    match cached {
        Some(Ok(schema)) => Ok(Cow::Borrowed(schema)),
        Some(Err(problems)) => Err(not_a_schema(file, problems, files)),
        None => fresh(file).map(Cow::Owned).map_err(|problems| not_a_schema(file, &problems, files)),
    }
}

/// Whether `path` names a schema file.
fn is_schema(path: &str) -> bool {
    SUFFIXES.iter().any(|suffix| path.ends_with(suffix))
}

/// What `file` holds as a schema, read now: a file its per-file stage did not read as one.
fn fresh(file: &SchemaFile<'_>) -> Read {
    match *file {
        SchemaFile::Sdl(_, text) => vc_parser::parse_schema(text),
        SchemaFile::Introspection(root) => introspection(root),
    }
}

/// The introspection result `root` as a schema.
fn introspection(root: &Node) -> Read {
    vc_parser::schema_from_introspection(root).map_err(|problem| vec![problem])
}

/// V1015 at the first of `problems`, which keep `file` from being a schema.
fn not_a_schema(file: &SchemaFile<'_>, problems: &[GraphqlProblem], files: &SourceFiles) -> Box<Diagnostic> {
    let (span, first, help) = match *file {
        SchemaFile::Sdl(id, text) => {
            let (span, first) = graphql::first_spot(id, text, problems);
            (
                span,
                first,
                "fix the schema here; it holds only type definitions (`type`, `input`, `enum`, …), and operations go in an api's `query`",
            )
        }
        SchemaFile::Introspection(root) => {
            let first = problems.first();
            (
                json_span(root, first, files),
                first,
                "save the whole answer to an introspection query, `{\"data\": {\"__schema\": …}}`, or write the schema as SDL",
            )
        }
    };
    let why = first.map_or("", |problem| problem.message.as_str());
    let message = format!("the `graphql.schema` file is not a GraphQL schema: {why}");
    Box::new(Diagnostic::new(Code::V1015, span, message).label("not part of a schema").help(help))
}

/// Where `problem` is in the JSON file of `root`; the whole file when it cannot be placed.
fn json_span(root: &Node, problem: Option<&GraphqlProblem>, files: &SourceFiles) -> SourceSpan {
    let place = |offset| files.position(root.span.file, offset);
    let spot = problem.and_then(|problem| place(problem.at.start).zip(place(problem.at.end)));
    spot.map_or(root.span, |(start, end)| SourceSpan::new(root.span.file, start, end))
}

/// The `graphql.schema` of `source`, an api file.
fn node<'n>(source: &Source<'n>) -> Option<&'n Node> {
    let graphql = source.root.get("graphql").filter(|_| source.kind == FileKind::Api)?;
    graphql.get(SCHEMA)
}

#[cfg(test)]
#[path = "graphql_schema_tests.rs"]
mod tests;
