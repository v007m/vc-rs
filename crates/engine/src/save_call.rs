//! `vc call --save-as-test`: the call just made, written as a test that expects the status it
//! got and the body fields named with `--expect-field`, and no inferred schema (D176).

use serde_json::Value;
use vc_model::Segment;
use vc_parser::write_text;

use crate::authoring::refused;
use crate::create::{Added, DOCS, NewItem, called, checked, sentence};
use crate::{Engine, EngineError};

/// A call made, to save as a test.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built in other crates, shared across the workspace; every user is in this repo (D160)"
)]
pub struct CallAsTest {
    /// The test's full ID: `users.crud#listsUsers`.
    pub id: String,
    /// The api called: `users#list`.
    pub api: String,
    /// The `--var name=value`s given on the command line, as written.
    pub vars: Vec<String>,
    /// The response as `vc call` saw it (`status`, `headers`, `body`): [`crate::CallReport`]'s.
    pub response: Option<Value>,
    /// The fields to expect as they came back: `body.word`, `body.items[0].id`.
    pub fields: Vec<String>,
}

/// A `body.match` key and the value it expects, written as YAML.
type Expected = (String, String);

impl Engine {
    /// The change that appends `call` as a test to the test file its ID derives from, creating
    /// that file when there is none.
    ///
    /// # Errors
    ///
    /// [`EngineError::Edit`] as [`Engine::new_test`], and for a field that is not a `body.`
    /// path, is not in the body, or holds a list or an object.
    pub fn save_call_as_test(&self, call: &CallAsTest) -> Result<Added, EngineError> {
        let named = checked(&call.id)?;
        let project = self.authoring()?;
        let api = called(&project, &call.api)?;
        let response = call.response.as_ref();
        let status =
            response.and_then(|response| response.get("status")?.as_u64()).and_then(|code| u16::try_from(code).ok());
        let status =
            status.ok_or_else(|| refused(format!("{api} got no response to save"), "call it again once it answers"))?;
        let body = response.and_then(|response| response.get("body"));
        let fields = call.fields.iter().map(|field| expected(field, body)).collect::<Result<Vec<_>, _>>()?;
        let yaml = test_yaml(call, named.item.unwrap_or_default(), (status, &fields));
        let comment = format!("# {} · saved from vc call {api} · keys: {DOCS}#tests", call.id);
        self.added(&project, NewItem::test(named, yaml, comment))
    }
}

/// The test `id` for `call`, unindented: its vars, the status and the `fields` it expects.
fn test_yaml(call: &CallAsTest, id: &str, (status, fields): (u16, &[Expected])) -> String {
    let mut lines =
        vec![format!("id: {id}"), format!("name: {}", write_text(&sentence(id), false)), format!("call: {}", call.api)];
    if !call.vars.is_empty() {
        lines.push("vars:".to_owned());
        let vars = call.vars.iter().filter_map(|var| var.split_once('='));
        lines.extend(vars.map(|(name, value)| format!("  {}: {}", write_text(name, false), write_text(value, false))));
    }
    lines.extend(["expect:".to_owned(), format!("  status: {status}")]);
    if !fields.is_empty() {
        lines.extend(["  body:".to_owned(), "    match:".to_owned()]);
        lines.extend(fields.iter().map(|(path, value)| format!("      {}: {value}", write_text(path, false))));
    }
    lines.join("\n")
}

/// The `body.match` key of `field` and the value `body` has there, written as YAML.
fn expected(field: &str, body: Option<&Value>) -> Result<Expected, EngineError> {
    let help = "name a field in the JSON body: `--expect-field body.id` or `body.items[0].id`";
    let path = field.strip_prefix("body.").ok_or_else(|| refused(format!("`{field}` is not a body field"), help))?;
    let segments = segments(path).ok_or_else(|| refused(format!("`{field}` is not a body path"), help))?;
    let body = body.ok_or_else(|| refused(format!("`{field}`: the response body is not JSON"), help))?;
    let value = vc_template::select("body", body, &segments).map_err(|error| refused(error.to_string(), help))?;
    let written = match value {
        Value::String(text) => write_text(&text.replace("${", "$${"), false),
        Value::Number(_) | Value::Bool(_) | Value::Null => value.to_string(),
        Value::Array(_) | Value::Object(_) => {
            return Err(refused(format!("`{field}` holds a list or an object"), "name a field inside it"));
        }
    };
    Ok((path.to_owned(), written))
}

/// `path` as dotted keys with `[n]` indexes: `items[0].id`; `None` for anything else.
fn segments(path: &str) -> Option<Vec<Segment>> {
    let mut segments = Vec::new();
    for part in path.split('.') {
        let (key, indexes) = part.split_once('[').unwrap_or((part, ""));
        if key.is_empty() || !key.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
            return None;
        }
        segments.push(Segment::Key(key.to_owned()));
        for index in indexes.split('[').filter(|index| !index.is_empty()) {
            segments.push(Segment::Index(index.strip_suffix(']')?.parse().ok()?));
        }
    }
    Some(segments)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vc_model::Segment;

    use super::{expected, segments};

    #[test]
    fn a_body_path_is_keys_and_indexes() {
        let key = |text: &str| Segment::Key(text.to_owned());
        assert_eq!(segments("word"), Some(vec![key("word")]));
        assert_eq!(segments("items[0].id"), Some(vec![key("items"), Segment::Index(0), key("id")]));
        assert_eq!(segments("items[-1]"), Some(vec![key("items"), Segment::Index(-1)]));
        assert_eq!(segments("items[x]"), None);
        assert_eq!(segments("a..b"), None);
    }

    #[test]
    fn a_field_is_expected_as_it_came_back() {
        let body = json!({"word": "hi", "n": 7, "code": "200", "t": "${var.x}", "list": [1]});
        let field = |path: &str| expected(path, Some(&body)).ok();
        assert_eq!(field("body.word"), Some(("word".to_owned(), "hi".to_owned())));
        assert_eq!(field("body.n"), Some(("n".to_owned(), "7".to_owned())));
        assert_eq!(field("body.t").map(|(_, value)| value.contains("$${")), Some(true));
        assert!(field("body.code").is_some_and(|(_, value)| value != "200"), "text that reads as a number stays text");
        assert_eq!(field("body.list"), None);
        assert_eq!(field("body.missing"), None);
        assert_eq!(field("status"), None);
    }
}
