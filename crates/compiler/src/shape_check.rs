//! The shape-check stage: walks a parsed file against its [`Shape`] and reports unknown keys
//! (V1004), missing keys (V1005, V1003), wrong value types (V2302) and one-of groups (V2201).
//! Values the next stage compiles (templates, units, matchers, CEL) are collected as leaves.

use vc_diagnostics::{Code, Diagnostic, Diagnostics, Fix, TextEdit};
use vc_model::Date;
use vc_parser::{Entry, Node, ScalarKind, Value};

use crate::shapes::{Key, Scope, Shape, Ty, Unit};
use crate::suggest::{self, Misspelling};

/// What a leaf compiles to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeafTy {
    /// A template.
    Template,
    /// A duration, written out (a template is V2301).
    Duration,
    /// A size, written out (a template is V2301).
    Size,
    /// A status, a list of them, or a matcher.
    Status,
    /// A literal or a matcher, comparing in a unit.
    Matcher(Unit),
    /// A CEL expression.
    Cel,
    /// Any value.
    Value,
}

/// A value for the compile-parts stage: where it is in the tree and what it compiles to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Leaf {
    /// What it compiles to.
    pub ty: LeafTy,
    /// Child indexes from the root: list items, or map entries in written order.
    pub path: Vec<usize>,
    /// The roots its templates may read (D125).
    pub scope: Scope,
}

/// What the walk found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Walk {
    /// The values to compile, in written order.
    pub leaves: Vec<Leaf>,
    /// The `id` of every action, so CEL can read the steps of the file (the linker checks order).
    pub steps: Vec<String>,
    /// Every problem found.
    pub diagnostics: Diagnostics,
}

/// The shape whose `id`s name steps.
const STEP_SHAPE: &str = "action";

/// Walks `root` against `shape`.
pub fn check(root: &Node, shape: &'static Shape) -> Walk {
    let mut walker = Walker { walk: Walk::default(), path: Vec::new() };
    walker.map(root, shape, Scope::NONE);
    walker.walk
}

/// The node at `path` under `root`, as a [`Leaf`] records it.
pub fn at<'n>(root: &'n Node, path: &[usize]) -> Option<&'n Node> {
    path.iter().try_fold(root, |node, &index| match &node.value {
        Value::Seq(items) => items.get(index),
        Value::Map(entries) => entries.get(index).map(|entry| &entry.value),
        Value::Scalar(_) => None,
    })
}

struct Walker {
    walk: Walk,
    path: Vec<usize>,
}

impl Walker {
    fn push(&mut self, diagnostic: Diagnostic) {
        self.walk.diagnostics.push(diagnostic);
    }

    fn value(&mut self, node: &Node, ty: Ty, scope: Scope) {
        if !accepts(ty, node) {
            self.push(mismatch(node, ty));
            return;
        }
        match ty {
            Ty::Obj(shape) => self.map(node, shape(), scope),
            Ty::List(item) | Ty::MapOf(item) => self.children(node, *item, scope),
            Ty::Either(alternatives) => self.either(node, alternatives, scope),
            Ty::Enum(words) => self.word(node, words),
            other => self.leaf(other, scope),
        }
    }

    fn children(&mut self, node: &Node, ty: Ty, scope: Scope) {
        let children: Vec<&Node> = match &node.value {
            Value::Seq(items) => items.iter().collect(),
            Value::Map(entries) => entries.iter().map(|entry| &entry.value).collect(),
            Value::Scalar(_) => Vec::new(),
        };
        for (index, child) in children.into_iter().enumerate() {
            self.path.push(index);
            self.value(child, ty, scope);
            self.path.pop();
        }
    }

    fn either(&mut self, node: &Node, alternatives: &[Ty], scope: Scope) {
        if let Some(ty) = alternatives.iter().find(|ty| accepts(**ty, node)) {
            self.value(node, *ty, scope);
        }
    }

    fn word(&mut self, node: &Node, words: &'static [&'static str]) {
        let Some(text) = node.as_str().filter(|text| !words.contains(text)) else { return };
        let message = format!("expected one of {}, found `{text}`", words.join(", "));
        let diagnostic = Diagnostic::new(Code::V2302, node.span, message).label("not a known value");
        self.push(suggest::hint(diagnostic, &Misspelling { word: text, span: node.span, known: words }));
    }

    fn leaf(&mut self, ty: Ty, scope: Scope) {
        if let Some(ty) = leaf_ty(ty) {
            self.walk.leaves.push(Leaf { ty, path: self.path.clone(), scope });
        }
    }

    fn map(&mut self, node: &Node, shape: &'static Shape, scope: Scope) {
        let Value::Map(entries) = &node.value else {
            let message = format!("expected {} map, found {}", article(shape.title), found(node));
            return self.push(Diagnostic::new(Code::V2302, node.span, message));
        };
        for (index, entry) in entries.iter().enumerate() {
            let Some(key) = shape.find(&entry.key) else {
                self.push(unknown_key(entry, shape));
                continue;
            };
            self.path.push(index);
            self.value(&entry.value, key.ty, scope.with(key.scope).with(sibling_scope(shape, entries, key)));
            self.path.pop();
        }
        self.required(node, shape);
        self.one_of(node, shape);
        self.step(node, shape);
    }

    fn required(&mut self, node: &Node, shape: &'static Shape) {
        let at = node.entries().first().map_or(node.span, |entry| entry.key_span);
        for key in shape.all_keys().filter(|key| key.required && node.get(key.name).is_none()) {
            let message = format!("{} needs `{}`", article(shape.title), key.name);
            let diagnostic = Diagnostic::new(key.missing, at, message).label(format!("`{}` is missing", key.name));
            self.push(diagnostic.help(add_key(key)));
        }
    }

    fn one_of(&mut self, node: &Node, shape: &'static Shape) {
        let Some(group) = shape.one_of else { return };
        let written: Vec<&Entry> = node.entries().iter().filter(|e| group.keys.contains(&e.key.as_str())).collect();
        let keys = suggest::quoted(group.keys);
        let diagnostic = match written.as_slice() {
            [_] => return,
            [] => {
                let at = node.entries().first().map_or(node.span, |entry| entry.key_span);
                Diagnostic::new(group.code, at, format!("{} needs one of {keys}", article(shape.title)))
            }
            [first, second, ..] => Diagnostic::new(group.code, second.key_span, format!("write only one of {keys}"))
                .label(format!("`{}` is also written", second.key))
                .secondary(first.key_span, format!("`{}` is written here", first.key)),
        };
        self.push(diagnostic);
    }

    fn step(&mut self, node: &Node, shape: &'static Shape) {
        if shape.title == STEP_SHAPE
            && let Some(id) = node.get("id").and_then(Node::as_str)
        {
            self.walk.steps.push(id.to_owned());
        }
    }
}

