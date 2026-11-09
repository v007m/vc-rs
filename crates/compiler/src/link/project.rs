//! Every file the linker sees, with its file ID, its tree and its pieces in source order.

use std::collections::{HashMap, HashSet};
use std::ops::Range;

use vc_model::{FileKind, SourceFile, SourceFileId, SourceSpan, StringTable};
use vc_parser::Node;

use crate::parts::Piece;
use crate::stage::FileUnit;

/// The lifecycle blocks of a test file, in run order.
pub const FILE_BLOCKS: [&str; 4] = ["beforeAll", "beforeEach", "afterEach", "afterAll"];
/// The lifecycle blocks of the config.
pub const RUN_BLOCKS: [&str; 2] = ["beforeRun", "afterRun"];
/// The action lists of a test, in run order around its `call` or `steps`.
pub const TEST_BLOCKS: [&str; 3] = ["before", "steps", "after"];

/// One file with a tree.
pub struct Source<'a> {
    /// Its number.
    pub file: SourceFileId,
    /// Its project path.
    pub path: &'a str,
    /// Its text, for exact spans.
    pub text: &'a str,
    /// Its kind.
    pub kind: FileKind,
    /// Its tree.
    pub root: &'a Node,
    /// The file ID derived from its path; `None` for the config (D139).
    pub id: Option<String>,
    /// Whether the per-file stages found an error in it.
    pub broken: bool,
    pieces: Vec<(usize, usize, &'a Piece)>,
}

impl<'a> Source<'a> {
    /// The source for a unit with a tree.
    pub fn new(file: SourceFileId, source: &'a SourceFile, unit: &'a FileUnit) -> Option<Self> {
        let (root, kind) = (unit.root.as_ref()?, unit.kind?);
        let mut pieces: Vec<_> =
            unit.parts.iter().map(|(span, piece)| (span.start.offset, span.end.offset, piece)).collect();
        pieces.sort_by_key(|(start, end, _)| (*start, *end));
        let id = (kind != FileKind::Config).then(|| file_id(source.path.as_str(), kind));
        let broken = unit.diagnostics.has_errors();
        Some(Self { file, path: source.path.as_str(), text: &source.text, kind, root, id, broken, pieces })
    }

    /// The pieces inside `span`, minus those inside any of `holes`.
    pub fn pieces<'s>(&'s self, span: SourceSpan, holes: &'s [SourceSpan]) -> impl Iterator<Item = &'a Piece> + 's {
        let range = span.byte_range();
        let first = self.pieces.partition_point(|(start, ..)| *start < range.start);
        let outside = move |start: usize| !holes.iter().any(|hole| hole.byte_range().contains(&start));
        self.pieces[first..]
            .iter()
            .take_while(move |(start, ..)| *start < range.end)
            .filter(move |(start, end, _)| *end <= range.end && outside(*start))
            .map(|(.., piece)| *piece)
    }

    /// The piece compiled from `node`, if any.
    pub fn piece(&self, node: &Node) -> Option<&'a Piece> {
        let start = node.span.start.offset;
        let first = self.pieces.partition_point(|(at, ..)| *at < start);
        let mut found = self.pieces[first..].iter().take_while(|(at, ..)| *at == start);
        found.find(|(_, end, _)| *end == node.span.end.offset).map(|(.., piece)| *piece)
    }

    /// The full ID of the item `id` written in this file: `users#create`.
    pub fn full(&self, id: &str) -> String {
        format!("{}#{id}", self.id.as_deref().unwrap_or_default())
    }

    /// The full ID a reference written in this file names: a bare ID means this file's item.
    pub fn resolve(&self, written: &str) -> String {
        if written.contains('#') { written.to_owned() } else { self.full(written) }
    }

    /// The bytes of the text `word` starting at byte `offset`, if it is there.
    pub fn word_at(&self, offset: usize, word: &str) -> Option<Range<usize>> {
        let range = offset..offset + word.len();
        (self.text.get(range.clone())? == word).then_some(range)
    }
}

/// A file's ID from its path (D139, D142; see [`FileKind::file_id`]).
pub fn file_id(path: &str, kind: FileKind) -> String {
    kind.file_id(path)
}

/// An item that other files reference: an api or a test.
#[derive(Clone, Copy)]
pub struct Item<'a> {
    /// Index of its file in [`Project::sources`].
    pub source: usize,
    /// Its node.
    pub node: &'a Node,
}

/// Every file at once.
pub struct Project<'a> {
    /// The files with a tree, in path order.
    pub sources: Vec<Source<'a>>,
    /// The symbols the pieces point into.
    pub strings: &'a StringTable,
    /// Every api, by full ID.
    pub apis: HashMap<String, Item<'a>>,
    /// Every test, by full ID.
    pub tests: HashMap<String, Item<'a>>,
    /// The IDs of files the per-file stages could not read cleanly: references into them are
    /// not reported, since their items may be missing from the tables.
    pub unreadable: HashSet<String>,
}

impl<'a> Project<'a> {
    /// Indexes `sources`; the first of two items with one full ID wins (the second is V2001).
    pub fn new(sources: Vec<Source<'a>>, strings: &'a StringTable, unreadable: HashSet<String>) -> Self {
        let mut project = Self { sources, strings, apis: HashMap::new(), tests: HashMap::new(), unreadable };
        for (index, source) in project.sources.iter().enumerate() {
            let table = match source.kind {
                FileKind::Api => (&mut project.apis, "apis"),
                FileKind::Test => (&mut project.tests, "tests"),
                FileKind::Config | FileKind::Load => continue,
            };
            for node in source.root.get(table.1).map(Node::items).unwrap_or_default() {
                let Some(id) = node.get("id").and_then(Node::as_str) else { continue };
                table.0.entry(source.full(id)).or_insert(Item { source: index, node });
            }
        }
        project
    }

    /// The config, if the project has one.
    pub fn config(&self) -> Option<&Source<'a>> {
        self.sources.iter().find(|source| source.kind == FileKind::Config)
    }

    /// The source of `item`.
    pub fn source(&self, item: Item<'a>) -> &Source<'a> {
        &self.sources[item.source]
    }
}

/// Every action a file declares: in its lifecycle blocks and in its tests' `before`, `steps`
/// and `after`, in file order.
pub fn actions<'n>(source: &Source<'n>) -> Vec<&'n Node> {
    let blocks: &[&str] = match source.kind {
        FileKind::Test => &FILE_BLOCKS,
        FileKind::Config => &RUN_BLOCKS,
        FileKind::Api | FileKind::Load => &[],
    };
    let root = source.root;
    let mut nodes: Vec<&Node> = blocks.iter().flat_map(|key| list(root, key)).collect();
    let tests = list(root, "tests").iter().filter(|_| source.kind == FileKind::Test);
    nodes.extend(tests.flat_map(|test| TEST_BLOCKS.iter().flat_map(|key| list(test, key))));
    nodes.sort_by_key(|node| node.span.start.offset);
    nodes
}

/// The text of `key` in a map node.
pub fn text<'n>(node: &'n Node, key: &str) -> Option<&'n str> {
    node.get(key).and_then(|value| value.as_scalar()).map(|scalar| scalar.text.as_str())
}

/// The items of the list `key` in a map node.
pub fn list<'n>(node: &'n Node, key: &str) -> &'n [Node] {
    node.get(key).map(Node::items).unwrap_or_default()
}

/// The spans of the given keys' values, as holes for [`Source::pieces`].
pub fn holes(node: &Node, keys: &[&str]) -> Vec<SourceSpan> {
    keys.iter().filter_map(|key| node.get(key)).map(|value| value.span).collect()
}
