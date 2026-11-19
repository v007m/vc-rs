//! A `type: graphql` api file's schema (D347): its `graphql.schema` names an SDL file (`.graphql`,
//! `.gql`, `.graphqls`) or an introspection result (`.json`), which the engine reads for the
//! compile (D132). The per-file stage of a `.graphqls` file and of a data file with a `__schema`
//! reads it as a schema once, which the compile cache keeps ([`from_text`], [`from_tree`]); any
//! other file an api file names is read when it is named ([`read`]). An SDL file that does not
//! parse is V1014 at its per-file stage, and V1015 is a file that reads but is not a schema.

use std::borrow::Cow;

use vc_diagnostics::{Code, Diagnostic};
use vc_model::{FileKind, SourceFile, SourceFileId, SourceFiles, SourceSpan, resolve_path};
use vc_parser::{GraphqlDocument, GraphqlProblem, GraphqlSchema, Node};

use crate::graphql::{self, Documents};
use crate::link::{self, Source};

/// The `graphql` key that names the schema.
const SCHEMA: &str = "schema";

/// The suffix of an SDL file that holds only a schema.
pub const SDL_SUFFIX: &str = ".graphqls";

/// The suffixes of a schema file.
const SUFFIXES: [&str; 4] = [".graphql", ".gql", SDL_SUFFIX, ".json"];

/// What a file holds as a schema, else where it is not one.
pub type Read = Result<GraphqlSchema, Vec<GraphqlProblem>>;

/// A schema file the compile was given, by how its kind is read and reported.
pub trait SchemaFile {
    /// What the file holds as a schema.
    fn read(&self) -> Read;

    /// Where the first of `problems` is in the file, and that problem.
    fn place<'p>(
        &self,
        problems: &'p [GraphqlProblem],
        files: &SourceFiles,
    ) -> (SourceSpan, Option<&'p GraphqlProblem>);

    /// How to fix a file of its kind that is not a schema.
    fn help(&self) -> &'static str;
}

/// A schema file of SDL: the file, and its text.
pub struct Sdl<'a>(pub SourceFileId, pub &'a str);

/// A schema file that is an introspection result, as its JSON tree.
pub struct Introspection<'a>(pub &'a Node);

impl SchemaFile for Sdl<'_> {
    fn read(&self) -> Read {
        vc_parser::parse_schema(self.1)
    }

    fn place<'p>(&self, problems: &'p [GraphqlProblem], _: &SourceFiles) -> (SourceSpan, Option<&'p GraphqlProblem>) {
        graphql::first_spot(self.0, self.1, problems)
    }

    fn help(&self) -> &'static str {
        "fix the schema here; it holds only type definitions (`type`, `input`, `enum`, …), and operations go in an api's `query`"
    }
}

impl SchemaFile for Introspection<'_> {
    fn read(&self) -> Read {
        vc_parser::schema_from_introspection(self.0).map_err(|problem| vec![problem])
    }

    fn place<'p>(
        &self,
        problems: &'p [GraphqlProblem],
        files: &SourceFiles,
    ) -> (SourceSpan, Option<&'p GraphqlProblem>) {
        let first = problems.first();
        (json_span(self.0, first, files), first)
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

/// The per-file stage of a GraphQL text file: the schema a `.graphqls` file that parses as
/// `document` holds, when it is one. A `.graphql` or `.gql` file is read only when it is named.
pub fn from_text(source: &SourceFile, document: Option<&GraphqlDocument>) -> Option<GraphqlSchema> {
    document.filter(|_| source.path.as_str().ends_with(SDL_SUFFIX))?;
    vc_parser::parse_schema(&source.text).ok()
}

/// The per-file stage of a data file: the schema it holds, when it holds a `__schema` that is one.
pub fn from_tree(root: &Node) -> Option<GraphqlSchema> {
    root.get("data").unwrap_or(root).get("__schema")?;
    vc_parser::schema_from_introspection(root).ok()
}

/// The schema in `file`, as its per-file stage `cached` it, else read now; V1015 where it is not one.
pub fn read<'a>(
    file: &dyn SchemaFile,
    cached: Option<&'a GraphqlSchema>,
    files: &SourceFiles,
) -> Result<Cow<'a, GraphqlSchema>, Box<Diagnostic>> {
    let fresh = || file.read().map(Cow::Owned).map_err(|problems| not_a_schema(file, &problems, files));
    cached.map_or_else(fresh, |schema| Ok(Cow::Borrowed(schema)))
}

/// Whether `path` names a schema file.
fn is_schema(path: &str) -> bool {
    SUFFIXES.iter().any(|suffix| path.ends_with(suffix))
}

/// V1015 at the first of `problems`, which keep `file` from being a schema.
fn not_a_schema(file: &dyn SchemaFile, problems: &[GraphqlProblem], files: &SourceFiles) -> Box<Diagnostic> {
    let (span, first) = file.place(problems, files);
    let why = first.map_or("", |problem| problem.message.as_str());
    let message = format!("the `graphql.schema` file is not a GraphQL schema: {why}");
    Box::new(Diagnostic::new(Code::V1015, span, message).label("not part of a schema").help(file.help()))
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
