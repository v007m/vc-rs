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

/// An environment variable name: letters, digits and `_`, not starting with a digit.
fn is_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|rest| rest.is_ascii_alphanumeric() || rest == '_')
}

#[cfg(test)]
mod tests {
    use super::{SecretsConfig, schema_names};

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
}
