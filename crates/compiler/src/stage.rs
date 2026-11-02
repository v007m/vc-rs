//! The per-file stages: parse → shape-check → compile parts. Their result, a [`FileUnit`], is
//! what the [`CompileCache`](crate::CompileCache) keeps for a file that has not changed; the link
//! and lint stages run over every unit on each compile.

use indexmap::IndexMap;
use vc_diagnostics::{Diagnostic, Diagnostics};
use vc_model::{FileKind, SourceFile, SourceFileId, SourceSpan, StringTable};
use vc_parser::{Node, ScalarKind, Syntax, Value};

use crate::parts::{self, Cx, Piece, Tools};
use crate::shape_check::{self, Walk};
use crate::{shapes, version};

/// One file after the per-file stages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileUnit {
    /// The file's kind, from its name.
    pub kind: Option<FileKind>,
    /// The parsed tree; `None` when the file does not parse or is of no known kind.
    pub root: Option<Node>,
    /// Each compiled value, keyed by the span of the node it was read from.
    pub parts: IndexMap<SourceSpan, Piece>,
    /// Every problem the per-file stages found.
    pub diagnostics: Diagnostics,
}

/// Runs the per-file stages over `source`, numbered `id`; names are interned into `strings`.
pub fn unit(id: SourceFileId, source: &SourceFile, (tools, strings): (&Tools, &mut StringTable)) -> FileUnit {
    let (root, mut diagnostics) = parse(id, source);
    let kind = source.kind();
    let (Some(root), Some(file_kind)) = (root.map(empty_as_map), kind) else {
        return FileUnit { kind, root: None, parts: IndexMap::new(), diagnostics };
    };
    let walk = shape(&root, file_kind);
    let steps = walk.steps.iter().filter_map(|step| strings.intern(step).ok()).collect();
    let mut cx = Cx { text: &source.text, strings, tools, steps, diagnostics: Diagnostics::new() };
    let parts = parts::compile(&root, &walk, &mut cx);
    let later = walk.diagnostics.into_iter().chain(cx.diagnostics);
    let fresh: Vec<Diagnostic> = later.filter(|later| !reported(&diagnostics, later)).collect();
    diagnostics.extend(fresh);
    FileUnit { kind, root: Some(root), parts, diagnostics }
}

/// The parse stage: the one place the compiler calls the parser.
pub fn parse(id: SourceFileId, source: &SourceFile) -> (Option<Node>, Diagnostics) {
    vc_parser::parse(id, Syntax::of(&source.path), &source.text)
}

/// Whether the parser already reported the spot `later` points at, as for an alias it read as
/// null; one problem gets one diagnostic.
fn reported(parsed: &Diagnostics, later: &Diagnostic) -> bool {
    parsed.iter().any(|diagnostic| diagnostic.primary.span == later.primary.span)
}

/// The shape-check stage, with the config's `syntaxVersion` check.
fn shape(root: &Node, kind: FileKind) -> Walk {
    let mut walk = shape_check::check(root, shapes::root(kind));
    if kind == FileKind::Config {
        walk.diagnostics.extend(version::check(root));
    }
    walk
}

/// An empty file reads as an empty map, so it is checked like one.
fn empty_as_map(root: Node) -> Node {
    match &root.value {
        Value::Scalar(scalar) if scalar.kind == ScalarKind::Null => Node { value: Value::Map(Vec::new()), ..root },
        _ => root,
    }
}

#[cfg(test)]
mod tests {
    use vc_diagnostics::Code;
    use vc_model::{ContentHash, SourceFile, SourceFileId, SourcePath, StringTable};

    use super::{FileUnit, unit};
    use crate::parts::Tools;

    fn run(path: &str, text: &str) -> FileUnit {
        let file =
            SourceFile { path: SourcePath::new(path).unwrap(), text: text.to_owned(), hash: ContentHash::new([0; 32]) };
        unit(SourceFileId::new(0), &file, (&Tools::new(), &mut StringTable::new()))
    }

    #[test]
    fn reports_a_spot_the_parser_reported_once() {
        let unit = run("services/users/users.api.yaml", "a: &x {}\nheaders: *x\n");
        assert_eq!(codes(&unit), [Code::V1007, Code::V1004]);
    }

    fn codes(unit: &FileUnit) -> Vec<Code> {
        unit.diagnostics.iter().map(|d| d.code).collect()
    }

    #[test]
    fn runs_every_stage_in_order() {
        let unit = run("vibranium.config.yaml", "syntaxVersion: '2'\nprojct: x\n");
        assert_eq!(codes(&unit), [Code::V2302, Code::V1004]);
        let unit = run("services/users/users.api.yaml", "baseUrl: ${env.url\n");
        assert_eq!(codes(&unit), [Code::V2305]);
        assert!(unit.root.is_some() && unit.parts.is_empty());
    }

    #[test]
    fn stops_after_a_parse_error_and_skips_files_of_no_kind() {
        let broken = run("services/users/users.api.yaml", "a: [\n");
        assert!(broken.root.is_none() && codes(&broken) == [Code::V1001]);
        let other = run("notes.yaml", "anything: 1\n");
        assert!(other.root.is_none() && other.diagnostics.is_empty() && other.kind.is_none());
    }

    #[test]
    fn reads_an_empty_file_as_an_empty_map() {
        assert!(run("services/users/users.api.yaml", "").diagnostics.is_empty());
        assert_eq!(codes(&run("vibranium.config.yaml", "")), [Code::V1003]);
        let config = run("vibranium.config.yaml", "syntaxVersion: 3\n");
        assert_eq!(codes(&config), [Code::V1003]);
    }
}
