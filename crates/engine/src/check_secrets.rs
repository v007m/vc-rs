//! The web UI's secret check (LLD `CheckSecrets(profile)`): whether each secret `.env.schema`
//! declares resolves under a profile, through the `secrets.provider` a run uses. A value never
//! leaves: only a name and whether it resolves, and a provider's failure, which holds none.

use vc_interfaces::{ConfigEntry, Secrets};

use crate::plan::{self, profile_flag};
use crate::{Engine, EngineError, ExplainRequest};

/// Whether one declared secret resolves.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SecretCheck {
    /// Its name in `.env.schema`.
    pub name: String,
    /// Whether the provider gave it a value that is not blank, as a run needs (V5114).
    pub resolved: bool,
}

/// What [`Engine::check_secrets`] found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct SecretsChecked {
    /// The `secrets.provider` that loaded them: `env` or `varlock`.
    pub provider: String,
    /// Each declared secret, in the order `.env.schema` writes them.
    pub secrets: Vec<SecretCheck>,
    /// Why the provider could not load them, holding no value; then none resolves.
    pub error: Option<String>,
    /// Notices from the compile.
    pub notices: Vec<String>,
}

impl Engine {
    /// Whether each declared secret resolves under `request`'s profiles and `--set`s.
    ///
    /// # Errors
    ///
    /// [`EngineError::Invalid`] when the project has errors, and [`EngineError::Setting`] for a
    /// bad setting or an unknown profile. A provider that cannot load is no error: it is
    /// [`SecretsChecked::error`].
    pub async fn check_secrets(&self, request: &ExplainRequest) -> Result<SecretsChecked, EngineError> {
        let flags: Vec<ConfigEntry> = profile_flag(&request.profiles).into_iter().collect();
        let layered = self.layered(&request.sets, &flags)?;
        plan::profiles(layered.program.as_ref(), layered.settings.profiles.as_ref())?;
        let config = &layered.program.config.secrets;
        let (loaded, error) = match self.loaded_secrets(config, &layered.environment).await {
            Ok(loaded) => (loaded, None),
            Err(error) => (Secrets::new(), Some(error.to_string())),
        };
        let resolved = |name: &str| loaded.get(name).is_some_and(|secret| !secret.expose().trim().is_empty());
        let secrets =
            config.declared.iter().map(|name| SecretCheck { name: name.clone(), resolved: resolved(name) }).collect();
        Ok(SecretsChecked { provider: config.provider.clone(), secrets, error, notices: layered.notices })
    }
}

#[cfg(test)]
#[path = "check_secrets_tests.rs"]
mod tests;
