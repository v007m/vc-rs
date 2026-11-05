//! `secrets.provider: env`, the default (D163).

use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use vc_interfaces::{Environment, Secret, SecretError, SecretProvider, SecretQuery, Secrets};

/// Resolves each declared name from the run's environment: the process environment, with the
/// project's `.env` filling the names it does not set.
#[derive(Clone)]
pub struct EnvSecrets {
    environment: Arc<dyn Environment>,
}

impl EnvSecrets {
    /// A provider reading `environment`.
    #[must_use]
    pub fn new(environment: Arc<dyn Environment>) -> Self {
        Self { environment }
    }
}

impl fmt::Debug for EnvSecrets {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EnvSecrets").finish_non_exhaustive()
    }
}

#[async_trait]
impl SecretProvider for EnvSecrets {
    async fn load(&self, query: &SecretQuery) -> Result<Secrets, SecretError> {
        let mut secrets = Secrets::new();
        for name in &query.names {
            if let Some(value) = self.environment.var(name) {
                secrets.insert(name.clone(), Secret::new(value));
            }
        }
        Ok(secrets)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use vc_interfaces::SecretProvider;
    use vc_testkit::{MemoryEnvironment, secret_provider_suite};

    use super::EnvSecrets;

    #[tokio::test]
    async fn env_secrets_meet_the_secret_provider_contract() {
        secret_provider_suite(&mut |values| {
            Box::new(EnvSecrets::new(Arc::new(MemoryEnvironment::new(values)))) as Box<dyn SecretProvider>
        })
        .await;
        let shown = format!("{:?}", EnvSecrets::new(Arc::new(MemoryEnvironment::new(&[("A", "s3cr3t")]))));
        assert!(!shown.contains("s3cr3t"));
    }
}
