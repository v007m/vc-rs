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

/// The names a `.env.schema` gives a default, with it: each `NAME=value` line whose value is
/// not blank, quotes taken off and a ` #` comment after it left out.
#[must_use]
pub fn schema_defaults(text: &str) -> Vec<(String, String)> {
    let lines = text.lines().map(str::trim).filter(|line| !line.starts_with('#'));
    let pairs = lines.filter_map(|line| line.strip_prefix("export ").unwrap_or(line).split_once('='));
    let pairs = pairs.map(|(name, value)| (name.trim(), unquoted(value.split(" #").next().unwrap_or_default().trim())));
    pairs
        .filter(|(name, value)| is_name(name) && !value.is_empty())
        .map(|(n, v)| (n.to_owned(), v.to_owned()))
        .collect()
}

/// `value` without the quotes around it, if it has a matching pair.
fn unquoted(value: &str) -> &str {
    ["\"", "'"].iter().find_map(|quote| value.strip_prefix(quote)?.strip_suffix(quote)).unwrap_or(value)
}

/// `text` as a secret name, upper case with `_` between words.
///
/// `partnerToken token`, `pay api_key` and `shop x-auth-token` give `PARTNER_TOKEN_TOKEN`,
/// `PAY_API_KEY` and `SHOP_X_AUTH_TOKEN`. A capital after a lower-case letter or a digit starts a word.
#[must_use]
pub fn secret_name(text: &str) -> String {
    let mut out = String::new();
    let mut lower_before = false;
    for c in text.chars() {
        if !c.is_ascii_alphanumeric() {
            lower_before = false;
            if !out.is_empty() && !out.ends_with('_') {
                out.push('_');
            }
            continue;
        }
        if c.is_ascii_uppercase() && lower_before {
            out.push('_');
        }
        lower_before = c.is_ascii_lowercase() || c.is_ascii_digit();
        out.push(c.to_ascii_uppercase());
    }
    out.trim_end_matches('_').to_owned()
}

/// An environment variable name: letters, digits and `_`, not starting with a digit.
fn is_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|rest| rest.is_ascii_alphanumeric() || rest == '_')
}

#[cfg(test)]
mod tests {
    use super::{SecretsConfig, schema_defaults, schema_names, secret_name};

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
    fn a_schema_default_is_a_value_after_the_name() {
        let text =
            "# @required\nAPI_TOKEN=\nREGION=eu # the closest\nexport TENANT=\"acme\"\nQUIET='' \n# OFF=1\n1BAD=x\n";
        let found = schema_defaults(text);
        let pairs: Vec<(&str, &str)> = found.iter().map(|(name, value)| (name.as_str(), value.as_str())).collect();
        assert_eq!(pairs, [("REGION", "eu"), ("TENANT", "acme")]);
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
}
