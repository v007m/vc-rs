//! The secret providers by `secrets.provider` name (D163).

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use vc_interfaces::{CommandRunner, Environment, SecretError, SecretProvider};

use crate::{EnvSecrets, VarlockSecrets};

/// Every secret provider, by the name `secrets.provider` gives it.
#[derive(Clone, Default)]
pub struct SecretProviders {
    by_name: BTreeMap<String, Arc<dyn SecretProvider>>,
}

impl SecretProviders {
    /// No providers.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `env` over `environment`, and `varlock` run with `runner` in the project `root`.
    #[must_use]
    pub fn standard(environment: Arc<dyn Environment>, runner: Arc<dyn CommandRunner>, root: PathBuf) -> Self {
        Self::new()
            .with(vc_model::DEFAULT_SECRETS_PROVIDER, Arc::new(EnvSecrets::new(environment)))
            .with("varlock", Arc::new(VarlockSecrets::new(runner, root)))
    }

    /// These providers, with `name` added or replaced.
    #[must_use]
    pub fn with(mut self, name: &str, provider: Arc<dyn SecretProvider>) -> Self {
        self.by_name.insert(name.to_owned(), provider);
        self
    }

    /// The provider called `name`.
    ///
    /// # Errors
    ///
    /// [`SecretError::Unknown`] when there is none.
    pub fn get(&self, name: &str) -> Result<Arc<dyn SecretProvider>, SecretError> {
        self.by_name.get(name).cloned().ok_or_else(|| SecretError::Unknown { provider: name.to_owned() })
    }
}

impl fmt::Debug for SecretProviders {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.by_name.keys()).finish()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use vc_interfaces::{Secret, SecretError, SecretQuery};
    use vc_testkit::{FakeCommands, MemoryEnvironment};

    use super::SecretProviders;

    #[tokio::test]
    async fn the_standard_providers_are_env_and_varlock() {
        let environment = Arc::new(MemoryEnvironment::new(&[("TOKEN", "from-env")]));
        let runner = Arc::new(FakeCommands::printing("varlock", r#"{"TOKEN":"from-varlock"}"#));
        let providers = SecretProviders::standard(environment, runner, "/p".into());
        assert_eq!(format!("{providers:?}"), r#"{"env", "varlock"}"#);
        let query = SecretQuery::new(["TOKEN"]);
        for (name, value) in [("env", "from-env"), ("varlock", "from-varlock")] {
            let secrets = providers.get(name).unwrap().load(&query).await.unwrap();
            assert_eq!(secrets.get("TOKEN").map(Secret::expose), Some(value));
        }
        assert_eq!(providers.get("vault").err(), Some(SecretError::Unknown { provider: "vault".into() }));
    }
}
