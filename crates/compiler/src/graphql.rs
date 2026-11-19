//! A `graphql` api's document (D343, D346, D356): its `body.query`, inline or a `$file` the engine
//! reads for the compile (D132). A document file is parsed at its per-file stage ([`read_file`]),
//! which the compile cache keeps; [`Documents`] holds those and parses each inline document once.
//! V1014 is reported at the spot where a document does not parse, in the document file or in the
//! YAML text of an inline one.

use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::Range;

use indexmap::IndexSet;
use vc_diagnostics::{Code, Diagnostic, Diagnostics};
use vc_model::{FILE_KEY, FileKind, Protocol, SourceFile, SourceFileId, SourceFiles, SourceSpan, resolve_path};
use vc_parser::{GraphqlDocument, GraphqlOperation, GraphqlProblem, GraphqlSchema, Node, ScalarStyle, Value};

use crate::cache::CacheEntry;
use crate::compile::numbered;
use crate::graphql_schema::{self, Read, SchemaFile};
use crate::link::{self, Project, Source};
use crate::stage::position;

/// The `body` key that holds the document.
pub const QUERY: &str = "query";

/// The `body` key that holds the variables, the only part of the body that is a template.
pub const VARIABLES: &str = "variables";

/// The `body` key that names the operation to run.
pub const OPERATION_NAME: &str = "operationName";

/// The suffixes of a document file.
const SUFFIXES: [&str; 2] = [".graphql", ".gql"];

/// Whether `path` names a document file.
pub fn is_document(path: &str) -> bool {
    SUFFIXES.iter().any(|suffix| path.ends_with(suffix))
}

/// Whether `path` names a file of GraphQL text: a document file, or a schema's SDL file.
pub fn is_text(path: &str) -> bool {
    is_document(path) || path.ends_with(graphql_schema::SDL_SUFFIX)
}

/// Where a document is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Origin {
    /// A document file the compile was given.
    File(SourceFileId),
    /// An inline `query`, by its span.
    Inline(SourceSpan),
}

/// Each document of a compile, parsed once: every document file the compile was given, as its
/// cached per-file stage read it, and the inline `query` of every `graphql` api; and each schema
/// file its api files name, by path, as its per-file stage read it.
pub struct Documents<'a> {
    files: &'a SourceFiles,
    paths: HashMap<&'a str, SourceFileId>,
    given: HashMap<SourceFileId, &'a GraphqlDocument>,
    inline: HashMap<SourceSpan, Option<GraphqlDocument>>,
    schemas: HashMap<String, Schema<'a>>,
    problems: Diagnostics,
}

/// A schema file's schema, else its V1015: borrowed from the unit that cached it, or read now.
type Schema<'a> = Result<Cow<'a, GraphqlSchema>, Box<Diagnostic>>;

/// A document that parses, and where it is written.
pub struct Written<'d> {
    /// Where it is written.
    pub origin: Origin,
    /// Its text.
    pub text: &'d str,
    /// What it holds.
    pub parsed: &'d GraphqlDocument,
    files: &'d SourceFiles,
}

/// Why an api's `operationName` picks no operation of its document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unpicked {
    /// The document holds no operation.
    Empty,
    /// It holds several, and no `operationName` names one.
    Several,
    /// No operation has the name.
    Unknown,
    /// More than one operation has the name.
    Twice,
}

impl<'a> Documents<'a> {
    /// The documents of `project`: its document files as their units hold them, and its inline
    /// ones; and its schema files, with the data files `trees` the compile was given, by path.
    pub fn new(
        project: &Project<'a>,
        (entries, files): (&'a [CacheEntry], &'a SourceFiles),
        trees: &HashMap<&'a str, &'a Node>,
    ) -> Self {
        let mut paths = HashMap::new();
        for (id, file) in numbered(files).filter(|(_, file)| is_text(file.path.as_str())) {
            paths.entry(file.path.as_str()).or_insert(id);
        }
        let units = entries.iter().filter_map(|entry| Some((entry.file, entry.unit.as_deref()?.document.as_ref()?)));
        let given = units.collect();
        let (inline, schemas, problems) = (HashMap::new(), HashMap::new(), Diagnostics::new());
        let mut documents = Self { files, paths, given, inline, schemas, problems };
        let sources = project.sources.iter();
        let inline = sources.flat_map(|source| queries(source).into_iter().map(move |(_, _, query)| (source, query)));
        for (source, query) in inline {
            let Some(text) = query.as_str() else { continue };
            let parsed = vc_parser::parse_document(text);
            if let Err(problem) = parsed.as_ref().map_err(|problems| problems.first()) {
                let at = problem.map_or(0..0, |problem| problem.at.clone());
                let span = exact_span(files, (source, query), &at).unwrap_or(query.span);
                documents.problems.extend(problem.map(|problem| broken(span, problem)));
            }
            documents.inline.insert(query.span, parsed.ok());
        }
        documents.schemas = documents.read_schemas(project, (entries, trees));
        documents
    }

