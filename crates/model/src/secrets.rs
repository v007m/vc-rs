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
}

impl Default for SecretsConfig {
    fn default() -> Self {
        Self {
            provider: DEFAULT_SECRETS_PROVIDER.to_owned(),
            schema: DEFAULT_SECRETS_SCHEMA.to_owned(),
            declared: Vec::new(),
        }
    }
}

impl SecretsConfig {
    /// Whether `.env.schema` declares `name`.
    #[must_use]
    pub fn declares(&self, name: &str) -> bool {
        self.declared.iter().any(|declared| declared == name)
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

/// The names a `.env.schema` declares: each `NAME=` line, with or without a value or an
/// `export ` before it; `#` lines (Varlock's decorators) and blank lines declare nothing.
#[must_use]
pub fn schema_names(text: &str) -> Vec<String> {
    let lines = text.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#'));
    let names = lines.filter_map(|line| line.strip_prefix("export ").unwrap_or(line).split_once('='));
    let names = names.map(|(name, _)| name.trim()).filter(|name| is_name(name));
    let mut found: Vec<String> = Vec::new();
    for name in names {
        if !found.iter().any(|seen| seen == name) {
            found.push(name.to_owned());
        }
    }
    found
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
    use super::{SecretsConfig, schema_names, secret_name};

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
        assert_eq!(schema_names(text), ["API_TOKEN", "DB_URL", "_X1"]);
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
