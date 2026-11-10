//! A collection's `{{name}}` variables as template text. A variable named like a credential,
//! marked secret or read from the environment (`{{process.env.NAME}}`) is a `${secret.NAME}`
//! reference; a dynamic one (`{{$guid}}`) is the generator that makes the
//! same kind of value; any other is `${var.name}`, with the collection's value as the api's
//! default. A value is never written when it is a credential's or too random to be anything
//! but a secret. No type here derives `Debug`, since the values may be secrets.

use indexmap::IndexMap;
use serde_json::{Value, json};

use crate::body::Scrub;
use crate::naming::camel;
use crate::secret::Secrets;
use crate::secrets::credential;
use crate::url::split_origin;

/// What a variable read from the environment starts with.
const ENV: &str = "process.env.";

/// How many variables deep a base URL is looked up.
const DEPTH: usize = 8;

/// What the origin of a collection whose base URL has no value is.
pub const UNKNOWN_ORIGIN: &str = "http://localhost";

/// The dynamic variables a collection can use, by the generator that makes the same kind of value.
const DYNAMIC: [(&str, &str); 16] = [
    ("$guid", "string.uuid"),
    ("$randomUUID", "string.uuid"),
    ("$randomEmail", "internet.email"),
    ("$randomExampleEmail", "internet.email"),
    ("$randomUserName", "internet.userName"),
    ("$randomFullName", "person.fullName"),
    ("$randomFirstName", "person.firstName"),
    ("$randomJobTitle", "person.jobTitle"),
    ("$randomPhoneNumber", "phone.number"),
    ("$randomStreetAddress", "location.streetAddress"),
    ("$randomCity", "location.city"),
    ("$randomCountry", "location.country"),
    ("$randomCountryCode", "location.countryCode"),
    ("$randomInt", "number.int(0, 1000)"),
    ("$randomPrice", "commerce.price"),
    ("$randomLoremSentence", "lorem.sentence"),
];

/// The collection's variables with a value, the vars its test scripts save and the variables
/// it marks secret.
pub struct Values {
    given: IndexMap<String, String>,
    saved: Vec<String>,
    secret: Vec<String>,
}

impl Values {
    /// The variables with a value; `saved` are the vars a script sets, which need none.
    pub const fn new(values: IndexMap<String, String>, saved: Vec<String>) -> Self {
        Self { given: values, saved, secret: Vec::new() }
    }

    /// The same, with `names` the variables marked secret, whose values are never written.
    pub fn with_secrets(self, names: Vec<String>) -> Self {
        Self { secret: names, ..self }
    }

    /// The name the secret `name` is referred to by (`API_KEY` for `process.env.API_KEY`), when
    /// it is one.
    fn secret<'n>(&self, name: &'n str) -> Option<&'n str> {
        if let Some(env) = name.strip_prefix(ENV) {
            return Some(env);
        }
        (credential(name) || self.secret.iter().any(|secret| secret == name)).then_some(name)
    }

    /// The origin `raw` names, with the variables it starts with looked up, and the path and
    /// query after it; no origin when one of them has no value.
    pub fn origin(&self, raw: &str) -> (Option<String>, String) {
        let mut text = raw.trim().to_owned();
        for _ in 0..DEPTH {
            let Some(value) = leading(&text).and_then(|(name, rest)| Some(format!("{}{rest}", self.get(name)?))) else {
                break;
            };
            text = value;
        }
        if let Some((_, rest)) = leading(&text) {
            return (None, rest.to_owned());
        }
        if !text.contains("://") && !text.starts_with('/') {
            text = format!("http://{text}");
        }
        let Some((origin, rest)) = split_origin(&text) else { return (None, text) };
        let origin = self.expanded(origin);
        if origin.contains("{{") { (None, rest.to_owned()) } else { (Some(origin), rest.to_owned()) }
    }

    /// The value of `name`; none for a secret, whose value is never written.
    fn get(&self, name: &str) -> Option<&str> {
        self.given.get(name).map(String::as_str).filter(|_| self.secret(name).is_none())
    }

    /// `text` with each variable that has a value replaced by it.
    fn expanded(&self, text: &str) -> String {
        let mut out = String::new();
        for (literal, token) in tokens(text) {
            out.push_str(literal);
            if let Some(name) = token {
                out.push_str(&self.get(name).map_or_else(|| format!("{{{{{name}}}}}"), str::to_owned));
            }
        }
        out
    }
}

