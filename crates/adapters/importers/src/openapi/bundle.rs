//! Bundling: every `$ref` to another document is replaced by what it points to, so the
//! snapshot is one self-contained file. References inside the root document stay references.

use std::collections::HashMap;

use serde_json::Value;
use vc_interfaces::{FetchError, ImportError, SpecFetcher, resolve_reference};

use super::values::decoded;

/// How many documents one spec may pull in, the root included.
const MAX_DOCUMENTS: usize = 64;

/// The documents a spec is made of, by location.
type Documents = HashMap<String, Value>;

/// `root`, read from `location`, with the documents it refers to fetched and inlined.
///
/// # Errors
///
/// [`ImportError::Fetch`] when a document cannot be fetched or decoded, or a reference leaves
/// what may be read; [`ImportError::Cycle`] when inlining leads back to a reference being
/// inlined; [`ImportError::Invalid`] when a reference names nothing.
pub async fn bundle(fetcher: &dyn SpecFetcher, location: &str, root: Value) -> Result<Value, ImportError> {
    let documents = fetch_all(fetcher, location, &root).await?;
    let mut bundled = root;
    let mut inliner = Inliner { root: location, documents: &documents, stack: Vec::new() };
    inliner.inline(&mut bundled, location)?;
    Ok(bundled)
}

/// Every document reachable from `root`, the root itself under `location`.
async fn fetch_all(fetcher: &dyn SpecFetcher, location: &str, root: &Value) -> Result<Documents, ImportError> {
    let mut documents = Documents::new();
    let mut queue = external(location, root)?;
    documents.insert(location.to_owned(), root.clone());
    while let Some(next) = queue.pop() {
        if documents.contains_key(&next) {
            continue;
        }
        if documents.len() >= MAX_DOCUMENTS {
            let reason = format!("it refers to more than {MAX_DOCUMENTS} documents");
            return Err(ImportError::Invalid { location: location.to_owned(), reason });
        }
        let bytes = fetcher.fetch(&next).await?;
        let document = fetcher.decode(&next, &bytes)?;
        queue.extend(external(&next, &document)?);
        documents.insert(next, document);
    }
    Ok(documents)
}

/// The other documents `value`, read from `base`, refers to.
fn external(base: &str, value: &Value) -> Result<Vec<String>, ImportError> {
    let mut found = Vec::new();
    refs(value, &mut |reference| {
        if !reference.starts_with('#') {
            found.push(target(base, reference)?);
        }
        Ok(())
    })?;
    Ok(found)
}

/// Calls `visit` with every `$ref` in `value`.
fn refs<F>(value: &Value, visit: &mut F) -> Result<(), ImportError>
where
    F: FnMut(&str) -> Result<(), ImportError>,
{
    match value {
        Value::Object(map) => {
            if let Some(Value::String(reference)) = map.get("$ref") {
                visit(reference)?;
            }
            map.values().try_for_each(|value| refs(value, visit))
        }
        Value::Array(items) => items.iter().try_for_each(|value| refs(value, visit)),
        _ => Ok(()),
    }
}

/// The document `reference` names, read from `base`.
fn target(base: &str, reference: &str) -> Result<String, ImportError> {
    resolve_reference(base, reference).ok_or_else(|| FetchError::NotAllowed { location: reference.to_owned() }.into())
}

struct Inliner<'a> {
    root: &'a str,
    documents: &'a Documents,
    /// The references being inlined, outermost first, as `location#pointer`.
    stack: Vec<String>,
}

impl Inliner<'_> {
    /// Inlines the references in `value`, which was read from document `at`.
    fn inline(&mut self, value: &mut Value, at: &str) -> Result<(), ImportError> {
        let reference = value.get("$ref").and_then(Value::as_str).map(str::to_owned);
        match (reference, value) {
            (Some(reference), value) => self.replace(value, at, &reference),
            (None, Value::Object(map)) => map.values_mut().try_for_each(|value| self.inline(value, at)),
            (None, Value::Array(items)) => items.iter_mut().try_for_each(|value| self.inline(value, at)),
            (None, _) => Ok(()),
        }
    }

    /// Replaces `value`, a `$ref` read from `at`, by its target; a reference into the root
    /// becomes a local one.
    fn replace(&mut self, value: &mut Value, at: &str, reference: &str) -> Result<(), ImportError> {
        let location = target(at, reference)?;
        let pointer = reference.split_once('#').map_or("", |(_, pointer)| pointer);
        if location == self.root {
            *value = serde_json::json!({ "$ref": format!("#{pointer}") });
            return Ok(());
        }
        let key = format!("{location}#{pointer}");
        if let Some(first) = self.stack.iter().position(|seen| *seen == key) {
            let chain = self.stack.get(first..).unwrap_or_default().iter().cloned().chain([key]).collect();
            return Err(ImportError::Cycle { chain });
        }
        let found = self.documents.get(&location).and_then(|document| document.pointer(&decoded(pointer)));
        let mut found = found.cloned().ok_or_else(|| ImportError::Invalid {
            location: at.to_owned(),
            reason: format!("`$ref: {reference}` names nothing"),
        })?;
        self.stack.push(key);
        self.inline(&mut found, &location)?;
        self.stack.pop();
        *value = found;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
