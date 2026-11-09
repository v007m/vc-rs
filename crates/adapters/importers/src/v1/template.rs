//! v1 placeholders (`{userId}`, `{dataset.names}`, `{lorem_50}`) and generator strings
//! (`[a-z]{8}@shop.example`) as v2 templates (D200).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};
use vc_interfaces::ImportNote;

use crate::naming::camel;

/// The v1 datasets with a Faker call of their own; any other dataset becomes [`OTHER_DATASET`].
const DATASETS: [(&str, &str); 2] = [("names", "person.fullName"), ("countries", "location.country")];
const OTHER_DATASET: &str = "lorem.word";
/// The characters that make a v1 variable's text a generator (a regex) rather than a value.
const REGEX: [char; 12] = ['[', ']', '(', ')', '{', '}', '*', '+', '?', '|', '\\', '^'];

/// Rewrites v1 text as v2 templates, gathering the named generators it needs and its notes.
#[derive(Default)]
pub struct Templates {
    /// The config's `env_vars` that are generators, by their v1 name.
    generated: BTreeSet<String>,
    /// The generators the config declares: ID → `fromRegExp`.
    pub generators: BTreeMap<String, String>,
    /// What was changed on the way, for the report.
    pub notes: Vec<ImportNote>,
}

impl Templates {
    /// Templates over the config's `env_vars`: one holding a regex becomes a named generator.
    pub fn new(env: Option<&Map<String, Value>>) -> Self {
        let mut templates = Self::default();
        for (name, value) in env.into_iter().flatten() {
            if let Some(regex) = value.as_str().filter(|text| generator(text)) {
                templates.generated.insert(name.clone());
                templates.generators.insert(camel(name), regex.to_owned());
            }
        }
        templates
    }

    /// Whether the config's env var `name` became a generator.
    pub fn is_generated(&self, name: &str) -> bool {
        self.generated.contains(name)
    }

    /// `text` with each v1 placeholder written as a v2 template; `at` names it in notes.
    pub fn text(&mut self, text: &str, at: &str) -> String {
        let mut out = String::new();
        let mut rest = text;
        while let Some((before, after)) = rest.split_once('{') {
            out.push_str(before);
            if let Some((name, tail)) = after.split_once('}').filter(|(name, _)| placeholder(name)) {
                out.extend(["${", &self.expression(name, at), "}"]);
                rest = tail;
            } else {
                out.push('{');
                rest = after;
            }
        }
        out + rest
    }

    /// `value` with every string in it rewritten by [`Self::text`].
    pub fn value(&mut self, value: &Value, at: &str) -> Value {
        match value {
            Value::String(text) => Value::String(self.text(text, at)),
            Value::Array(items) => items.iter().map(|item| self.value(item, at)).collect(),
            Value::Object(fields) => fields.iter().map(|(key, field)| (key.clone(), self.value(field, at))).collect(),
            other => other.clone(),
        }
    }

    /// A v1 variable's value: a regex becomes a Faker call, anything else a value.
    pub fn variable(&mut self, value: &Value, at: &str) -> Value {
        let Some(text) = value.as_str().filter(|text| generator(text)) else { return self.value(value, at) };
        if let Some(choices) = text.strip_prefix('[').and_then(|text| text.strip_suffix(']')).filter(|c| choice(c)) {
            let quoted: Vec<String> = choices.split('|').map(|choice| format!("'{choice}'")).collect();
            self.note(at, format!("`{text}` is read as a choice of {}", choices.replace('|', " or ")));
            return Value::String(format!("${{fake.helpers.arrayElement({})}}", quoted.join(", ")));
        }
        Value::String(format!("${{fake.string.fromRegExp('{text}')}}"))
    }

    /// Adds a note about `at`.
    pub fn note(&mut self, at: &str, message: String) {
        self.notes.push(ImportNote { at: at.to_owned(), message });
    }

    /// The v2 expression for the v1 placeholder `{name}`.
    fn expression(&mut self, name: &str, at: &str) -> String {
        if let Some(set) = name.strip_prefix("dataset.") {
            let faker = DATASETS.iter().find(|(of, _)| *of == set).map_or(OTHER_DATASET, |(_, faker)| faker);
            self.note(at, format!("`{{{name}}}` is now `${{fake.{faker}}}`"));
            return format!("fake.{faker}");
        }
        if let Some(count) = name.strip_prefix("lorem_").filter(|count| count.parse::<u16>().is_ok()) {
            self.generators.insert(format!("lorem{count}"), format!("[a-z]{{{count}}}"));
            return format!("fake.lorem{count}");
        }
        if name == "_range_index" {
            return "iteration.index".to_owned();
        }
        if self.generated.contains(name) { format!("fake.{}", camel(name)) } else { format!("var.{}", var_path(name)) }
    }
}

