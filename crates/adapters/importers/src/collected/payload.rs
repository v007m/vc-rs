//! A request's body, read by the shared body rules once its variables are template text: text
//! by its content type (the header's, else its own), a form as fields, GraphQL as JSON with its
//! query and variables. A body that is not imported is left out with a note.

use serde_json::{Map, Value, json};

use super::request::Payload;
use super::vars::Filling;
use crate::body::{Body, Scrub, Sent, field, read};

/// The body `payload` sends, under the content type its header names.
pub fn body(payload: Option<&Payload>, content_type: Option<&str>, filling: &mut Filling<'_, '_>) -> Body {
    match payload {
        None => Body::default(),
        Some(Payload::Raw { text, content_type: own }) => raw(text, content_type.unwrap_or(*own), filling),
        Some(Payload::Form(fields)) => form(fields, content_type, filling),
        Some(Payload::GraphQl { query, variables }) => graphql(query, variables, filling),
        Some(Payload::Left(note)) => {
            filling.secrets.note((*note).to_owned());
            Body::default()
        }
    }
}

fn raw(text: &str, content_type: &str, filling: &mut Filling<'_, '_>) -> Body {
    if text.trim().is_empty() {
        return Body::default();
    }
    let data = if content_type.contains("json") { filling.fill_json(text) } else { filling.fill(text) };
    read(&Sent { data: &data, content_type }, &filling.vars, filling.secrets)
}

fn form(fields: &[(String, String)], content_type: Option<&str>, filling: &mut Filling<'_, '_>) -> Body {
    let mut filled = Map::new();
    for (name, value) in fields {
        let value = filling.fill(value);
        let (name, value) = field(name.clone(), json!(value), filling.secrets);
        filled.insert(name, value);
    }
    let content_type = content_type.unwrap_or("application/x-www-form-urlencoded");
    Body { value: Some(Value::Object(filled)), content_type: Some(content_type.to_owned()), ..Body::default() }
}

fn graphql(query: &str, variables: &str, filling: &mut Filling<'_, '_>) -> Body {
    let query = filling.fill(query);
    let variables = filling.fill_json(variables);
    let variables = serde_json::from_str::<Value>(&variables).unwrap_or_else(|_| json!({}));
    let data = json!({ "query": query, "variables": variables }).to_string();
    read(&Sent { data: &data, content_type: "application/json" }, &filling.vars, filling.secrets)
}
