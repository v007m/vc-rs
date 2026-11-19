//! A `graphql` api's document (D343, D346, D356): its `body.query`, inline or a `$file` the engine
//! reads for the compile (D132), and V1014 at the spot where it does not parse, in the document
//! file or in the YAML text of an inline one.

use std::ops::Range;

use vc_diagnostics::{Code, Diagnostic, Diagnostics};
use vc_model::{FILE_KEY, FileKind, Protocol, SourceFile, SourceFileId, SourceFiles, SourceSpan, resolve_path};
use vc_parser::{GraphqlProblem, Node, ScalarStyle, Value};

use crate::link::{self, Source};
use crate::script;

/// The suffixes of a document file.
const SUFFIXES: [&str; 2] = [".graphql", ".gql"];

/// Whether `path` names a document file.
pub fn is_document(path: &str) -> bool {
    SUFFIXES.iter().any(|suffix| path.ends_with(suffix))
}

/// Where a document is written: inline in its api, or in a document file the compile was given.
pub enum Written<'f> {
    /// The text of an inline `query`.
    Inline(&'f str),
    /// A document file and its text.
    File(SourceFileId, &'f str),
}

impl<'f> Written<'f> {
    /// The document `query` of `source` holds, inline or from a file in `files`.
    pub fn of(files: &'f SourceFiles, (source, query): (&Source<'_>, &'f Node)) -> Option<Self> {
        if let Some(text) = query.as_str() {
            return Some(Self::Inline(text));
        }
        file(source, query).and_then(|path| find(files, &path)).map(|(id, text)| Self::File(id, text))
    }

    /// Its text.
    pub const fn text(&self) -> &'f str {
        match self {
            Self::Inline(text) | Self::File(_, text) => text,
        }
    }

    /// Where its bytes `at` are, exactly: in its file, or in the YAML of the inline `query` of
    /// `source` when they can be placed there.
    pub fn spot(&self, (source, query): (&Source<'_>, &Node), at: &Range<usize>) -> Option<SourceSpan> {
        match self {
            Self::Inline(_) => exact_span(source, query, at),
            Self::File(id, text) => {
                Some(SourceSpan::new(*id, script::position(text, at.start), script::position(text, at.end)))
            }
        }
    }
}

/// The api, its `body` and the body's `query` of each `graphql` api in `source`.
pub fn queries<'n>(source: &Source<'n>) -> Vec<(&'n Node, &'n Node, &'n Node)> {
    let apis = link::list(source.root, "apis").iter().filter(|_| source.kind == FileKind::Api);
    let graphql = apis.filter(|api| link::protocol(source.root, api) == Protocol::Graphql);
    graphql.filter_map(|api| api.get("body").and_then(|body| Some((api, body, body.get("query")?)))).collect()
}

/// The document file a `query: {$file: …}` names, from the project root; `None` for an inline
/// document, a templated path or another suffix.
pub fn file(source: &Source<'_>, query: &Node) -> Option<String> {
    let [only] = query.entries() else { return None };
    let written = only.value.as_str().filter(|path| only.key == FILE_KEY && !path.contains("${"))?;
    resolve_path(&link::folder(source.path), written).filter(|path| is_document(path))
}

/// The text of the document file at `path`, when the compile was given it.
pub fn text<'f>(files: &'f SourceFiles, path: &str) -> Option<&'f str> {
    find(files, path).map(|(_, text)| text)
}

/// The number and text of the file at `path`, when the compile was given it.
fn find<'f>(files: &'f SourceFiles, path: &str) -> Option<(SourceFileId, &'f str)> {
    let given = (0..).map(SourceFileId::new).map_while(|id| Some((id, files.get(id)?)));
    given.into_iter().find(|(_, file)| file.path.as_str() == path).map(|(id, file)| (id, file.text.as_str()))
}

/// V1014 at the first spot the document file `file` does not parse.
pub fn file_problem(id: SourceFileId, file: &SourceFile) -> Diagnostics {
    let Some(problem) = first_problem(&file.text) else { return Diagnostics::new() };
    let at = |offset: usize| script::position(&file.text, offset);
    std::iter::once(broken(SourceSpan::new(id, at(problem.at.start), at(problem.at.end)), &problem)).collect()
}

/// V1014 at the first spot the inline `query` of `source` does not parse, in its YAML text.
pub fn inline_problem(source: &Source<'_>, query: &Node) -> Option<Diagnostic> {
    let problem = first_problem(query.as_str()?)?;
    Some(broken(inline_span(source, query, &problem.at), &problem))
}

/// The first spot `text` does not parse.
fn first_problem(text: &str) -> Option<GraphqlProblem> {
    vc_parser::parse_document(text).err()?.into_iter().next()
}

/// V1014 for `problem`, at `span`.
fn broken(span: SourceSpan, problem: &GraphqlProblem) -> Diagnostic {
    let message = format!("the GraphQL document does not parse: {}", problem.message);
    let diagnostic = Diagnostic::new(Code::V1014, span, message).label("the parser stopped here");
    diagnostic.help("fix the document's syntax here; check the braces, parentheses and quotes before this spot")
}

/// Where bytes `at` of an inline document are in the YAML: exact for a one-line scalar written
/// without escapes and for a literal block (see [`exact_span`]); else the whole value.
fn inline_span(source: &Source<'_>, query: &Node, at: &Range<usize>) -> SourceSpan {
    exact_span(source, query, at).unwrap_or(query.span)
}

/// Where bytes `at` of an inline document are in the YAML, when they can be placed exactly.
fn exact_span(source: &Source<'_>, query: &Node, at: &Range<usize>) -> Option<SourceSpan> {
    let Value::Scalar(scalar) = &query.value else { return None };
    let offset = |at: usize| match scalar.style {
        ScalarStyle::Literal => block_offset((source.text, query), &scalar.text, at),
        ScalarStyle::Folded => None,
        _ => line_offset((source.text, query), &scalar.text, at),
    };
    let (from, to) = (offset(at.start)?, offset(at.end)?);
    let end = source.text.get(..query.span.end.offset).map_or(query.span.end.offset, |before| before.trim_end().len());
    let place = |offset: usize| script::position(source.text, offset.min(end));
    Some(SourceSpan::new(query.span.file, place(from), place(to.max(from))))
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
