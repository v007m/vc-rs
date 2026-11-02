use saphyr_parser::Parser;
use vc_diagnostics::Diagnostics;
use vc_model::SourceFileId;

use crate::Node;
use crate::build::{Builder, Flow};
use crate::positions::Positions;

/// Parses one YAML or JSON file into a [`Node`] tree, collecting every problem it can find.
///
/// The root is `None` only when the text cannot be read at all (V1001, or V1002 from the
/// scanner); an empty file gives a null scalar. Structure problems the tree can survive
/// (duplicate keys, aliases, tags, a second document) are reported and the tree is still
/// returned, so later stages can report more. JSON is parsed as the YAML 1.2 subset it is.
///
/// ```
/// use vc_model::SourceFileId;
///
/// let (root, diagnostics) = vc_parser::parse(SourceFileId::new(0), "id: getsUser\nretries: 2\n");
/// let root = root.unwrap();
/// assert!(diagnostics.is_empty());
/// assert_eq!(root.get("id").and_then(vc_parser::Node::as_str), Some("getsUser"));
/// assert_eq!(root.entries()[1].key_span.start.line, 2);
/// ```
#[must_use]
pub fn parse(file: SourceFileId, text: &str) -> (Option<Node>, Diagnostics) {
    let mut builder = Builder::new(text, Positions::new(file, text));
    let mut parser = Parser::new_from_str(text);
    while let Some(next) = parser.next_event() {
        match next {
            Ok((event, span)) => {
                if builder.event(event, &span) == Flow::Stop {
                    break;
                }
            }
            Err(error) => {
                builder.scan_error(&error);
                break;
            }
        }
    }
    builder.finish()
}
