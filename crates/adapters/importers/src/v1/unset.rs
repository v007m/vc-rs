//! A var an api reads that nothing in the migration sets: v1 set it in a script. The api gets
//! it as an empty var, with a TODO to give it a value, so the project checks clean (V2101).

use std::collections::BTreeSet;

use serde_json::{Map, Value, json};
use vc_interfaces::ImportNote;

use super::MigratedFile;

/// The keys whose map sets vars: `vars` and a call's `save`.
const SETTERS: [&str; 2] = ["vars", "save"];

/// Gives each api the vars it reads that no file sets, each with a TODO.
pub fn fill(files: &mut [MigratedFile]) {
    let mut known = BTreeSet::new();
    for file in files.iter() {
        set(&file.doc, &mut known);
    }
    for file in files.iter_mut().filter(|file| file.path.ends_with(".api.yaml")) {
        let apis = file.doc.get_mut("apis").and_then(Value::as_array_mut).into_iter().flatten();
        for api in apis.filter_map(Value::as_object_mut) {
            let unset: BTreeSet<String> = read(&Value::Object(api.clone())).difference(&known).cloned().collect();
            let at = api.get("id").and_then(Value::as_str).unwrap_or_default().to_owned();
            for name in unset {
                let message = format!("{name} is set only by a v1 script; give it a value");
                file.todos.push(ImportNote { at: at.clone(), message });
                give(api, name);
            }
        }
    }
}

/// Adds the names `value` sets anywhere in it to `known`.
fn set(value: &Value, known: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (key, inner) in map {
                if SETTERS.contains(&key.as_str()) {
                    known.extend(inner.as_object().into_iter().flat_map(Map::keys).cloned());
                }
                set(inner, known);
            }
        }
        Value::Array(items) => items.iter().for_each(|item| set(item, known)),
        _ => {}
    }
}

/// The names of the vars `value` reads with `${var.<name>…}`.
fn read(value: &Value) -> BTreeSet<String> {
    let text = value.to_string();
    let reads = text.split("${var.").skip(1);
    let names =
        reads.map(|rest| rest.split(|c: char| !c.is_ascii_alphanumeric() && c != '_').next().unwrap_or_default());
    names.filter(|name| !name.is_empty()).map(str::to_owned).collect()
}

/// Gives the api the var `name`, empty.
fn give(api: &mut Map<String, Value>, name: String) {
    if let Value::Object(vars) = api.entry("vars").or_insert_with(|| json!({})) {
        vars.insert(name, json!(""));
    }
}
