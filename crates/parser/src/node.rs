use serde::{Deserialize, Serialize};
use vc_model::SourceSpan;

use crate::Scalar;

/// One node of a parsed file: a value and the span it was read from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    /// What the node holds.
    pub value: Value,
    /// Where it was written; for a block collection, from its first to the end of its last child.
    pub span: SourceSpan,
}

/// The value of a [`Node`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Value {
    /// Text, a number, a boolean or null.
    Scalar(Scalar),
    /// A list, in source order.
    Seq(Vec<Node>),
    /// A map, in source order; keys are unique.
    Map(Vec<Entry>),
}

/// One `key: value` pair of a map.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// The key's text.
    pub key: String,
    /// Where the key was written, so diagnostics can underline it.
    pub key_span: SourceSpan,
    /// The value.
    pub value: Node,
}

impl Node {
    /// The value under `key`, if this is a map that has it.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Self> {
        self.entry(key).map(|entry| &entry.value)
    }

    /// The entry for `key`, with the key's span, if this is a map that has it.
    #[must_use]
    pub fn entry(&self, key: &str) -> Option<&Entry> {
        self.entries().iter().find(|entry| entry.key == key)
    }

    /// The entries in source order; empty unless this is a map.
    #[must_use]
    pub fn entries(&self) -> &[Entry] {
        if let Value::Map(entries) = &self.value { entries } else { &[] }
    }

    /// The items in source order; empty unless this is a list.
    #[must_use]
    pub fn items(&self) -> &[Self] {
        if let Value::Seq(items) = &self.value { items } else { &[] }
    }

    /// The scalar, if this is one.
    #[must_use]
    pub const fn as_scalar(&self) -> Option<&Scalar> {
        if let Value::Scalar(scalar) = &self.value { Some(scalar) } else { None }
    }

    /// The text, if this is a scalar that resolves to a string.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        self.as_scalar().filter(|s| s.kind == crate::ScalarKind::String).map(|s| s.text.as_str())
    }

    /// What the node is, for messages such as "expected a map, found a list".
    #[must_use]
    pub const fn kind_name(&self) -> &'static str {
        match &self.value {
            Value::Map(_) => "a map",
            Value::Seq(_) => "a list",
            Value::Scalar(_) => "a value",
        }
    }
}
