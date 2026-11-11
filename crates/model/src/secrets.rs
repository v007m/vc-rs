//! `secrets` and `redact`: where `${secret.*}` values come from and what else the redactor masks.

use crate::{CelProgram, HookRef};

/// The provider `secrets.provider` names when the config does not say (D163).
pub const DEFAULT_SECRETS_PROVIDER: &str = "env";

/// The file that declares secret names, relative to the project root (D165).
pub const DEFAULT_SECRETS_SCHEMA: &str = ".env.schema";

/// `secrets`: the provider that resolves `${secret.*}` and the names `.env.schema` declares.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct SecretsConfig {
    /// `provider`: `env` (the default) or `varlock`.
    pub provider: String,
    /// `schema`: the file that declares the secret names.
    pub schema: String,
    /// The names the schema declares, in the order written; empty when the compile had no schema.
    pub declared: Vec<String>,
    /// Those of them marked `# @optional`, which may be left without a value (D280).
    pub optional: Vec<String>,
}

impl Default for SecretsConfig {
    fn default() -> Self {
        Self {
            provider: DEFAULT_SECRETS_PROVIDER.to_owned(),
            schema: DEFAULT_SECRETS_SCHEMA.to_owned(),
            declared: Vec::new(),
            optional: Vec::new(),
        }
    }
}

impl SecretsConfig {
    /// Whether `.env.schema` declares `name`.
    #[must_use]
    pub fn declares(&self, name: &str) -> bool {
        self.declared.iter().any(|declared| declared == name)
    }

    /// Whether `.env.schema` declares `name` without `# @optional`: a run that reads it stops
    /// before any request when it has no value (V5114, D280).
    #[must_use]
    pub fn requires(&self, name: &str) -> bool {
        self.declares(name) && !self.optional.iter().any(|optional| optional == name)
    }
}

/// One user rule under `redact.rules` (D171), applied after the built-in secret masking.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_enums,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub enum RedactRule {
    /// `regex`: every match in any text is masked.
    Regex(String),
    /// `cel`: a JSON field whose `name` and `value` make the expression true is masked.
    Cel(CelProgram),
    /// `function`: a hook function that masks each text and JSON field, within its budget.
    Function(HookRef),
}

/// The names a `.env.schema` declares, required and optional apart (D280).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct SchemaNames {
    /// Every declared name, once, in the order written.
    pub declared: Vec<String>,
    /// Those marked `# @optional`, which may be left without a value.
    pub optional: Vec<String>,
}

impl SchemaNames {
    /// The declared names not marked `# @optional`.
    pub fn required(&self) -> impl Iterator<Item = &str> {
        self.declared.iter().map(String::as_str).filter(|name| !self.optional.iter().any(|optional| optional == name))
    }

    /// Adds `name` unless it is declared already: the first line that declares a name decides
    /// whether it is optional.
    fn add(&mut self, name: &str, optional: bool) {
        if self.declared.iter().any(|seen| seen == name) {
            return;
        }
        self.declared.push(name.to_owned());
        if optional {
            self.optional.push(name.to_owned());
        }
    }
}

/// The names a `.env.schema` declares.
///
/// Each `NAME=` line declares one, with or without a value or an `export ` before it. `#` lines
/// are Varlock's decorators: a name is optional when `@optional` is in the comment lines just
/// above it (no blank line between) or in a `#` comment after its `=`, as Varlock reads it
/// (D280). Blank lines declare nothing.
#[must_use]
pub fn schema_names(text: &str) -> SchemaNames {
    let mut names = SchemaNames::default();
    let mut marked = false;
    for line in text.lines().map(str::trim) {
        if let Some(comment) = line.strip_prefix('#') {
            marked |= optional(comment);
            continue;
        }
        if let Some((name, value)) = item(line) {
            let after = value.split_once('#').is_some_and(|(_, comment)| optional(comment));
            names.add(name, marked || after);
        }
        marked = false;
    }
    names
}

/// The name and value of a `NAME=value` line, `export ` or not; none for any other line.
fn item(line: &str) -> Option<(&str, &str)> {
    let (name, value) = line.strip_prefix("export ").unwrap_or(line).split_once('=')?;
    let name = name.trim();
    is_name(name).then_some((name, value))
}

/// Whether a comment holds the `@optional` decorator (or `@optional=true`).
fn optional(comment: &str) -> bool {
    comment.split_whitespace().any(|word| word == "@optional" || word == "@optional=true")
}

/// `text` as a secret name, upper case with `_` between words.
///
/// `partnerToken token`, `pay api_key` and `shop x-auth-token` give `PARTNER_TOKEN_TOKEN`,
/// `PAY_API_KEY` and `SHOP_X_AUTH_TOKEN`; `APIKey` and `userID` give `API_KEY` and `USER_ID`.
#[must_use]
pub fn secret_name(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    for (at, c) in chars.iter().enumerate() {
        if !c.is_ascii_alphanumeric() {
            if !out.is_empty() && !out.ends_with('_') {
                out.push('_');
            }
            continue;
        }
        if starts_word(&chars, at) {
            out.push('_');
        }
        out.push(c.to_ascii_uppercase());
    }
    out.trim_end_matches('_').to_owned()
}

