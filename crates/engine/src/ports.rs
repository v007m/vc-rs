//! The ports a run needs beyond the builder's: the config layers around the project's files, the
//! environment, the transport, the auth types, the hook runner, the secrets and the random
//! source. Each is the injected one when the builder was given it, else the system, HTTP, auth
//! or secrets adapter.

use std::hash::{BuildHasher as _, Hasher as _, RandomState};
use std::sync::Arc;

use vc_adapter_auth::AuthTypes;
use vc_adapter_hooks::QuickJsHooks;
use vc_adapter_http::{HttpOptions, HttpTransport, ProxyConfig};
use vc_adapter_secrets::SecretProviders;
use vc_adapter_system::{DotEnv, SeededRandom, SetFlags, SystemBrowser, SystemEnvironment, UserConfig, VibVars};
use vc_interfaces::{
    AuthProvider, ConfigEntry, Environment, HookRunner, Random, SecretError, SecretQuery, Secrets, Transport,
};
use vc_model::{Program, SecretsConfig};

use crate::settings::{self, Settings};
use crate::user::UnderProject;
use crate::{Engine, EngineError};

/// The compiled program, the settings of a run and the environment its `${env.X}` reads.
pub struct Layered {
    pub program: Arc<Program>,
    pub notices: Vec<String>,
    pub settings: Settings,
    pub environment: Arc<dyn Environment>,
    /// The `.env` variables the process environment does not set, for a `command:`'s child.
    pub dot_env: Vec<(String, String)>,
    /// Every entry the layers set, lowest first: what `vc config explain` shows.
    pub entries: Vec<ConfigEntry>,
}

impl Engine {
    /// Compiles the project, then reads the settings the user config, `.env`, the `VIB_*`
    /// variables, `flags` and `--set` give (D129), in that order, and the run's environment:
    /// the process environment (or the injected one) over the `.env` file.
    ///
    /// # Errors
    ///
    /// [`EngineError::Invalid`] when the project has errors, and [`EngineError::Setting`] when a
    /// layer cannot be read or sets something unknown.
    pub(crate) fn layered(&self, sets: &[String], flags: &[ConfigEntry]) -> Result<Layered, EngineError> {
        let (program, notices) = self.program()?;
        let dot_env = DotEnv::read(self.root()).map_err(|error| settings::source_error(&error))?;
        let process = self.environment.clone().unwrap_or_else(|| Arc::new(SystemEnvironment::capture()));
        let user = self.user_layer(process.as_ref())?;
        let vib = VibVars::new(Arc::clone(&process));
        let set = SetFlags::new(sets.to_vec());
        let entries = settings::ordered(&[&UnderProject::new(&user, &program), &dot_env, &vib, &set], flags)?;
        let settings = Settings::applied(&entries)?;
        let unset = |(name, _): &&(String, String)| process.var(name).is_none();
        let dot_env: Vec<(String, String)> = dot_env.vars().iter().filter(unset).cloned().collect();
        let environment = SystemEnvironment::snapshot(process.as_ref()).with_fallback(dot_env.clone());
        Ok(Layered { program, notices, settings, environment: Arc::new(environment), dot_env, entries })
    }

    /// The user config: the builder's path, else the default one `environment` names.
    fn user_layer(&self, environment: &dyn Environment) -> Result<UserConfig, EngineError> {
        let path = self.user_config.clone().or_else(|| UserConfig::default_path(environment));
        path.map_or_else(|| Ok(UserConfig::default()), |path| UserConfig::read(&path))
            .map_err(|error| settings::source_error(&error))
    }

    /// The transport requests go through: HTTP, through the proxy the environment names.
    /// Call it from within a Tokio runtime.
    ///
    /// # Errors
    ///
    /// [`EngineError::Internal`] when the HTTP client cannot be built.
    pub(crate) fn transport(&self, environment: &dyn Environment) -> Result<Arc<dyn Transport>, EngineError> {
        if let Some(transport) = &self.transport {
            return Ok(Arc::clone(transport));
        }
        let options = HttpOptions { proxy: ProxyConfig::from_env(|name| environment.var(name)), ..Default::default() };
        let transport =
            HttpTransport::new(&options).map_err(|error| EngineError::Internal { reason: error.to_string() })?;
        Ok(Arc::new(transport))
    }

    /// The auth types a run obtains credentials through (D106): the builder's, else the
    /// standard registry, its token endpoints reached through `transport`, a sign-in opened in
    /// the system browser and refresh tokens kept in the keychain.
    pub(crate) fn auth(&self, transport: Arc<dyn Transport>) -> Arc<dyn AuthProvider> {
        if let Some(auth) = &self.auth {
            return Arc::clone(auth);
        }
        let browser = self.browser.clone().unwrap_or_else(|| Arc::new(SystemBrowser::new(Arc::clone(&self.commands))));
        Arc::new(AuthTypes::standard(transport, browser, self.keychain.clone()))
    }

    /// The secrets `.env.schema` declares, from the provider `secrets.provider` names (D163):
    /// `env` reads them from the run's `environment`, `varlock` runs through the command runner.
    /// Nothing is loaded when nothing is declared.
    ///
    /// # Errors
    ///
    /// [`EngineError::Setting`] when the provider is unknown or cannot load them.
    pub(crate) async fn secrets(
        &self,
        config: &SecretsConfig,
        environment: &Arc<dyn Environment>,
    ) -> Result<Secrets, EngineError> {
        if config.declared.is_empty() {
            return Ok(Secrets::new());
        }
        let commands = Arc::clone(&self.commands);
        let providers = SecretProviders::standard(Arc::clone(environment), commands, self.root().to_path_buf());
        let provider = providers.get(&config.provider).map_err(|error| secret_error(&error))?;
        provider.load(&SecretQuery::new(config.declared.iter().cloned())).await.map_err(|error| secret_error(&error))
    }
}

/// A provider's failure, which never carries a secret value, as the `secrets` setting's.
fn secret_error(error: &SecretError) -> EngineError {
    EngineError::Setting { origin: "secrets".to_owned(), reason: error.to_string(), help: None }
}

impl Engine {
    /// The runner of `program`'s hooks: the injected one, else `QuickJS`; none when it has no
    /// hook files, so a run without hooks starts no hook thread.
    pub(crate) fn hook_runner(&self, program: &Program) -> Option<Arc<dyn HookRunner>> {
        if program.hooks.is_empty() {
            return None;
        }
        Some(self.hooks.clone().unwrap_or_else(|| Arc::new(QuickJsHooks::new())))
    }
}

/// The seed of the run's random source, `seed` or a fresh one, and the source.
pub fn random(seed: Option<u64>) -> (u64, Arc<dyn Random>) {
    let seed = seed.unwrap_or_else(|| RandomState::new().build_hasher().finish());
    (seed, Arc::new(SeededRandom::new(seed)))
}
