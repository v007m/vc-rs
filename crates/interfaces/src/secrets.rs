//! Where `${secret.*}` values come from (D163, D165).

use std::collections::BTreeMap;
use std::fmt;

use async_trait::async_trait;
use vc_diagnostics::Code;
use zeroize::Zeroizing;

/// What `Debug` shows in place of a secret value.
const HIDDEN: &str = "••••";

/// One secret value. `Debug` never shows it, and the memory is wiped when it is dropped.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(Zeroizing<String>);

impl Secret {
    /// Holds `value`.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(Zeroizing::new(value.into()))
    }

    /// The value itself, for the one place that renders it into a request or masks it.
    #[must_use]
    pub fn expose(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(HIDDEN)
    }
}

/// The secrets a provider resolved, by name. `Debug` shows the names only.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Secrets {
    by_name: BTreeMap<String, Secret>,
}

impl Secrets {
    /// No secrets.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `name`, replacing any earlier value.
    pub fn insert(&mut self, name: impl Into<String>, secret: Secret) {
        self.by_name.insert(name.into(), secret);
    }

    /// The secret called `name`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Secret> {
        self.by_name.get(name)
    }

    /// Every name and its secret, by name.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Secret)> {
        self.by_name.iter().map(|(name, secret)| (name.as_str(), secret))
    }

    /// How many secrets there are.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    /// Whether there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }
}

impl fmt::Debug for Secrets {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.by_name.keys()).finish()
    }
}

/// The names to resolve: the ones `.env.schema` declares.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct SecretQuery {
    /// The declared names, in the order written.
    pub names: Vec<String>,
}

impl SecretQuery {
    /// A query for `names`.
    #[must_use]
    pub fn new(names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self { names: names.into_iter().map(Into::into).collect() }
    }
}

/// Resolves the declared secrets once per run (D163): `env` (the default) or `varlock`.
///
/// A name the provider cannot resolve is left out of the [`Secrets`]; the executor reports it
/// where a template reads it.
#[async_trait]
pub trait SecretProvider: Send + Sync {
    /// The values for `query`'s names.
    ///
    /// # Errors
    ///
    /// [`SecretError`] when the provider cannot run or its output is not what it promises.
    async fn load(&self, query: &SecretQuery) -> Result<Secrets, SecretError>;
}

/// Why the secrets could not be loaded. No variant carries a secret value.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SecretError {
    /// The provider's tool could not run, such as `varlock` not on `PATH`.
    #[error("the {provider} secrets provider is unavailable: {reason}")]
    Unavailable {
        /// The provider, such as `varlock`.
        provider: String,
        /// What went wrong, without any value.
        reason: String,
    },
    /// The provider ran but its output could not be read.
    #[error("the {provider} secrets provider gave output it cannot read: {reason}")]
    Malformed {
        /// The provider.
        provider: String,
        /// What was wrong with it, without any value.
        reason: String,
    },
    /// `secrets.provider` names no provider.
    #[error("no secrets provider is called `{provider}`")]
    Unknown {
        /// The name as configured.
        provider: String,
    },
}

impl SecretError {
    /// The diagnostic code: secrets are credentials, so every cause is V5114.
    #[must_use]
    pub const fn code(&self) -> Code {
        Code::V5114
    }

    /// What the user can do about it.
    #[must_use]
    pub const fn help(&self) -> &'static str {
        match self {
            Self::Unavailable { .. } => "install the provider's tool and check it runs, or set `secrets.provider: env`",
            Self::Malformed { .. } => "run the provider by hand and check it prints the values as JSON",
            Self::Unknown { .. } => "set `secrets.provider` to `env` or `varlock`",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Secret, SecretError, SecretQuery, Secrets};

    #[test]
    fn debug_never_shows_a_value() {
        let mut secrets = Secrets::new();
        secrets.insert("API_TOKEN", Secret::new("s3cr3t-value"));
        assert_eq!(format!("{secrets:?}"), "{\"API_TOKEN\"}");
        assert_eq!(format!("{:?}", Secret::new("s3cr3t-value")), "••••");
        assert_eq!(secrets.get("API_TOKEN").map(Secret::expose), Some("s3cr3t-value"));
        assert_eq!((secrets.len(), secrets.is_empty(), Secrets::new().is_empty()), (1, false, true));
        assert_eq!(secrets.iter().map(|(name, _)| name).collect::<Vec<_>>(), ["API_TOKEN"]);
    }

    #[test]
    fn every_cause_is_v5114_with_its_own_help() {
        let causes = [
            SecretError::Unavailable { provider: "varlock".into(), reason: "not found".into() },
            SecretError::Malformed { provider: "varlock".into(), reason: "not JSON".into() },
            SecretError::Unknown { provider: "vault".into() },
        ];
        assert!(causes.iter().all(|cause| cause.code() == vc_diagnostics::Code::V5114));
        let mut helps: Vec<_> = causes.iter().map(SecretError::help).collect();
        helps.dedup();
        assert_eq!(helps.len(), 3);
        assert_eq!(causes[2].to_string(), "no secrets provider is called `vault`");
        assert_eq!(SecretQuery::new(["A", "B"]).names, ["A", "B"]);
    }
}
