//! `$extend` and `$remove` (syntax reference "Reusing a body"): a deep merge of the changes over
//! the base, then the removals.

use serde_json::Value;
use vc_model::{Path, Segment};

use crate::select;

/// `base` with `changes` merged over it and each path in `remove` taken out.
#[must_use]
pub fn extend(base: Value, changes: Value, remove: &[Path]) -> Value {
    let mut merged = merge(base, changes);
    for path in remove {
        remove_at(&mut merged, &path.segments);
    }
    merged
}

/// Objects merge key by key, recursively; anything else in `changes` replaces the base, and a
/// `null` sets `null`.
#[must_use]
pub fn merge(base: Value, changes: Value) -> Value {
    match (base, changes) {
        (Value::Object(mut base), Value::Object(changes)) => {
            for (key, change) in changes {
                match base.get_mut(&key) {
                    Some(slot) => *slot = merge(slot.take(), change),
                    None => {
                        base.insert(key, change);
                    }
                }
            }
            Value::Object(base)
        }
        (_, changes) => changes,
    }
}

/// Takes out the value at `path`; `[*]` applies the rest of the path to every element, and a
/// path that leads nowhere changes nothing.
fn remove_at(value: &mut Value, path: &[Segment]) {
    match path {
        [] => {}
        [last] => remove_child(value, last),
        [first, rest @ ..] => {
            for child in children(value, first) {
                remove_at(child, rest);
            }
        }
    }
}

fn remove_child(value: &mut Value, segment: &Segment) {
    match (segment, value) {
        (Segment::Key(key), Value::Object(map)) => {
            map.remove(key);
        }
        (Segment::Index(index), Value::Array(items)) => {
            if let Some(at) = select::position(*index, items.len()).filter(|at| *at < items.len()) {
                items.remove(at);
            }
        }
        (Segment::All, Value::Array(items)) => items.clear(),
        _ => {}
    }
}

fn children<'v>(value: &'v mut Value, segment: &Segment) -> Vec<&'v mut Value> {
    match (segment, value) {
        (Segment::Key(key), Value::Object(map)) => map.get_mut(key).into_iter().collect(),
        (Segment::Index(index), Value::Array(items)) => {
            let at = select::position(*index, items.len());
            at.and_then(|at| items.get_mut(at)).into_iter().collect()
        }
        (Segment::All, Value::Array(items)) => items.iter_mut().collect(),
        _ => Vec::new(),
    }
}
