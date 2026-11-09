//! The recording as one test (`--flow`): a step per kept request, in recorded order. A value a
//! response gave back (an ID, or a number under a key named like one) that a later request
//! sends is chained: the step that first got it saves it, `cartId: ${response.body.id}`, and
//! the later step sends `${var.cartId}`, so the test replays with fresh data. A step sends only
//! the vars that are chained or differ from its api's defaults. Only templates over a response
//! are written, never a value it held.

use indexmap::IndexMap;
use serde_json::{Value, json};
use vc_interfaces::{DraftFlow, DraftStep};

use super::entry::Response;
use crate::path_vars::identifier;

/// One kept request: the api it calls (`shop#addItem`), the vars it sent and the api's own.
pub struct Recorded {
    pub call: String,
    pub vars: IndexMap<String, Value>,
    pub defaults: IndexMap<String, Value>,
}

/// How deep in a response values are looked for, and how many items of a list.
const DEPTH: usize = 6;
const ITEMS: usize = 10;

/// The flow `id` (`<fileId>#<testId>`; a bare test ID goes in `<fallback>.recorded`) replaying
/// `steps`, each request with what came back.
pub fn flow<'r>(id: &str, fallback: &str, steps: impl IntoIterator<Item = (Recorded, &'r Response)>) -> DraftFlow {
    let (file_id, test_id) = id.split_once('#').map_or_else(
        || (format!("{fallback}.recorded"), id.to_owned()),
        |(file, test)| (file.to_owned(), test.to_owned()),
    );
    let mut chain = Chain::default();
    for (recorded, response) in steps {
        chain.step(recorded, response);
    }
    DraftFlow { file_id, test_id, steps: chain.steps }
}

/// The steps so far, and where each value a response gave back first came back.
#[derive(Default)]
struct Chain {
    steps: Vec<DraftStep>,
    seen: IndexMap<String, (usize, String)>,
}

impl Chain {
    fn step(&mut self, recorded: Recorded, response: &Response) {
        let mut vars = IndexMap::new();
        for (name, value) in recorded.vars {
            if let Some(saved) = self.chained(&name, &value) {
                vars.insert(name, json!(format!("${{var.{saved}}}")));
            } else if recorded.defaults.get(&name) != Some(&value) {
                vars.insert(name, value);
            }
        }
        let at = self.steps.len();
        self.steps.push(DraftStep { call: recorded.call, vars, ..DraftStep::default() });
        let mut found = Vec::new();
        if let Some(body) = body(response) {
            leaves(&body, "response.body", &mut found);
        }
        for (text, template) in found {
            self.seen.entry(text).or_insert((at, template));
        }
    }

    /// The name the text `value`, sent as the var `name`, is saved under when a response gave
    /// it back (a path's `42` matches a response's `"id": 42`).
    fn chained(&mut self, name: &str, value: &Value) -> Option<String> {
        let (step, read) = self.seen.get(value.as_str()?)?.clone();
        let template = format!("${{{read}}}");
        let taken = self.steps.iter().map(|step| step.save.len()).sum::<usize>();
        let save = &mut self.steps.get_mut(step)?.save;
        if let Some((saved, _)) = save.iter().find(|(_, kept)| **kept == template) {
            return Some(saved.clone());
        }
        let mut names = (1..=taken + 1).map(|n| if n == 1 { name.to_owned() } else { format!("{name}{n}") });
        let saved = names.find(|candidate| !self.saves(candidate))?;
        self.steps.get_mut(step)?.save.insert(saved.clone(), template);
        Some(saved)
    }

    fn saves(&self, name: &str) -> bool {
        self.steps.iter().any(|step| step.save.contains_key(name))
    }
}

/// The response body as JSON, when it was recorded as sent and is JSON.
fn body(response: &Response) -> Option<Value> {
    let content = &response.content;
    let text = content.text.as_deref().filter(|_| content.encoding.is_none())?;
    serde_json::from_str(text).ok()
}

/// Each value under `at` that could be an ID, with the template that reads it.
fn leaves(value: &Value, at: &str, found: &mut Vec<(String, String)>) {
    let deep = at.matches(['.', '[']).count() > DEPTH;
    match value {
        Value::String(text) if identifier(text) => found.push((text.clone(), at.to_owned())),
        Value::Number(number) if id_key(at) => found.push((number.to_string(), at.to_owned())),
        Value::Object(fields) if !deep => {
            for (key, value) in fields.iter().filter(|(key, _)| plain(key)) {
                leaves(value, &format!("{at}.{key}"), found);
            }
        }
        Value::Array(items) if !deep => {
            for (n, item) in items.iter().take(ITEMS).enumerate() {
                leaves(item, &format!("{at}[{n}]"), found);
            }
        }
        _ => {}
    }
}

/// Whether the last key of `at` names an ID: `id`, `orderId`, `order_id`.
fn id_key(at: &str) -> bool {
    let key = at.rsplit('.').next().unwrap_or_default();
    key.eq_ignore_ascii_case("id") || key.ends_with("Id") || key.to_ascii_lowercase().ends_with("_id")
}

/// Whether `key` can follow a `.` in a template.
fn plain(key: &str) -> bool {
    key.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}
