use std::borrow::Cow;

use saphyr_parser::{Event, ScalarStyle as SaphyrStyle, ScanError, Span};
use vc_diagnostics::Diagnostics;
use vc_model::{Pos, SourceSpan};

use crate::positions::Positions;
use crate::{Entry, Node, Scalar, ScalarStyle, Value, cut, problems};

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

enum KeyState {
    Expect,
    Have(String, SourceSpan),
    Skip,
}

enum Frame {
    Seq { start: SourceSpan, items: Vec<Node> },
    Map { start: SourceSpan, entries: Vec<Entry>, key: KeyState },
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

    /// Takes one event.
    pub fn event(&mut self, event: Event<'_>, span: &Span) -> Flow {
        let at = self.positions.span(span);
        match event {
            Event::DocumentStart(_) => return self.document(at),
            Event::Scalar(text, style, _, tag) => {
                self.check_tag(tag.is_some(), at);
                self.scalar(text, style, at);
            }
            Event::Alias(_) => {
                self.diagnostics.push(problems::alias(at));
                self.attach(Node { value: Value::Scalar(Scalar::new("", ScalarStyle::Plain)), span: at });
            }
            Event::SequenceStart(_, tag) => {
                return self.open(tag.is_some(), Frame::Seq { start: at, items: Vec::new() });
            }
            Event::MappingStart(_, tag) => {
                let frame = Frame::Map { start: at, entries: Vec::new(), key: KeyState::Expect };
                return self.open(tag.is_some(), frame);
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

    /// The root (a null scalar for an empty file; `None` if the file could not be read) and the
    /// diagnostics.
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

    fn scalar(&mut self, text: Cow<'_, str>, style: SaphyrStyle, at: SourceSpan) {
        let scalar = Scalar::new(text, style_of(style));
        if scalar.style == ScalarStyle::Plain {
            self.diagnostics.extend(cut::check(self.text, at));
        }
        self.attach(Node { value: Value::Scalar(scalar), span: at });
    }

    fn open(&mut self, tagged: bool, frame: Frame) -> Flow {
        let (Frame::Seq { start, .. } | Frame::Map { start, .. }) = &frame;
        self.check_tag(tagged, *start);
        if self.stack.len() >= MAX_DEPTH {
            self.diagnostics.push(problems::too_deep(*start, MAX_DEPTH));
            self.failed = true;
            return Flow::Stop;
        }
        self.stack.push(frame);
        Flow::Continue
    }

    fn close(&mut self, end: SourceSpan) {
        let node = match self.stack.pop() {
            Some(Frame::Seq { start, items }) => {
                let last = items.last().map(|item| item.span);
                Node { span: collection_span(start, last, end), value: Value::Seq(items) }
            }
            Some(Frame::Map { start, entries, .. }) => {
                let last = entries.last().map(|entry| entry.value.span);
                Node { span: collection_span(start, last, end), value: Value::Map(entries) }
            }
            None => return,
        };
        self.attach(node);
    }

    fn attach(&mut self, node: Node) {
        match self.stack.last_mut() {
            None => self.root = Some(node),
            Some(Frame::Seq { items, .. }) => items.push(node),
            Some(Frame::Map { entries, key, .. }) => match std::mem::replace(key, KeyState::Expect) {
                KeyState::Expect => *key = key_state(node, entries, &mut self.diagnostics),
                KeyState::Have(text, key_span) => entries.push(Entry { key: text, key_span, value: node }),
                KeyState::Skip => {}
            },
        }
    }
}

/// A flow collection ends at its closing bracket; a block one at the end of its last child.
fn collection_span(start: SourceSpan, last_child: Option<SourceSpan>, end: SourceSpan) -> SourceSpan {
    let end = if end.start == end.end { last_child.map_or(start.end, |last| last.end) } else { end.end };
    SourceSpan::new(start.file, start.start, end)
}

/// Accepts `node` as the next key unless it is not text or repeats an earlier key.
fn key_state(node: Node, entries: &[Entry], diagnostics: &mut Diagnostics) -> KeyState {
    let Value::Scalar(scalar) = node.value else {
        diagnostics.push(problems::complex_key(node.span));
        return KeyState::Skip;
    };
    if let Some(first) = entries.iter().find(|entry| entry.key == scalar.text) {
        diagnostics.push(problems::duplicate_key(node.span, first.key_span, &scalar.text));
        return KeyState::Skip;
    }
    KeyState::Have(scalar.text, node.span)
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
