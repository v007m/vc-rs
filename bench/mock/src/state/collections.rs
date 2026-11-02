use std::collections::{BTreeMap, HashMap};

use serde_json::{Map, Value};

type Item = Map<String, Value>;

/// In-memory CRUD resources (`users`, `orders`), keyed by collection and numeric id.
#[derive(Default)]
pub struct Collections {
    last_id: u64,
    items: HashMap<&'static str, BTreeMap<u64, Item>>,
}

impl Collections {
    pub fn create(&mut self, kind: &'static str, mut item: Item) -> Value {
        self.last_id += 1;
        item.insert("id".to_owned(), self.last_id.into());
        self.items.entry(kind).or_default().insert(self.last_id, item.clone());
        Value::Object(item)
    }

    pub fn get(&self, kind: &str, id: u64) -> Option<Value> {
        self.items.get(kind)?.get(&id).cloned().map(Value::Object)
    }

    pub fn list(&self, kind: &str) -> Value {
        let items = self.items.get(kind).into_iter().flat_map(BTreeMap::values);
        Value::Array(items.cloned().map(Value::Object).collect())
    }

    /// Replaces the item (PUT), keeping its id.
    pub fn replace(&mut self, kind: &str, id: u64, mut item: Item) -> Option<Value> {
        let slot = self.items.get_mut(kind)?.get_mut(&id)?;
        item.insert("id".to_owned(), id.into());
        *slot = item;
        Some(Value::Object(slot.clone()))
    }

    /// Merges top-level fields into the item (PATCH), keeping its id.
    pub fn merge(&mut self, kind: &str, id: u64, patch: Item) -> Option<Value> {
        let slot = self.items.get_mut(kind)?.get_mut(&id)?;
        slot.extend(patch);
        slot.insert("id".to_owned(), id.into());
        Some(Value::Object(slot.clone()))
    }

    pub fn delete(&mut self, kind: &str, id: u64) -> bool {
        self.items.get_mut(kind).and_then(|items| items.remove(&id)).is_some()
    }
}
