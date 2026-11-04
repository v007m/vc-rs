//! The levels that can set a var at a spot, per profile, with what each definition holds.

use std::collections::HashMap;
use std::rc::Rc;

use vc_model::TemplateValue;
use vc_parser::Node;
use vc_template::ValueType;

use super::project;
use super::project::Source;
use crate::parts::Piece;

/// What a definition holds, when it is a literal; `None` when it is only known at run time.
pub type Type = Option<ValueType>;

/// One level: the names it sets for every profile, and those it sets for one profile only.
#[derive(Debug, Default)]
pub struct Layer {
    all: HashMap<String, Type>,
    by_profile: HashMap<String, HashMap<String, Type>>,
}

impl Layer {
    /// A level setting `names` for every profile, to values known only at run time: saves.
    pub fn names(names: impl IntoIterator<Item = String>) -> Self {
        Self { all: names.into_iter().map(|name| (name, None)).collect(), by_profile: HashMap::new() }
    }

    /// The `vars` of `node`, and the `vars` of each entry in its `profiles` list.
    pub fn vars(source: &Source<'_>, node: &Node) -> Self {
        let mut layer = Self { all: defs(source, node.get("vars")), by_profile: HashMap::new() };
        for profile in project::list(node, "profiles") {
            let Some(id) = project::text(profile, "id") else { continue };
            layer.by_profile.entry(id.to_owned()).or_default().extend(defs(source, profile.get("vars")));
        }
        layer
    }

    /// Adds every name `other` sets.
    pub fn absorb(&mut self, other: &Self) {
        self.all.extend(other.all.iter().map(|(name, ty)| (name.clone(), *ty)));
        for (profile, names) in &other.by_profile {
            let mine = self.by_profile.entry(profile.clone()).or_default();
            mine.extend(names.iter().map(|(name, ty)| (name.clone(), *ty)));
        }
    }

    fn sets(&self, name: &str, profile: &str) -> bool {
        self.all.contains_key(name) || self.by_profile.get(profile).is_some_and(|names| names.contains_key(name))
    }

    fn types<'s>(&'s self, name: &'s str) -> impl Iterator<Item = Type> + 's {
        let profiles = self.by_profile.values().filter_map(move |names| names.get(name));
        self.all.get(name).into_iter().chain(profiles).copied()
    }

    fn names_for<'s>(&'s self, profile: &'s str) -> impl Iterator<Item = &'s str> + 's {
        let profiles = self.by_profile.get(profile).into_iter().flat_map(HashMap::keys);
        self.all.keys().chain(profiles).map(String::as_str)
    }
}

/// The names in each entry of a `vars` map, with their types.
fn defs(source: &Source<'_>, vars: Option<&Node>) -> HashMap<String, Type> {
    let entries = vars.map(Node::entries).unwrap_or_default();
    entries.iter().map(|entry| (entry.key.clone(), source.piece(&entry.value).and_then(piece_type))).collect()
}

/// What a var's value holds, when that is known without running; a var is always a value.
const fn piece_type(piece: &Piece) -> Type {
    match piece {
        Piece::Value(value) => value_type(value),
        _ => None,
    }
}

const fn value_type(value: &TemplateValue) -> Type {
    match value {
        TemplateValue::Bool(_) => Some(ValueType::Bool),
        TemplateValue::Number(_) => Some(ValueType::Number),
        TemplateValue::String(template) if template.single_ref().is_none() => Some(ValueType::Text),
        TemplateValue::List(_) => Some(ValueType::List),
        TemplateValue::Map(_) => Some(ValueType::Object),
        _ => None,
    }
}

/// The levels a spot sees, and the action ids that have run before it in its test or block.
#[derive(Clone, Debug, Default)]
pub struct Scope<'a> {
    layers: Vec<Rc<Layer>>,
    /// The ids of the actions that ran earlier in the same test or block.
    pub steps: Vec<&'a str>,
}

impl<'a> Scope<'a> {
    /// This scope with one more level.
    #[must_use]
    pub fn with(&self, layer: Layer) -> Self {
        let mut scope = self.clone();
        scope.layers.push(Rc::new(layer));
        scope
    }

    /// Adds a level.
    pub fn push(&mut self, layer: Layer) {
        self.layers.push(Rc::new(layer));
    }

    /// Whether some level sets `name` for `profile`.
    pub fn sets(&self, name: &str, profile: &str) -> bool {
        self.layers.iter().any(|layer| layer.sets(name, profile))
    }

    /// Every name set for `profile`, sorted.
    pub fn names<'s>(&'s self, profile: &'s str) -> Vec<&'s str> {
        let mut names: Vec<&str> = self.layers.iter().flat_map(|layer| layer.names_for(profile)).collect();
        names.sort_unstable();
        names.dedup();
        names
    }

    /// The type of `name` when every definition of it is a literal of that one type.
    pub fn ty(&self, name: &str) -> Type {
        let mut types = self.layers.iter().flat_map(|layer| layer.types(name));
        let first = types.next()??;
        types.all(|ty| ty == Some(first)).then_some(first)
    }

    /// Whether `id` ran earlier in this test or block.
    pub fn ran(&self, id: &str) -> bool {
        self.steps.contains(&id)
    }

    /// Records that the action `id` has run.
    pub fn ran_now(&mut self, id: Option<&'a str>) {
        self.steps.extend(id);
    }
}
