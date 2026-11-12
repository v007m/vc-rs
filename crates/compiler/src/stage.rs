//! The per-file stages: parse → shape-check → compile parts. Their result, a [`FileUnit`], is
//! what the [`CompileCache`](crate::CompileCache) keeps for a file that has not changed; the link
//! and lint stages run over every unit on each compile.

use indexmap::IndexMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use vc_diagnostics::{Diagnostic, Diagnostics};
use vc_model::{FileKind, SourceFile, SourceFileId, SourceSpan, StringTable};
use vc_parser::{Node, ScalarKind, Syntax, Value};

use crate::parts::{self, Cx, Piece, Tools};
use crate::script::{self, Script};
use crate::shape_check::{self, Walk};
use crate::{shapes, version};

/// One file after the per-file stages.
///
/// The compile cache stores it without its diagnostics, so it stores only a unit that has none.
/// The pieces are written as a list of span and piece pairs, since JSON keys are text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileUnit {
    /// The file's kind, from its name.
    pub kind: Option<FileKind>,
    /// The parsed tree; `None` when the file does not parse. A file of no kind (a dataset) only parses.
    pub root: Option<Node>,
    /// Each compiled value, keyed by the span of the node it was read from.
    #[serde(serialize_with = "pairs_out", deserialize_with = "pairs_in")]
    pub parts: IndexMap<SourceSpan, Piece>,
    /// A hook file, stripped (D172); `None` for any other file.
    pub script: Option<Script>,
    /// Every problem the per-file stages found.
    #[serde(skip)]
    pub diagnostics: Diagnostics,
}

/// Writes the pieces as a list of `[span, piece]` pairs.
fn pairs_out<S: Serializer>(parts: &IndexMap<SourceSpan, Piece>, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_seq(parts)
}

/// Reads the pieces back from a list of `[span, piece]` pairs.
fn pairs_in<'de, D: Deserializer<'de>>(deserializer: D) -> Result<IndexMap<SourceSpan, Piece>, D::Error> {
    Vec::<(SourceSpan, Piece)>::deserialize(deserializer).map(|pairs| pairs.into_iter().collect())
}

/// Runs the per-file stages over `source`, numbered `id`; names are interned into `strings`.
pub fn unit(id: SourceFileId, source: &SourceFile, (tools, strings): (&Tools, &mut StringTable)) -> FileUnit {
    if script::is_script(source.path.as_str()) {
        let (script, diagnostics) = script::strip(id, source);
        return FileUnit { kind: None, root: None, parts: IndexMap::new(), script, diagnostics };
    }
    let (root, mut diagnostics) = parse(id, source);
    let kind = source.kind();
    let Some(file_kind) = kind else {
        return FileUnit { kind, root, parts: IndexMap::new(), script: None, diagnostics };
    };
    let Some(root) = root.map(empty_as_map) else {
        return FileUnit { kind, root: None, parts: IndexMap::new(), script: None, diagnostics };
    };
    let walk = shape(&root, file_kind);
    let steps = walk.steps.iter().filter_map(|step| strings.intern(step).ok()).collect();
    let mut cx = Cx { text: &source.text, strings, tools, steps, diagnostics: Diagnostics::new() };
    let parts = parts::compile(&root, &walk, &mut cx);
    let later = walk.diagnostics.into_iter().chain(cx.diagnostics);
    let fresh: Vec<Diagnostic> = later.filter(|later| !reported(&diagnostics, later)).collect();
    diagnostics.extend(fresh);
    FileUnit { kind, root: Some(root), parts, script: None, diagnostics }
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
    fn stops_after_a_parse_error_and_only_parses_files_of_no_kind() {
        let broken = run("services/users/users.api.yaml", "a: [\n");
        assert!(broken.root.is_none() && codes(&broken) == [Code::V1001]);
        let other = run("datasets/notes.yaml", "- anything: 1\n");
        assert!(other.root.is_some() && other.diagnostics.is_empty() && other.kind.is_none() && other.parts.is_empty());
        let rows = run("datasets/cities.csv", "city\nUlm\n");
        assert!(rows.root.is_some_and(|root| root.items().len() == 1) && rows.diagnostics.is_empty());
    }

    /// The exports and V1013 come from oxc, the `hooks` feature.
    #[test]
    #[cfg(feature = "hooks")]
    fn strips_a_hook_file_instead_of_parsing_it() {
        let hook = run("hooks/sign.ts", "export const sign = (x: number): number => x;\n");
        assert!(hook.root.is_none() && hook.kind.is_none() && hook.diagnostics.is_empty());
        assert_eq!(hook.script.map(|script| script.exports), Some(vec!["sign".to_owned()]));
        assert_eq!(codes(&run("hooks/bad.js", "export const = 1;\n")), [Code::V1013]);
        assert!(run("datasets/notes.yaml", "- a\n").script.is_none());
    }

    #[test]
    fn reads_an_empty_file_as_an_empty_map() {
        assert!(run("services/users/users.api.yaml", "").diagnostics.is_empty());
        assert_eq!(codes(&run("vibranium.config.yaml", "")), [Code::V1003]);
        let config = run("vibranium.config.yaml", "syntaxVersion: 3\n");
        assert_eq!(codes(&config), [Code::V1003]);
        assert_eq!(codes(&run("services/users/users.api.yaml", "hello\n")), [Code::V2302]);
    }
}