/// One request's text being filled: the vars its variables became, with their defaults.
pub struct Filling<'f, 's> {
    pub values: &'f Values,
    pub secrets: &'f mut Secrets<'s>,
    pub vars: IndexMap<String, Value>,
}

impl Filling<'_, '_> {
    /// `text` with each variable as template text.
    pub fn fill(&mut self, text: &str) -> String {
        let mut out = String::new();
        for (literal, token) in tokens(text) {
            out.push_str(literal);
            if let Some(name) = token {
                out.push_str(&self.token(name));
            }
        }
        out
    }

    /// JSON `text` with each variable as template text, quoted where it stood for a whole value
    /// (`{"quantity": {{qty}}}`), so the text stays JSON.
    pub fn fill_json(&mut self, text: &str) -> String {
        let mut out = String::new();
        let mut quoted = false;
        for (literal, token) in tokens(text) {
            quoted = in_string(literal, quoted);
            out.push_str(literal);
            if let Some(name) = token {
                let filled = self.token(name);
                if quoted {
                    out.push_str(&filled);
                } else {
                    out.push('"');
                    out.push_str(&filled);
                    out.push('"');
                }
            }
        }
        out
    }

    /// The template text for the variable `name`.
    fn token(&mut self, name: &str) -> String {
        if name.starts_with('$') {
            return self.dynamic(name);
        }
        if let Some(secret) = self.values.secret(name) {
            return self.secrets.reference(&format!("var {secret}"));
        }
        let var = camel(name);
        if var.is_empty() {
            return format!("{{{{{name}}}}}");
        }
        if !self.vars.contains_key(&var) {
            let default = self.default(name, &var);
            self.vars.insert(var.clone(), json!(default));
        }
        format!("${{var.{var}}}")
    }

    /// The default of `var`, for the variable `name`: its value, scrubbed, else empty.
    fn default(&mut self, name: &str, var: &str) -> String {
        if let Some(value) = self.values.get(name) {
            return self.secrets.text(&format!("var {name}"), value);
        }
        if !self.values.saved.iter().any(|saved| saved == var) {
            self.secrets.note(format!("`{{{{{name}}}}}` has no value in the collection, so `{var}` is empty"));
        }
        String::new()
    }

    fn dynamic(&mut self, name: &str) -> String {
        if let Some((_, generator)) = DYNAMIC.iter().find(|(dynamic, _)| *dynamic == name) {
            return format!("${{fake.{generator}}}");
        }
        self.secrets.note(format!("`{{{{{name}}}}}` has no generator, so it is left as written"));
        format!("{{{{{name}}}}}")
    }
}

/// The text before each `{{name}}` in `text`, with the name; the last part has none.
fn tokens(text: &str) -> Vec<(&str, Option<&str>)> {
    let mut parts = Vec::new();
    let mut rest = text;
    while let Some((before, after)) = rest.split_once("{{")
        && let Some((name, after)) = after.split_once("}}")
    {
        parts.push((before, Some(name.trim())));
        rest = after;
    }
    parts.push((rest, None));
    parts
}

/// The name and the rest of `text` when it starts with a variable.
fn leading(text: &str) -> Option<(&str, &str)> {
    let (name, rest) = text.strip_prefix("{{")?.split_once("}}")?;
    Some((name.trim(), rest))
}

/// Whether the end of `literal` is inside a JSON string, when its start was `quoted`.
fn in_string(literal: &str, quoted: bool) -> bool {
    let mut quoted = quoted;
    let mut escaped = false;
    for c in literal.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            _ => {}
        }
    }
    quoted
}
