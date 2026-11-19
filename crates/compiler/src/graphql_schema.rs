//! A `type: graphql` api file's schema (D347): its `graphql.schema` names an SDL file (`.graphql`,
//! `.gql`, `.graphqls`) or an introspection result (`.json`), which the engine reads for the
//! compile (D132). The per-file stage of a GraphQL text file that defines types, and of a data
//! file with a `__schema`, reads it once as a schema, or as the problem that keeps it from being
//! one, and the compile cache keeps that ([`from_text`], [`from_tree`]); any other file an api
//! file names is read when it is named ([`read`]). A file that is not a schema is V1015 when an
//! api file names it, and an SDL file that does not parse is V1014 at its per-file stage.

use std::borrow::Cow;

use vc_diagnostics::{Code, Diagnostic};
use vc_model::{FileKind, SourceFile, SourceFileId, SourceSpan, resolve_path};
use vc_parser::{GraphqlDocument, GraphqlProblem, GraphqlSchema, Node};

use crate::graphql::{self, Documents};
use crate::link::{self, Source};

/// The `graphql` key that names the schema.
const SCHEMA: &str = "schema";

/// The suffix of an SDL file that holds only a schema.
pub const SDL_SUFFIX: &str = ".graphqls";

/// The suffixes of a schema file.
const SUFFIXES: [&str; 4] = [".graphql", ".gql", SDL_SUFFIX, ".json"];

/// What a file holds as a schema, else the first problem that keeps it from being one, which is
/// all its V1015 reports.
pub type Loaded = Result<GraphqlSchema, GraphqlProblem>;

/// A schema file the compile was given, by how its kind is read and reported.
pub trait SchemaFile {
    /// What the file holds as a schema.
    fn read(&self) -> Loaded;

    /// Where `problem` is in the file.
    fn place(&self, problem: &GraphqlProblem) -> SourceSpan;

    /// How to fix a file of its kind that is not a schema.
    fn help(&self) -> &'static str;
}

/// A schema file of SDL: the file, and its text.
pub struct Sdl<'a>(pub SourceFileId, pub &'a str);

/// A schema file that is an introspection result: its JSON tree, and its text.
pub struct Introspection<'a>(pub &'a Node, pub &'a str);

impl SchemaFile for Sdl<'_> {
    fn read(&self) -> Loaded {
        vc_parser::parse_schema(self.1)
    }

    fn place(&self, problem: &GraphqlProblem) -> SourceSpan {
        graphql::file_span(self.0, self.1, &problem.at)
    }

    fn help(&self) -> &'static str {
        "fix the schema here; it holds only type definitions (`type`, `input`, `enum`, …), and operations go in an api's `query`"
    }
}

impl SchemaFile for Introspection<'_> {
    fn read(&self) -> Loaded {
        vc_parser::schema_from_introspection(self.0)
    }

    /// At the part at fault; the whole file when that is past its text.
    fn place(&self, problem: &GraphqlProblem) -> SourceSpan {
        let inside = self.1.get(problem.at.clone());
        inside.map_or(self.0.span, |_| graphql::file_span(self.0.span.file, self.1, &problem.at))
    }

    fn help(&self) -> &'static str {
        "save the whole answer to an introspection query, `{\"data\": {\"__schema\": …}}`, or write the schema as SDL"
    }
}

/// The schema file the api file `source` names, from the project root; `None` when it names
/// none, or not as a literal `$file` of a schema file.
pub fn path(source: &Source<'_>) -> Option<String> {
    resolve_path(&link::folder(source.path), graphql::literal_file(node(source)?, is_schema)?)
}

/// V2302 when the `graphql.schema` of `source` is not a literal `$file` of a schema file.
pub fn check(source: &Source<'_>) -> Option<Diagnostic> {
    let node = node(source).filter(|node| graphql::literal_file(node, is_schema).is_none())?;
    let message = "`graphql.schema` takes the literal `$file` path of a `.graphql`, `.gql`, `.graphqls` or `.json` file, with no `${…}` or `#` in it";
    let diagnostic = Diagnostic::new(Code::V2302, node.span, message).label("not a schema file");
    Some(diagnostic.help("name the schema file as written, such as `schema: {$file: schema.graphql}`"))
}

/// V1015 for each schema file the api files name that is not a schema: each file once, however
/// many name it; a file the compile was not given is left alone.
pub fn problems(documents: &Documents<'_>) -> Vec<Diagnostic> {
    let paths = documents.schema_paths().iter();
    paths.filter_map(|path| documents.schema(path)?.err()).cloned().collect()
}

/// The per-file stage of GraphQL text file `id`, which parses as `document`: the schema it holds,
/// or why it is not one, when it defines types. A file of only operations and fragments is read
/// as a schema only when an api file names it.
pub fn from_text(id: SourceFileId, source: &SourceFile, document: Option<&GraphqlDocument>) -> Option<Loaded> {
    document.filter(|document| document.defines_types)?;
    Some(Sdl(id, &source.text).read())
}

/// The per-file stage of a data file of `text`: the schema it holds, or why it is not one, when
/// it holds a `__schema`.
pub fn from_tree(root: &Node, text: &str) -> Option<Loaded> {
    root.get("data").unwrap_or(root).get("__schema")?;
    Some(Introspection(root, text).read())
}

/// The schema in `file`, as its per-file stage `cached` it, else read now; V1015 where it is not one.
pub fn read<'a>(file: &dyn SchemaFile, cached: Option<&'a Loaded>) -> Result<Cow<'a, GraphqlSchema>, Box<Diagnostic>> {
    match cached {
        Some(Ok(schema)) => Ok(Cow::Borrowed(schema)),
        Some(Err(problem)) => Err(not_a_schema(file, problem)),
        None => file.read().map(Cow::Owned).map_err(|problem| not_a_schema(file, &problem)),
    }
}

/// Whether `path` names a schema file.
pub fn is_schema(path: &str) -> bool {
    SUFFIXES.iter().any(|suffix| path.ends_with(suffix))
}

/// V1015 at `problem`, which keeps `file` from being a schema.
fn not_a_schema(file: &dyn SchemaFile, problem: &GraphqlProblem) -> Box<Diagnostic> {
    let span = file.place(problem);
    let message = format!("the `graphql.schema` file is not a GraphQL schema: {}", problem.message);
    Box::new(Diagnostic::new(Code::V1015, span, message).label("not part of a schema").help(file.help()))
}

/// The `graphql.schema` of `source`, an api file.
fn node<'n>(source: &Source<'n>) -> Option<&'n Node> {
    let graphql = source.root.get("graphql").filter(|_| source.kind == FileKind::Api)?;
    graphql.get(SCHEMA)
}

#[cfg(test)]
#[path = "graphql_schema_tests.rs"]
mod tests;