/// `name.0.field` as `name[0].field`, with the var's name in camelCase.
pub fn var_path(name: &str) -> String {
    let mut words = name.split('.');
    let first = camel(words.next().unwrap_or_default());
    words.fold(first, |path, word| {
        if !word.is_empty() && word.bytes().all(|byte| byte.is_ascii_digit()) {
            format!("{path}[{word}]")
        } else {
            format!("{path}.{word}")
        }
    })
}

/// Whether `{name}` is a v1 placeholder rather than a regex count such as `{8}`.
fn placeholder(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

/// Whether v1 reads `text` as a regex to generate from: it has regex characters and no placeholder.
fn generator(text: &str) -> bool {
    let placeholders =
        text.split('{').skip(1).any(|after| after.split_once('}').is_some_and(|(name, _)| placeholder(name)));
    !placeholders && text.contains(REGEX)
}

/// Whether `[text]` is v1's `[A|B]`: a choice of words, not a character class.
fn choice(text: &str) -> bool {
    text.contains('|') && !text.contains(['[', ']', '(', ')']) && text.split('|').all(|word| !word.is_empty())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{Templates, var_path};

    #[test]
    fn placeholders_become_vars_fakes_and_the_iteration() {
        let env = json!({"short_text": "[a-z ]{5,20}", "currency": "EUR"});
        let mut templates = Templates::new(env.as_object());
        let text = "/u/{userId}/{productIds.0}?n={short_text}&c={currency}&i={_range_index}&b={lorem_5}&r=[a-z]{8}";
        assert_eq!(
            templates.text(text, "a"),
            "/u/${var.userId}/${var.productIds[0]}?n=${fake.shortText}&c=${var.currency}\
             &i=${iteration.index}&b=${fake.lorem5}&r=[a-z]{8}"
        );
        assert!(templates.is_generated("short_text") && !templates.is_generated("currency"));
        let generators: Vec<_> = templates.generators.iter().map(|(id, regex)| format!("{id}={regex}")).collect();
        assert_eq!(generators, ["lorem5=[a-z]{5}", "shortText=[a-z ]{5,20}"]);
        assert_eq!(templates.notes, []);
    }

    #[test]
    fn datasets_become_faker_calls_with_a_note() {
        let mut templates = Templates::default();
        assert_eq!(templates.text("{dataset.names}", "a"), "${fake.person.fullName}");
        assert_eq!(templates.text("{dataset.countries}", "a"), "${fake.location.country}");
        assert_eq!(templates.text("{dataset.pokemon}", "b"), "${fake.lorem.word}");
        assert_eq!(templates.notes.len(), 3);
        assert!(templates.notes[2].message.contains("{dataset.pokemon}") && templates.notes[2].at == "b");
    }

    #[test]
    fn a_variable_with_regex_characters_is_a_generator() {
        let mut templates = Templates::default();
        assert_eq!(
            templates.variable(&json!("[a-z]{8}@x.example"), "a"),
            json!("${fake.string.fromRegExp('[a-z]{8}@x.example')}")
        );
        assert_eq!(
            templates.variable(&json!("[ADMIN|MEMBER]"), "a"),
            json!("${fake.helpers.arrayElement('ADMIN', 'MEMBER')}")
        );
        assert_eq!(templates.notes.len(), 1);
        assert_eq!(templates.variable(&json!("a.b@x.example"), "a"), json!("a.b@x.example"));
        assert_eq!(templates.variable(&json!("{dataset.names}"), "a"), json!("${fake.person.fullName}"));
        assert_eq!(templates.variable(&json!({"n": "{count}"}), "a"), json!({"n": "${var.count}"}));
        assert_eq!(templates.variable(&json!(2), "a"), json!(2));
    }

    #[test]
    fn var_paths_index_lists_with_brackets() {
        assert_eq!(var_path("product_ids.0.id"), "productIds[0].id");
        assert_eq!(var_path("userId"), "userId");
    }
}
