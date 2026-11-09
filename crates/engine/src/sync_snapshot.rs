//! The snapshot a sync writes: the spec as it is now, except that an operation left in conflict
//! keeps the entry the old snapshot had for it (or none), so the next sync sees the same
//! conflict until it is settled.

use serde_json::{Map, Value};

/// `theirs` with each operation in `conflicted` as `base` has it.
pub fn kept(theirs: &Value, base: Option<&Value>, conflicted: &[String]) -> Value {
    let mut doc = theirs.clone();
    let mut restored = false;
    for op in conflicted {
        take_out(&mut doc, op);
        if let Some((path, method, old)) = base.and_then(|base| locate(base, op)) {
            put_back(&mut doc, (path, method), old);
            restored = true;
        }
    }
    if restored && let Some(old) = base.and_then(|base| base.get("components")) {
        components(&mut doc, old);
    }
    doc
}

/// The old snapshot's components that `doc` lacks, added, so an entry put back still resolves.
fn components(doc: &mut Value, old: &Value) {
    if let Some(root) = doc.as_object_mut() {
        fill(root.entry("components").or_insert_with(|| Value::Object(Map::new())), old, 2);
    }
}

/// Each key of `from` that `into` lacks, added; at `depth` 2 also each key one map down.
fn fill(into: &mut Value, from: &Value, depth: u8) {
    let (Some(into), Some(from)) = (into.as_object_mut(), from.as_object()) else {
        return;
    };
    for (key, value) in from {
        match into.get_mut(key) {
            Some(have) if depth > 1 => fill(have, value, depth - 1),
            Some(_) => {}
            None => {
                into.insert(key.clone(), value.clone());
            }
        }
    }
}

/// The path, method and operation object whose `operationId` is `op`.
fn locate<'d>(doc: &'d Value, op: &str) -> Option<(&'d str, &'d str, &'d Value)> {
    let paths = doc.get("paths")?.as_object()?;
    paths.iter().find_map(|(path, item)| {
        let mut operations = item.as_object()?.iter();
        let (method, operation) =
            operations.find(|(_, operation)| operation.get("operationId").and_then(Value::as_str) == Some(op))?;
        Some((path.as_str(), method.as_str(), operation))
    })
}

fn take_out(doc: &mut Value, op: &str) {
    let Some((path, method)) = locate(doc, op).map(|(path, method, _)| (path.to_owned(), method.to_owned())) else {
        return;
    };
    let item = doc.get_mut("paths").and_then(|paths| paths.get_mut(&path)).and_then(Value::as_object_mut);
    if let Some(item) = item {
        item.remove(&method);
    }
}

fn put_back(doc: &mut Value, (path, method): (&str, &str), old: &Value) {
    let Some(paths) = doc.get_mut("paths").and_then(Value::as_object_mut) else {
        return;
    };
    let item = paths.entry(path.to_owned()).or_insert_with(|| Value::Object(Map::new()));
    if let Some(item) = item.as_object_mut() {
        item.insert(method.to_owned(), old.clone());
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::kept;

    fn spec(path: &str, method: &str) -> serde_json::Value {
        let a = json!({ "get": { "operationId": "listA" } });
        json!({ "openapi": "3.1.0", "paths": { (path): { (method): { "operationId": "updateUser" } }, "/a": a } })
    }

    #[test]
    fn a_conflicted_operation_keeps_its_old_entry_and_the_rest_moves_on() {
        let (base, theirs) = (spec("/users/{id}", "put"), spec("/users/{id}", "patch"));
        let doc = kept(&theirs, Some(&base), &["updateUser".to_owned()]);
        assert_eq!(doc, base);
        let moved = kept(&spec("/people/{id}", "put"), Some(&base), &["updateUser".to_owned()]);
        assert_eq!(moved["paths"]["/users/{id}"]["put"]["operationId"], "updateUser");
        assert_eq!(moved["paths"]["/people/{id}"], json!({}));
    }

    #[test]
    fn an_entry_put_back_keeps_the_old_components_it_may_refer_to() {
        let mut base = spec("/users/{id}", "put");
        base["components"] = json!({ "schemas": { "Old": { "type": "string" }, "User": { "required": ["id"] } } });
        let mut theirs = spec("/users/{id}", "patch");
        theirs["components"] = json!({ "schemas": { "User": { "type": "object" } } });
        let doc = kept(&theirs, Some(&base), &["updateUser".to_owned()]);
        let schemas = json!({ "Old": { "type": "string" }, "User": { "type": "object" } });
        assert_eq!(doc["components"], json!({ "schemas": schemas }));
        assert_eq!(kept(&theirs, Some(&base), &[])["components"], theirs["components"]);
        assert_eq!(kept(&theirs, Some(&base), &["newOne".to_owned()])["components"], theirs["components"]);
    }

    #[test]
    fn with_no_conflict_or_no_old_entry_the_snapshot_is_the_spec_without_it() {
        let theirs = spec("/users/{id}", "patch");
        assert_eq!(kept(&theirs, None, &[]), theirs);
        let doc = kept(&theirs, None, &["updateUser".to_owned()]);
        assert_eq!(doc["paths"]["/users/{id}"], json!({}));
        assert_eq!(doc["paths"]["/a"]["get"]["operationId"], "listA");
    }
}
