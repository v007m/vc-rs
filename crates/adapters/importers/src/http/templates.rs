//! `{{…}}` in a `.http` URL or body as v2 text: a file variable is its value, a dynamic
//! variable with a Faker form is that form, anything else is a var, `{{userId}}` giving
//! `${var.userId}`. A variable named like a credential is never written; a var takes its place.

use indexmap::IndexMap;

use crate::naming::camel;
use crate::secrets::credential;

/// What a template starts and ends with.
const OPEN: &str = "{{";
const CLOSE: &str = "}}";

/// The words of a request variable (`{{login.response.body.$.token}}`) that do not name it.
const REQUEST_WORDS: [&str; 4] = ["response", "request", "body", "headers"];

/// The dynamic variables that are a random UUID, and their v2 form.
const UUIDS: [&str; 3] = ["$guid", "$uuid", "$random.uuid"];
const UUID: &str = "${fake.string.uuid}";
/// The dynamic variable that is a random integer between its two arguments.
const RANDOM_INT: &str = "$randomInt";

/// The file variables with their values resolved, credentials left out.
#[derive(Debug, Default)]
pub struct Templates {
    values: IndexMap<String, String>,
}

impl Templates {
    /// The variables of a file, each value resolved with those defined before it; a credential
    /// is not kept, so its references stay vars.
    pub fn new(variables: &[(String, String)]) -> Self {
        let mut templates = Self::default();
        for (name, value) in variables.iter().filter(|(name, _)| !credential(name)) {
            let value = templates.text(value, &mut Vec::new());
            templates.values.insert(name.clone(), value);
        }
        templates
    }

    /// `text` with each `{{…}}` replaced; what it says goes in `notes`.
    pub fn text(&self, text: &str, notes: &mut Vec<String>) -> String {
        let mut out = String::new();
        let mut rest = text;
        while let Some((before, after)) = rest.split_once(OPEN) {
            let Some((inner, next)) = after.split_once(CLOSE) else { break };
            out.push_str(before);
            out.push_str(&self.replaced(inner.trim(), notes));
            rest = next;
        }
        out + rest
    }

    fn replaced(&self, inner: &str, notes: &mut Vec<String>) -> String {
        if inner.starts_with('$') {
            return dynamic(inner, notes);
        }
        if let Some(value) = self.values.get(inner) {
            return value.clone();
        }
        let words: Vec<&str> = inner.split('.').collect();
        let name = if words.iter().skip(1).any(|word| REQUEST_WORDS.contains(word)) {
            let named = words.iter().filter(|word| !REQUEST_WORDS.contains(word) && **word != "$");
            let name = camel(&named.copied().collect::<Vec<_>>().join(" "));
            notes.push(format!("{{{{{inner}}}}} reads another request's response; it is the var {name}"));
            name
        } else {
            camel(inner)
        };
        if credential(inner) {
            notes.push(format!("{{{{{inner}}}}}: a credential is never written; it is the var {name}"));
        }
        format!("${{var.{name}}}")
    }
}

/// A dynamic variable's v2 form; one without is kept as written, with a note.
fn dynamic(inner: &str, notes: &mut Vec<String>) -> String {
    let (name, arguments) = inner.split_once(' ').unwrap_or((inner, ""));
    let form = if UUIDS.contains(&name) {
        Some(UUID.to_owned())
    } else if name == RANDOM_INT {
        int(arguments.trim())
    } else {
        None
    };
    form.unwrap_or_else(|| {
        notes.push(format!("{{{{{name}}}}} has no v2 form; it is sent as written"));
        format!("{OPEN}{inner}{CLOSE}")
    })
}

/// `{{$randomInt 1 100}}` as `${fake.number.int(1, 100)}`.
fn int(arguments: &str) -> Option<String> {
    let numbers: Vec<i64> = arguments.split_whitespace().map(str::parse).collect::<Result<_, _>>().ok()?;
    let [low, high] = numbers.as_slice() else { return None };
    Some(format!("${{fake.number.int({low}, {high})}}"))
}

#[cfg(test)]
mod tests {
    use super::Templates;

    fn owned(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(name, value)| ((*name).to_owned(), (*value).to_owned())).collect()
    }

    #[test]
    fn variables_are_their_values_and_the_rest_vars() {
        let templates = Templates::new(&owned(&[("host", "a.test"), ("base", "https://{{host}}/v1")]));
        let mut notes = Vec::new();
        assert_eq!(
            templates.text("{{base}}/users/{{ userId }}?x={{host}}", &mut notes),
            "https://a.test/v1/users/${var.userId}?x=a.test"
        );
        assert_eq!(templates.text("{{unclosed", &mut notes), "{{unclosed");
        assert!(notes.is_empty(), "{notes:?}");
    }

    #[test]
    fn credentials_and_responses_are_vars_and_dynamic_variables_fake_data() {
        let templates = Templates::new(&owned(&[("apiToken", "not-a-real-value")]));
        let mut notes = Vec::new();
        let text = "{{apiToken}} {{login.response.body.$.orderId}} {{$guid}} {{$randomInt 1 9}} {{$timestamp}}";
        let expected =
            "${var.apiToken} ${var.loginOrderId} ${fake.string.uuid} ${fake.number.int(1, 9)} {{$timestamp}}";
        assert_eq!(templates.text(text, &mut notes), expected);
        assert_eq!(
            notes,
            [
                "{{apiToken}}: a credential is never written; it is the var apiToken",
                "{{login.response.body.$.orderId}} reads another request's response; it is the var loginOrderId",
                "{{$timestamp}} has no v2 form; it is sent as written",
            ]
        );
    }
}