    /// V1014 for each inline document that does not parse.
    pub const fn problems(&self) -> &Diagnostics {
        &self.problems
    }

    /// The text of the document file at `path`, when the compile was given it.
    pub fn text(&self, path: &str) -> Option<&'a str> {
        let id = self.paths.get(path)?;
        self.files.get(*id).map(|file| file.text.as_str())
    }

    /// The schema in the file at `path`, which an api file names, else V1015; `None` when the
    /// compile was not given it, or it is SDL that does not parse, which is V1014 at its per-file stage.
    pub fn schema(&self, path: &str) -> Option<Result<&GraphqlSchema, &Diagnostic>> {
        Some(self.schemas.get(path)?.as_ref().map(|schema| &**schema).map_err(|diagnostic| &**diagnostic))
    }

    /// Each schema file the api files of `project` name, as its unit among `entries` read it.
    fn read_schemas(
        &self,
        project: &Project<'_>,
        (entries, trees): (&'a [CacheEntry], &HashMap<&'a str, &'a Node>),
    ) -> HashMap<String, Schema<'a>> {
        let unit = |entry: &'a CacheEntry| Some((self.files.get(entry.file)?.path.as_str(), entry.unit.as_deref()?));
        let cached: HashMap<&str, &'a Read> =
            entries.iter().filter_map(unit).filter_map(|(path, unit)| Some((path, unit.schema.as_ref()?))).collect();
        let paths: IndexSet<String> = project.sources.iter().filter_map(graphql_schema::path).collect();
        let read = |path: String| {
            let file = self.schema_file(&path, trees)?;
            let read = graphql_schema::read(&file, cached.get(path.as_str()).copied(), self.files);
            Some((path, read))
        };
        paths.into_iter().filter_map(read).collect()
    }

    /// The schema file at `path`: a data file among `trees`, else SDL that parses.
    fn schema_file(&self, path: &str, trees: &HashMap<&'a str, &'a Node>) -> Option<SchemaFile<'a>> {
        match (trees.get(path), self.paths.get(path)) {
            (Some(tree), _) => Some(SchemaFile::Introspection(tree)),
            (None, Some(id)) if self.given.contains_key(id) => {
                Some(SchemaFile::Sdl(*id, self.files.get(*id)?.text.as_str()))
            }
            (None, _) => None,
        }
    }

    /// The document the `query` of `source` holds, when it parses.
    pub fn get<'d>(&'d self, source: &Source<'_>, query: &'d Node) -> Option<Written<'d>> {
        let origin = self.origin(source, query)?;
        let (text, parsed) = match origin {
            Origin::Inline(span) => (query.as_str()?, self.inline.get(&span)?.as_ref()?),
            Origin::File(id) => (self.files.get(id)?.text.as_str(), *self.given.get(&id)?),
        };
        Some(Written { origin, text, parsed, files: self.files })
    }

    /// Where the `query` of `source` is written: inline, or in a document file the compile was given.
    fn origin(&self, source: &Source<'_>, query: &Node) -> Option<Origin> {
        if query.as_str().is_some() {
            return Some(Origin::Inline(query.span));
        }
        self.paths.get(file(source, query)?.as_str()).copied().map(Origin::File)
    }
}

impl Written<'_> {
    /// Where its bytes `at` are, exactly: in its file, or in the YAML of the inline `query` of
    /// `source` when they can be placed there.
    pub fn spot(&self, (source, query): (&Source<'_>, &Node), at: &Range<usize>) -> Option<SourceSpan> {
        match self.origin {
            Origin::Inline(_) => exact_span(self.files, (source, query), at),
            Origin::File(id) => self.files.get(id).map(|file| file_span(id, &file.text, at)),
        }
    }

    /// The operation `name` picks, else the document's only one (D346).
    ///
    /// # Errors
    ///
    /// Why it picks none.
    pub fn pick(&self, name: Option<&str>) -> Result<&GraphqlOperation, Unpicked> {
        let operations = &self.parsed.operations;
        if operations.is_empty() {
            return Err(Unpicked::Empty);
        }
        let Some(name) = name else {
            return match operations.as_slice() {
                [one] => Ok(one),
                _ => Err(Unpicked::Several),
            };
        };
        let mut named = operations.iter().filter(|operation| operation.name.as_deref() == Some(name));
        let first = named.next().ok_or(Unpicked::Unknown)?;
        named.next().map_or(Ok(first), |_| Err(Unpicked::Twice))
    }
}

/// The api, its `body` and the body's `query` of each `graphql` api in `source`.
pub fn queries<'n>(source: &Source<'n>) -> Vec<(&'n Node, &'n Node, &'n Node)> {
    let apis = link::list(source.root, "apis").iter().filter(|_| source.kind == FileKind::Api);
    let graphql = apis.filter(|api| link::protocol(source.root, api) == Protocol::Graphql);
    graphql.filter_map(|api| api.get("body").and_then(|body| Some((api, body, body.get(QUERY)?)))).collect()
}

