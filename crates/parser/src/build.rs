//! The [`Node`] tree, built from scalars and collection starts and ends whatever the syntax.

use std::collections::HashMap;

use vc_diagnostics::{Diagnostic, Diagnostics};
use vc_model::{Pos, SourceFileId, SourceSpan};

use crate::{Entry, Node, Scalar, ScalarStyle, Value, problems};

/// The deepest nesting accepted; deeper input is refused rather than risking the stack.
const MAX_DEPTH: usize = 128;

/// Whether the reader goes on.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    /// Read on.
    Continue,
    /// Stop reading.
    Stop,
}

/// The kind of collection a reader opens.
#[derive(Clone, Copy)]
pub enum Shape {
    /// A list.
    Seq,
    /// A map.
    Map,
}

#[derive(Default)]
enum KeyState {
    #[default]
    Expect,
    Have(String, SourceSpan),
    Skip,
}

/// An open map: its entries so far, the key waiting for its value, and every key kept so far.
#[derive(Default)]
struct MapBody {
    entries: Vec<Entry>,
    key: KeyState,
    seen: HashMap<String, SourceSpan>,
}

enum Body {
    Seq(Vec<Node>),
    Map(MapBody),
}

/// An open collection. `last` is the span of the last child read from the source, kept or not.
struct Frame {
    start: SourceSpan,
    last: Option<SourceSpan>,
    body: Body,
}

/// Builds a [`Node`] tree from what a reader finds, collecting every problem it can see. In a
/// map, nodes alternate between key and value.
pub struct Builder {
    file: SourceFileId,
    stack: Vec<Frame>,
    root: Option<Node>,
    failed: bool,
    diagnostics: Diagnostics,
}

impl Builder {
    /// A builder for spans in `file`.
    pub fn new(file: SourceFileId) -> Self {
        Self { file, stack: Vec::new(), root: None, failed: false, diagnostics: Diagnostics::new() }
    }

    /// Records a problem the tree survives.
    pub fn report(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    /// Records the problem that stopped the reader; the tree is dropped.
    pub fn fail(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
        self.failed = true;
    }

    /// Takes a scalar read at `at`.
    pub fn scalar(&mut self, scalar: Scalar, at: SourceSpan) {
        self.attach(Node { value: Value::Scalar(scalar), span: at });
    }

    /// Opens a collection that starts at `at`; stops, dropping the tree, past the nesting limit.
    pub fn open(&mut self, shape: Shape, at: SourceSpan) -> Flow {
        if self.stack.len() >= MAX_DEPTH {
            self.fail(problems::too_deep(at, MAX_DEPTH));
            return Flow::Stop;
        }
        let body = match shape {
            Shape::Seq => Body::Seq(Vec::new()),
            Shape::Map => Body::Map(MapBody::default()),
        };
        self.stack.push(Frame { start: at, last: None, body });
        Flow::Continue
    }

    /// Closes the innermost collection; `end` is its closing bracket, or empty for a block one.
    pub fn close(&mut self, end: SourceSpan) {
        let Some(Frame { start, last, body }) = self.stack.pop() else {
            return;
        };
        let value = match body {
            Body::Seq(items) => Value::Seq(items),
            Body::Map(map) => Value::Map(map.entries),
        };
        self.attach(Node { span: collection_span(start, last, end), value });
    }

    /// The root (a null scalar for an empty file; `None` after a reader error or nesting deeper
    /// than the limit) and the diagnostics.
    pub fn finish(self) -> (Option<Node>, Diagnostics) {
        if self.failed {
            return (None, self.diagnostics);
        }
        let start = SourceSpan::new(self.file, Pos::START, Pos::START);
        let empty = || Node { value: Value::Scalar(Scalar::new("", ScalarStyle::Plain)), span: start };
        (Some(self.root.unwrap_or_else(empty)), self.diagnostics)
    }

    fn attach(&mut self, node: Node) {
        let Some(frame) = self.stack.last_mut() else {
            self.root = Some(node);
            return;
        };
        frame.last = Some(node.span);
        match &mut frame.body {
            Body::Seq(items) => items.push(node),
            Body::Map(map) => map.take(node, &mut self.diagnostics),
        }
    }
}

impl MapBody {
    /// Takes `node` as the next key or as the value of the waiting key.
    fn take(&mut self, node: Node, diagnostics: &mut Diagnostics) {
        match std::mem::take(&mut self.key) {
            KeyState::Expect => self.key = self.key_state(node, diagnostics),
            KeyState::Have(key, key_span) => self.entries.push(Entry { key, key_span, value: node }),
            KeyState::Skip => {}
        }
    }

    /// Accepts `node` as the next key unless it is not text or repeats an earlier key.
    fn key_state(&mut self, node: Node, diagnostics: &mut Diagnostics) -> KeyState {
        let Value::Scalar(scalar) = node.value else {
            diagnostics.push(problems::complex_key(node.span));
            return KeyState::Skip;
        };
        if let Some(first) = self.seen.get(&scalar.text) {
            diagnostics.push(problems::duplicate_key(node.span, *first, &scalar.text));
            return KeyState::Skip;
        }
        self.seen.insert(scalar.text.clone(), node.span);
        KeyState::Have(scalar.text, node.span)
    }
}

/// A flow collection ends at its closing bracket; a block one at the end of its last child.
fn collection_span(start: SourceSpan, last_child: Option<SourceSpan>, end: SourceSpan) -> SourceSpan {
    let end = if end.start == end.end { last_child.map_or(start.end, |last| last.end) } else { end.end };
    SourceSpan::new(start.file, start.start, end)
}
