//! The checks on the trees `vc fmt` reads and writes: the scalars written over several lines,
//! and whether the formatted tree holds the same values.

use std::ops::Range;

use super::Document;
use crate::{Entry, Node, Value};

/// The span of every scalar in `node` written over several lines.
pub(super) fn multi_line_scalars(doc: &Document, node: &Node, found: &mut Vec<Range<usize>>) {
    match &node.value {
        Value::Scalar(_) if doc.text.get(node.span.byte_range()).is_some_and(|text| text.contains('\n')) => {
            found.push(node.span.byte_range());
        }
        Value::Scalar(_) => {}
        Value::Seq(items) => items.iter().for_each(|item| multi_line_scalars(doc, item, found)),
        Value::Map(entries) => entries.iter().for_each(|entry| multi_line_scalars(doc, &entry.value, found)),
    }
}

/// Whether two trees hold the same values, whatever their order of keys and spelling.
pub(super) fn same(before: &Node, after: &Node) -> bool {
    match (&before.value, &after.value) {
        (Value::Scalar(old), Value::Scalar(new)) => old.text == new.text && old.kind == new.kind,
        (Value::Seq(old), Value::Seq(new)) => {
            old.len() == new.len() && old.iter().zip(new).all(|(old, new)| same(old, new))
        }
        (Value::Map(old), Value::Map(new)) => {
            let found =
                |entry: &Entry| new.iter().any(|other| other.key == entry.key && same(&entry.value, &other.value));
            old.len() == new.len() && old.iter().all(found)
        }
        _ => false,
    }
}