/// Whether the capital at `at` starts a word: after a lower-case letter or a digit, or as the
/// last capital of a run that a lower-case letter follows (the `K` of `APIKey`).
fn starts_word(chars: &[char], at: usize) -> bool {
    let is = |at: Option<usize>, class: fn(&char) -> bool| at.and_then(|at| chars.get(at)).is_some_and(class);
    let before = at.checked_sub(1);
    let lower_or_digit = |c: &char| c.is_ascii_lowercase() || c.is_ascii_digit();
    let acronym_end = is(before, char::is_ascii_uppercase) && is(Some(at + 1), char::is_ascii_lowercase);
    is(Some(at), char::is_ascii_uppercase) && (is(before, lower_or_digit) || acronym_end)
}

/// An environment variable name: letters, digits and `_`, not starting with a digit.
fn is_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|rest| rest.is_ascii_alphanumeric() || rest == '_')
}

#[cfg(test)]
mod tests {
    use super::{SchemaNames, SecretsConfig, schema_names, secret_name};

    #[test]
    fn the_default_provider_is_env_and_declares_nothing() {
        let secrets = SecretsConfig::default();
        assert_eq!((secrets.provider.as_str(), secrets.schema.as_str()), ("env", ".env.schema"));
        assert!(!secrets.declares("TOKEN"));
        let declared = SecretsConfig { declared: vec!["TOKEN".to_owned()], ..SecretsConfig::default() };
        assert!(declared.declares("TOKEN"));
        assert!(!declared.declares("OTHER"));
    }

    #[test]
    fn a_schema_declares_each_assigned_name_once() {
        let text = "# @defaultSensitive=true\n\n# @required\nAPI_TOKEN=\nexport DB_URL=op(x)\n_X1 = 2\nAPI_TOKEN=again\n1BAD=x\nno equals\nA-B=1\n";
        assert_eq!(schema_names(text).declared, ["API_TOKEN", "DB_URL", "_X1"]);
        assert!(schema_names(text).optional.is_empty(), "`@required` and `@defaultSensitive` mark none optional");
    }

    #[test]
    fn a_name_is_optional_when_optional_is_in_the_comments_just_above_or_after_it() {
        let text = concat!(
            "# @defaultSensitive=true\n",
            "# ---\n\n",
            "# The partner sandbox, which only some tests call.\n",
            "# @optional @sensitive\n",
            "PARTNER_KEY=\n",
            "API_TOKEN=\n",
            "# @optional\n\n",
            "DB_URL=\n",
            "WEBHOOK_URL= # @optional\n",
            "# @optional=true\n",
            "export TRACE_KEY=\n",
            "# @optional=false\n",
            "AUDIT_KEY=\n",
            "API_TOKEN= # @optional\n",
        );
        let names = schema_names(text);
        assert_eq!(names.declared, ["PARTNER_KEY", "API_TOKEN", "DB_URL", "WEBHOOK_URL", "TRACE_KEY", "AUDIT_KEY"]);
        assert_eq!(names.optional, ["PARTNER_KEY", "WEBHOOK_URL", "TRACE_KEY"], "a blank line ends a comment's reach");
        assert_eq!(names.required().collect::<Vec<_>>(), ["API_TOKEN", "DB_URL", "AUDIT_KEY"]);
        assert_eq!(SchemaNames::default().required().count(), 0);
    }

    #[test]
    fn a_secret_is_required_when_declared_and_not_optional() {
        let declared = vec!["TOKEN".to_owned(), "SANDBOX".to_owned()];
        let secrets = SecretsConfig { declared, optional: vec!["SANDBOX".to_owned()], ..SecretsConfig::default() };
        assert!(secrets.requires("TOKEN"));
        assert!(!secrets.requires("SANDBOX") && secrets.declares("SANDBOX"));
        assert!(!secrets.requires("OTHER"));
    }

    #[test]
    fn secret_names_are_upper_snake_case() {
        assert_eq!(secret_name("partnerToken token"), "PARTNER_TOKEN_TOKEN");
        assert_eq!(secret_name("pay refreshToken"), "PAY_REFRESH_TOKEN");
        assert_eq!(secret_name("pay api_key"), "PAY_API_KEY");
        assert_eq!(secret_name("pay apiKey"), "PAY_API_KEY");
        assert_eq!(secret_name("shop x-auth-token"), "SHOP_X_AUTH_TOKEN");
        assert_eq!(secret_name("shop2 card"), "SHOP2_CARD");
        assert_eq!(secret_name("shopAdmin"), "SHOP_ADMIN");
        assert_eq!(secret_name(" a--b "), "A_B");
    }

    #[test]
    fn a_run_of_capitals_is_one_word_up_to_the_capital_before_a_lower_case_letter() {
        assert_eq!(secret_name("APIKey"), "API_KEY");
        assert_eq!(secret_name("userID"), "USER_ID");
        assert_eq!(secret_name("shop HTTPServer"), "SHOP_HTTP_SERVER");
        assert_eq!(secret_name("ID"), "ID");
        assert_eq!(secret_name("v2API"), "V2_API");
        // Known ambiguity: a plural acronym splits before its last capital, as heck does.
        assert_eq!(secret_name("userIDs"), "USER_I_DS");
    }
}
