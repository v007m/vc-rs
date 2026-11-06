//! The YAML reader: saphyr's events turned into [`Builder`] calls.

use std::borrow::Cow;

use saphyr_parser::{Event, Parser, ScalarStyle as SaphyrStyle};
use vc_diagnostics::Diagnostics;
use vc_model::{SourceFileId, SourceSpan};

use crate::build::{Builder, Flow, Shape};
use crate::positions::{self, Positions};
use crate::{Node, Scalar, ScalarKind, ScalarStyle, cut, problems};

/// Reads `text` as one YAML document.
pub fn read(file: SourceFileId, text: &str) -> (Option<Node>, Diagnostics) {
    let mut reader = Reader { text, positions: Positions::new(file, text), builder: Builder::new(file), documents: 0 };
    let mut parser = Parser::new_from_str(text);
    while let Some(next) = parser.next_event() {
        match next {
            Ok((event, span)) => {
                if reader.event(event, reader.positions.span(&span)) == Flow::Stop {
                    break;
                }
            }
            Err(error) => {
                let at = reader.positions.char_at(error.marker());
                reader.builder.fail(problems::scan_error(at, error.info()));
                break;
            }
        }
    }
    reader.builder.finish()
}

struct Reader<'t> {
    text: &'t str,
    positions: Positions,
    builder: Builder,
    documents: usize,
}

impl Reader<'_> {
    /// Takes one event. Node spans leave out `&anchor` and `!tag`; tag diagnostics keep them.
    fn event(&mut self, event: Event<'_>, at: SourceSpan) -> Flow {
        match event {
            Event::DocumentStart(_) => return self.document(at),
            Event::Scalar(text, style, _, tag) => {
                self.check_tag(tag.is_some(), at);
                let at = positions::to_closing_quote(self.text, positions::without_properties(self.text, at));
                self.scalar(text, style, at);
            }
            Event::Alias(_) => {
                self.builder.report(problems::alias(at));
                self.builder.scalar(Scalar::new("", ScalarStyle::Plain), at);
            }
            Event::SequenceStart(_, tag) => {
                self.check_tag(tag.is_some(), at);
                return self.builder.open(Shape::Seq, positions::without_properties(self.text, at));
            }
            Event::MappingStart(_, tag) => {
                self.check_tag(tag.is_some(), at);
                return self.builder.open(Shape::Map, positions::without_properties(self.text, at));
            }
            Event::SequenceEnd | Event::MappingEnd => self.builder.close(at),
            _ => {}
        }
        Flow::Continue
    }

    fn document(&mut self, at: SourceSpan) -> Flow {
        self.documents += 1;
        if self.documents == 1 {
            return Flow::Continue;
        }
        self.builder.report(problems::second_document(at));
        Flow::Stop
    }

    fn check_tag(&mut self, tagged: bool, at: SourceSpan) {
        if tagged {
            self.builder.report(problems::tag(at));
        }
    }

    /// Only plain text is checked for a cut at ` #`: a typed value such as `3 #max` is left alone.
    fn scalar(&mut self, text: Cow<'_, str>, style: SaphyrStyle, at: SourceSpan) {
        let scalar = Scalar::new(text, style_of(style));
        if scalar.style == ScalarStyle::Plain
            && scalar.kind == ScalarKind::String
            && let Some(cut) = cut::check(self.text, at)
        {
            self.builder.report(cut);
        }
        self.builder.scalar(scalar, at);
    }
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
