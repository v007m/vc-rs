//! `secrets.provider: varlock` (D163): one `varlock load --format json` per run, told the run's
//! profile as `VC_PROFILE` (D306).

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::OnceCell;
use vc_interfaces::{
    CommandError, CommandOutput, CommandRunner, CommandSpec, Secret, SecretError, SecretProvider, SecretQuery, Secrets,
};
use vc_model::PROFILE_VARIABLE;

/// The provider's name, in `secrets.provider` and in messages.
const VARLOCK: &str = "varlock";

/// The arguments Varlock is run with: resolve every item of `.env.schema` and print JSON.
const ARGS: [&str; 3] = ["load", "--format", "json"];

/// Resolves the declared names through Varlock.
///
/// Varlock reads `.env.schema` in the project root and asks its own providers (1Password,
/// Vault and the rest). It runs once; every later load in the run reads what that one gave.
/// Its environment is vc's own plus `VC_PROFILE` when the run has a profile, and nothing else.
#[derive(Clone)]
pub struct VarlockSecrets {
    runner: Arc<dyn CommandRunner>,
    root: PathBuf,
    profile: Option<String>,
    loaded: Arc<OnceCell<Result<Secrets, SecretError>>>,
}

impl VarlockSecrets {
    /// A provider running Varlock with `runner` in the project `root`.
    #[must_use]
    pub fn new(runner: Arc<dyn CommandRunner>, root: impl Into<PathBuf>) -> Self {
        Self { runner, root: root.into(), profile: None, loaded: Arc::default() }
    }

    /// This provider, giving Varlock `profile` as `VC_PROFILE`: the run's last profile (D306).
    /// `None` leaves it unset; vc never passes `--env`.
    #[must_use]
    pub fn for_profile(mut self, profile: Option<String>) -> Self {
        self.profile = profile;
        self
    }

    async fn all(&self) -> Result<Secrets, SecretError> {
        let mut spec = CommandSpec::new(VARLOCK, ARGS).in_dir(&self.root);
        spec.env.extend(self.profile.iter().map(|profile| (PROFILE_VARIABLE.to_owned(), profile.clone())));
        let output = self.runner.run(&spec).await.map_err(unavailable)?;
        parse(&output)
    }
}

impl fmt::Debug for VarlockSecrets {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VarlockSecrets")
            .field("root", &self.root)
            .field("profile", &self.profile)
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl SecretProvider for VarlockSecrets {
    async fn load(&self, query: &SecretQuery) -> Result<Secrets, SecretError> {
        let all = self.loaded.get_or_init(|| self.all()).await.as_ref().map_err(Clone::clone)?;
        let mut secrets = Secrets::new();
        for name in &query.names {
            if let Some(secret) = all.get(name) {
                secrets.insert(name.clone(), secret.clone());
            }
        }
        Ok(secrets)
    }
}

/// Why Varlock could not run, without its output, which may hold values.
fn unavailable(error: CommandError) -> SecretError {
    let reason = match error {
        CommandError::NotFound { .. } => "`varlock` is not on PATH".to_owned(),
        other => other.to_string(),
    };
    SecretError::Unavailable { provider: VARLOCK.to_owned(), reason }
}

/// The values in Varlock's JSON object: a string as it is, a number or boolean as its text;
/// `null` (an optional item with no value) is left out.
fn parse(output: &CommandOutput) -> Result<Secrets, SecretError> {
    if !output.success() {
        let status = output.status.map_or_else(|| "a signal".to_owned(), |code| format!("status {code}"));
        let reason = format!("`varlock load` exited with {status}; run it in the project root to see why");
        return Err(SecretError::Unavailable { provider: VARLOCK.to_owned(), reason });
    }
    let malformed = |reason: &str| SecretError::Malformed { provider: VARLOCK.to_owned(), reason: reason.to_owned() };
    if output.truncated {
        return Err(malformed("stdout went past the output cap"));
    }
    let parsed: Value = serde_json::from_slice(&output.stdout).map_err(|_| malformed("stdout is not JSON"))?;
    let Value::Object(items) = parsed else { return Err(malformed("stdout is not a JSON object")) };
    let mut secrets = Secrets::new();
    for (name, value) in items {
        let text = match value {
            Value::String(text) => text,
            Value::Null => continue,
            Value::Bool(_) | Value::Number(_) => value.to_string(),
            Value::Array(_) | Value::Object(_) => return Err(malformed("an item is not text")),
        };
        secrets.insert(name, Secret::new(text));
    }
    Ok(secrets)
}

#[cfg(test)]
#[path = "varlock_tests.rs"]
mod tests;
