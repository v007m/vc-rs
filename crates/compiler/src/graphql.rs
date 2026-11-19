//! A `graphql` api's document (D343, D346, D356): its `body.query`, inline or a `$file` the engine
//! reads for the compile (D132). [`Documents`] parses each document of a compile once, and
//! reports V1014 at the spot where one does not parse, in the document file or in the YAML text
//! of an inline one.

use std::collections::HashMap;
use std::ops::Range;

use vc_diagnostics::{Code, Diagnostic, Diagnostics};
use vc_model::{FILE_KEY, FileKind, Pos, Protocol, SourceFileId, SourceFiles, SourceSpan, resolve_path};
use vc_parser::{GraphqlDocument, GraphqlOperation, GraphqlProblem, Node, ScalarStyle, Value};

use crate::compile::numbered;
use crate::link::{self, Project, Source};

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

/// Where a document is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Origin {
    /// A document file the compile was given.
    File(SourceFileId),
    /// An inline `query`, by its span.
    Inline(SourceSpan),
}

/// Each document of a compile, parsed once: every document file the compile was given, and the
/// inline `query` of every `graphql` api.
pub struct Documents<'a> {
    files: &'a SourceFiles,
    paths: HashMap<&'a str, SourceFileId>,
    parsed: HashMap<Origin, Option<GraphqlDocument>>,
    problems: Diagnostics,
}

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
    /// The documents of `project`, from the files the compile was given.
    pub fn new(project: &Project<'a>, files: &'a SourceFiles) -> Self {
        let given: Vec<_> = numbered(files).filter(|(_, file)| is_document(file.path.as_str())).collect();
        let paths = given.iter().map(|(id, file)| (file.path.as_str(), *id)).collect();
        let mut documents = Self { files, paths, parsed: HashMap::new(), problems: Diagnostics::new() };
        for (id, file) in given {
            let problem = documents.read(Origin::File(id), &file.text);
            documents.problems.extend(problem.map(|problem| broken(file_span(files, id, &problem.at), &problem)));
        }
        let sources = project.sources.iter();
        let inline = sources.flat_map(|source| queries(source).into_iter().map(move |(_, _, query)| (source, query)));
        for (source, query) in inline {
            let Some(text) = query.as_str() else { continue };
            let Some(problem) = documents.read(Origin::Inline(query.span), text) else { continue };
            let span = exact_span(files, (source, query), &problem.at).unwrap_or(query.span);
            documents.problems.push(broken(span, &problem));
        }
        documents
    }

    /// Parses the document `text` written at `origin`; gives where it first does not parse, and why.
    fn read(&mut self, origin: Origin, text: &str) -> Option<GraphqlProblem> {
        let parsed = vc_parser::parse_document(text);
        let problem = parsed.as_ref().err().and_then(|problems| problems.first().cloned());
        self.parsed.insert(origin, parsed.ok());
        problem
    }

    /// V1014 for each document that does not parse.
    pub const fn problems(&self) -> &Diagnostics {
        &self.problems
    }

    /// The text of the document file at `path`, when the compile was given it.
    pub fn text(&self, path: &str) -> Option<&'a str> {
        let id = self.paths.get(path)?;
        self.files.get(*id).map(|file| file.text.as_str())
    }

    /// The document the `query` of `source` holds, when it parses.
    pub fn get<'d>(&'d self, source: &Source<'_>, query: &'d Node) -> Option<Written<'d>> {
        let origin = self.origin(source, query)?;
        let text = match origin {
            Origin::Inline(_) => query.as_str()?,
            Origin::File(id) => self.files.get(id)?.text.as_str(),
        };
        let parsed = self.parsed.get(&origin)?.as_ref()?;
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
            Origin::File(id) => Some(file_span(self.files, id, at)),
        }
    }

    /// The operation `name` picks, else the document's only one (D346).
    ///
    /// # Errors
    ///
    /// Why it picks none.
    pub fn pick(&self, name: Option<&str>) -> Result<&GraphqlOperation, Unpicked> {
        let operations = &self.parsed.operations;
        let Some(name) = name else {
            return match operations.as_slice() {
                [] => Err(Unpicked::Empty),
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
    let [only] = query.entries() else { return None };
    let written = only.value.as_str().filter(|path| only.key == FILE_KEY && !path.contains("${"))?;
    resolve_path(&link::folder(source.path), written).filter(|path| is_document(path))
}

/// V1014 for `problem`, at `span`.
fn broken(span: SourceSpan, problem: &GraphqlProblem) -> Diagnostic {
    let message = format!("the GraphQL document does not parse: {}", problem.message);
    let diagnostic = Diagnostic::new(Code::V1014, span, message).label("the parser stopped here");
    diagnostic.help("fix the document's syntax here; check the braces, parentheses and quotes before this spot")
}

/// Where bytes `at` of the document file `id` are.
fn file_span(files: &SourceFiles, id: SourceFileId, at: &Range<usize>) -> SourceSpan {
    let place = |offset: usize| files.position(id, offset).unwrap_or_else(|| Pos::new(1, 1, 0));
    SourceSpan::new(id, place(at.start), place(at.end))
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