/// The roots that sibling keys such as `examples` and `forEach` add for `key`.
fn sibling_scope(shape: &Shape, entries: &[Entry], key: &Key) -> Scope {
    shape
        .scopes
        .iter()
        .filter(|(name, _)| *name != key.name && entries.iter().any(|entry| entry.key == *name))
        .fold(Scope::NONE, |scope, (_, added)| scope.with(*added))
}

/// Whether `node` has the kind `ty` takes; scalars are checked loosely where the next stage
/// parses them (durations, sizes, statuses).
fn accepts(ty: Ty, node: &Node) -> bool {
    match ty {
        Ty::Either(alternatives) => alternatives.iter().any(|ty| accepts(*ty, node)),
        Ty::Status | Ty::Value | Ty::Matcher(_) => true,
        Ty::Obj(_) | Ty::MapOf(_) => matches!(node.value, Value::Map(_)),
        Ty::List(_) => matches!(node.value, Value::Seq(_)),
        Ty::Schema => matches!(node.value, Value::Map(_)) || node.as_str().is_some(),
        Ty::Date => node.as_str().is_some_and(|text| Date::parse(text).is_some()),
        scalar => node.as_scalar().is_some_and(|value| scalar_accepts(scalar, value.kind)),
    }
}

fn scalar_accepts(ty: Ty, kind: ScalarKind) -> bool {
    match ty {
        Ty::Int => kind == ScalarKind::Int,
        Ty::Bool => kind == ScalarKind::Bool,
        Ty::Id | Ty::Ref | Ty::Cel | Ty::Enum(_) => kind == ScalarKind::String,
        _ => kind != ScalarKind::Null,
    }
}

const fn leaf_ty(ty: Ty) -> Option<LeafTy> {
    match ty {
        Ty::Template => Some(LeafTy::Template),
        Ty::Duration => Some(LeafTy::Duration),
        Ty::Size => Some(LeafTy::Size),
        Ty::Status => Some(LeafTy::Status),
        Ty::Matcher(unit) => Some(LeafTy::Matcher(unit)),
        Ty::Cel => Some(LeafTy::Cel),
        Ty::Value => Some(LeafTy::Value),
        _ => None,
    }
}

const fn found(node: &Node) -> &'static str {
    match &node.value {
        Value::Scalar(scalar) => scalar.kind.name(),
        _ => node.kind_name(),
    }
}

fn mismatch(node: &Node, ty: Ty) -> Diagnostic {
    let message = format!("expected {}, found {}", ty.describe(), found(node));
    let diagnostic = Diagnostic::new(Code::V2302, node.span, message).label(format!("expected {}", ty.describe()));
    match unquoted(node, ty) {
        Some(text) => {
            let help = format!("write `{text}` without quotes");
            let fix = Fix::certain(format!("Write {text} without quotes"), vec![TextEdit::replace(node.span, text)]);
            diagnostic.help(help).fix(fix)
        }
        None => diagnostic,
    }
}

/// The text of a quoted scalar that reads as the integer or boolean `ty` takes.
fn unquoted(node: &Node, ty: Ty) -> Option<String> {
    let scalar = node.as_scalar().filter(|scalar| scalar.kind == ScalarKind::String)?;
    let plain = vc_parser::Scalar::new(scalar.text.clone(), vc_parser::ScalarStyle::Plain);
    let wanted = match ty {
        Ty::Int => matches!(plain.kind, ScalarKind::Int),
        Ty::Bool => plain.kind == ScalarKind::Bool,
        _ => false,
    };
    wanted.then_some(plain.text)
}

fn unknown_key(entry: &Entry, shape: &'static Shape) -> Diagnostic {
    let message = format!("unknown key `{}` in {}", entry.key, shape.title);
    let diagnostic = Diagnostic::new(Code::V1004, entry.key_span, message).label("not a known key");
    let known: Vec<&str> = shape.all_keys().map(|key| key.name).collect();
    suggest::hint(diagnostic, &Misspelling { word: &entry.key, span: entry.key_span, known: &known })
}

fn add_key(key: &Key) -> String {
    let written = key.example.map_or_else(|| key.name.to_owned(), |example| format!("{}: {example}", key.name));
    format!("add `{written}`: {}", key.doc)
}

/// "an api", "a test".
fn article(title: &str) -> String {
    let vowel = title.starts_with(['a', 'e', 'i', 'o', 'u']);
    format!("{} {title}", if vowel { "an" } else { "a" })
}

#[cfg(test)]
mod tests;
