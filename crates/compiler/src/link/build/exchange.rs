//! A stream call's `exchange` (D377): what it sends and the messages it waits for, in order.

use vc_model::{ExchangeItem, ExchangeKind, Matcher};
use vc_parser::{Node, Value};

use super::read;
use crate::link::project::Source;
use crate::parts::Piece;

/// The items of the `exchange` at `node`, as the compiler lowered them; an item that did not
/// lower is left out, since its file reported it.
pub fn read(source: &Source<'_>, node: &Node) -> Vec<ExchangeItem> {
    node.items().iter().filter_map(|item| Some(ExchangeItem { kind: kind(source, item)?, span: item.span })).collect()
}

/// A `send`, a `receive` of one message, or a `receive` of a group in any order.
fn kind(source: &Source<'_>, item: &Node) -> Option<ExchangeKind> {
    if let Some(send) = item.get("send") {
        return read::value(source, send).map(ExchangeKind::Send);
    }
    let receive = item.get("receive")?;
    match &receive.value {
        Value::Seq(checks) => {
            checks.iter().map(|check| matcher(source, check)).collect::<Option<_>>().map(ExchangeKind::Group)
        }
        _ => matcher(source, receive).map(ExchangeKind::Receive),
    }
}

/// The message check the compiler lowered at `node`.
fn matcher(source: &Source<'_>, node: &Node) -> Option<Matcher> {
    match source.piece(node)? {
        Piece::Matcher(matcher) => Some(matcher.clone()),
        _ => None,
    }
}