/// The document file a `query: {$file: …}` names, from the project root; `None` for an inline
/// document, a templated path or another suffix.
pub fn file(source: &Source<'_>, query: &Node) -> Option<String> {
    resolve_path(&link::folder(source.path), literal_file(query, is_document)?)
}

/// The path `node` names when it is exactly `{$file: <path>}` and the path is literal, with a
/// suffix `suffix` takes ([`literal_path`]).
pub fn literal_file(node: &Node, suffix: fn(&str) -> bool) -> Option<&str> {
    let [only] = node.entries() else { return None };
    literal_path(&only.value).filter(|path| only.key == FILE_KEY && suffix(path))
}

/// The text of the `$file` value `value` when it is a literal path: no template, and no `#`
/// part, as a dataset's `$file` takes none.
pub fn literal_path(value: &Node) -> Option<&str> {
    value.as_str().filter(|path| !path.contains("${") && !path.contains('#'))
}

/// The per-file stage of a document file: what it holds, else V1014 where it first does not parse.
pub fn read_file(id: SourceFileId, source: &SourceFile) -> (Option<GraphqlDocument>, Diagnostics) {
    match vc_parser::parse_document(&source.text) {
        Ok(document) => (Some(document), Diagnostics::new()),
        Err(problems) => {
            let (span, first) = first_spot(id, &source.text, &problems);
            (None, first.map(|problem| broken(span, problem)).into_iter().collect())
        }
    }
}

/// V1014 for `problem`, at `span`.
fn broken(span: SourceSpan, problem: &GraphqlProblem) -> Diagnostic {
    let message = format!("the GraphQL document does not parse: {}", problem.message);
    let diagnostic = Diagnostic::new(Code::V1014, span, message).label("the parser stopped here");
    diagnostic.help("fix the document's syntax here; check the braces, parentheses and quotes before this spot")
}

/// The first of `problems` in the file `id` of `text`, and where it is: at the file's start when
/// there is none.
pub fn first_spot<'p>(
    id: SourceFileId,
    text: &str,
    problems: &'p [GraphqlProblem],
) -> (SourceSpan, Option<&'p GraphqlProblem>) {
    let first = problems.first();
    (file_span(id, text, &first.map_or(0..0, |problem| problem.at.clone())), first)
}

/// Where bytes `at` of the document file `id`, of `text`, are; an offset past the text, or inside
/// a character, is moved back to the nearest character boundary.
fn file_span(id: SourceFileId, text: &str, at: &Range<usize>) -> SourceSpan {
    SourceSpan::new(id, position(text, at.start), position(text, at.end))
}

/// Where bytes `at` of an inline document are in the YAML, when they can be placed exactly: for a
/// one-line scalar written without escapes and for a literal block, not for a folded one.
fn exact_span(files: &SourceFiles, (source, query): (&Source<'_>, &Node), at: &Range<usize>) -> Option<SourceSpan> {
    let Value::Scalar(scalar) = &query.value else { return None };
    let offset = |at: usize| match scalar.style {
        ScalarStyle::Literal => block_offset((source.text, query), &scalar.text, at),
        ScalarStyle::Folded => None,
        _ => line_offset((source.text, query), &scalar.text, at),
    };
    let (from, to) = (offset(at.start)?, offset(at.end)?);
    let end = source.text.get(..query.span.end.offset).map_or(query.span.end.offset, |before| before.trim_end().len());
    let place = |offset: usize| files.position(query.span.file, offset.min(end));
    Some(SourceSpan::new(query.span.file, place(from)?, place(to.max(from))?))
}

/// The YAML offset of document offset `at` in a one-line plain or quoted scalar with no escapes.
fn line_offset((yaml, query): (&str, &Node), text: &str, at: usize) -> Option<usize> {
    let written = yaml.get(query.span.start.offset..query.span.end.offset)?;
    let start = query.span.start.offset + written.len().checked_sub(text.len())? / 2;
    let exact = written.len() <= text.len() + 2 && yaml.get(start..start + text.len()) == Some(text);
    exact.then_some(start + at)
}

/// The YAML offset of document offset `at` in a literal block, whose span starts at its first
/// line's text: the same line of the block, past the same indent.
fn block_offset((yaml, query): (&str, &Node), text: &str, at: usize) -> Option<usize> {
    let first = yaml.get(..query.span.start.offset)?.rfind('\n').map_or(0, |newline| newline + 1);
    let indent = query.span.start.offset - first;
    let before = text.get(..at).filter(|_| !text.starts_with('\n'))?;
    let line = before.matches('\n').count();
    let column = before.len() - before.rfind('\n').map_or(0, |newline| newline + 1);
    let start = if line == 0 { 0 } else { yaml.get(first..)?.match_indices('\n').nth(line - 1)?.0 + 1 };
    Some(first + start + indent + column)
}

#[cfg(test)]
#[path = "graphql_tests.rs"]
mod tests;
