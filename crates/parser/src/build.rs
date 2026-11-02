use std::borrow::Cow;
use std::collections::HashMap;

use saphyr_parser::{Event, ScalarStyle as SaphyrStyle, ScanError, Span};
use vc_diagnostics::Diagnostics;
use vc_model::{Pos, SourceSpan};

use crate::positions::{self, Positions};
use crate::{Entry, Node, Scalar, ScalarKind, ScalarStyle, Value, cut, problems};

/// The deepest nesting accepted; deeper input is refused rather than risking the stack.
const MAX_DEPTH: usize = 128;

/// Whether the event loop goes on.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    /// Take the next event.
    Continue,
    /// Stop reading.
    Stop,
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

impl Frame {
    const fn new(start: SourceSpan, body: Body) -> Self {
        Self { start, last: None, body }
    }
}

/// Builds a [`Node`] tree from saphyr's events, collecting every problem it can see.
pub struct Builder<'t> {
    text: &'t str,
    positions: Positions,
    stack: Vec<Frame>,
    root: Option<Node>,
    documents: usize,
    failed: bool,
    diagnostics: Diagnostics,
}

impl<'t> Builder<'t> {
    /// A builder for `text`, with spans in `positions`' file.
    pub fn new(text: &'t str, positions: Positions) -> Self {
        let diagnostics = Diagnostics::new();
        Self { text, positions, stack: Vec::new(), root: None, documents: 0, failed: false, diagnostics }
    }

    /// Takes one event. Node spans leave out `&anchor` and `!tag`; tag diagnostics keep them.
    pub fn event(&mut self, event: Event<'_>, span: &Span) -> Flow {
        let at = self.positions.span(span);
        match event {
            Event::DocumentStart(_) => return self.document(at),
            Event::Scalar(text, style, _, tag) => {
                self.check_tag(tag.is_some(), at);
                self.scalar(text, style, positions::without_properties(self.text, at));
            }
            Event::Alias(_) => {
                self.diagnostics.push(problems::alias(at));
                self.attach(Node { value: Value::Scalar(Scalar::new("", ScalarStyle::Plain)), span: at });
            }
            Event::SequenceStart(_, tag) => {
                self.check_tag(tag.is_some(), at);
                return self.open(Frame::new(positions::without_properties(self.text, at), Body::Seq(Vec::new())));
            }
            Event::MappingStart(_, tag) => {
                self.check_tag(tag.is_some(), at);
                let body = Body::Map(MapBody::default());
                return self.open(Frame::new(positions::without_properties(self.text, at), body));
            }
            Event::SequenceEnd | Event::MappingEnd => self.close(at),
            _ => {}
        }
        Flow::Continue
    }

    /// Records the error that stopped the scanner; the tree is dropped.
    pub fn scan_error(&mut self, error: &ScanError) {
        self.diagnostics.push(problems::scan_error(self.positions.char_at(error.marker()), error.info()));
        self.failed = true;
    }

    /// The root (a null scalar for an empty file; `None` after a scanner error or nesting
    /// deeper than the limit) and the diagnostics.
    pub fn finish(self) -> (Option<Node>, Diagnostics) {
        if self.failed {
            return (None, self.diagnostics);
        }
        let start = SourceSpan::new(self.positions.file(), Pos::START, Pos::START);
        let empty = || Node { value: Value::Scalar(Scalar::new("", ScalarStyle::Plain)), span: start };
        (Some(self.root.unwrap_or_else(empty)), self.diagnostics)
    }

    fn document(&mut self, at: SourceSpan) -> Flow {
        self.documents += 1;
        if self.documents == 1 {
            return Flow::Continue;
        }
        self.diagnostics.push(problems::second_document(at));
        Flow::Stop
    }

    fn check_tag(&mut self, tagged: bool, at: SourceSpan) {
        if tagged {
            self.diagnostics.push(problems::tag(at));
        }
    }

    /// Only plain text is checked for a cut at ` #`: a typed value such as `3 #max` is left alone.
    fn scalar(&mut self, text: Cow<'_, str>, style: SaphyrStyle, at: SourceSpan) {
        let scalar = Scalar::new(text, style_of(style));
        if scalar.style == ScalarStyle::Plain && scalar.kind == ScalarKind::String {
            self.diagnostics.extend(cut::check(self.text, at));
        }
        self.attach(Node { value: Value::Scalar(scalar), span: at });
    }

    fn open(&mut self, frame: Frame) -> Flow {
        if self.stack.len() >= MAX_DEPTH {
            self.diagnostics.push(problems::too_deep(frame.start, MAX_DEPTH));
            self.failed = true;
            return Flow::Stop;
        }
        self.stack.push(frame);
        Flow::Continue
    }

    fn close(&mut self, end: SourceSpan) {
        let Some(Frame { start, last, body }) = self.stack.pop() else {
            return;
        };
        let value = match body {
            Body::Seq(items) => Value::Seq(items),
            Body::Map(map) => Value::Map(map.entries),
        };
        self.attach(Node { span: collection_span(start, last, end), value });
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

const fn style_of(style: SaphyrStyle) -> ScalarStyle {
    match style {
        SaphyrStyle::Plain => ScalarStyle::Plain,
        SaphyrStyle::SingleQuoted => ScalarStyle::SingleQuoted,
        SaphyrStyle::DoubleQuoted => ScalarStyle::DoubleQuoted,
        SaphyrStyle::Literal => ScalarStyle::Literal,
        SaphyrStyle::Folded => ScalarStyle::Folded,
    }
}
