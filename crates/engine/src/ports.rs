//! The ports a run needs beyond the builder's: the config layers around the project's files, the
//! environment, the transport, the auth types, the hook runner, the secrets and the random
//! source. Each is the injected one when the builder was given it, else the system, HTTP, auth
//! or secrets adapter.

use std::collections::BTreeSet;
use std::hash::{BuildHasher as _, Hasher as _, RandomState};
use std::sync::Arc;

use vc_adapter_auth::{AuthTypes, PluginAuth};
use vc_adapter_hooks::QuickJsHooks;
use vc_adapter_http::{HttpOptions, HttpTransport, ProxyConfig};
use vc_adapter_notifiers::NotifierRegistry;
use vc_adapter_secrets::SecretProviders;
use vc_adapter_system::{DotEnv, SeededRandom, SetFlags, SystemBrowser, SystemEnvironment, UserConfig, VibVars};
use vc_diagnostics::Code;
use vc_interfaces::{
    AuthProvider, ConfigEntry, Environment, HookRunner, NotifierFactory, PluginKind, Random, SecretError, SecretQuery,
    Secrets, Transport,
};
use vc_model::{Plan, Program, SecretsConfig};

use crate::plan::Prepared;
use crate::plugin_hosts::Hosts;
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
    pub(crate) fn user_layer(&self, environment: &dyn Environment) -> Result<UserConfig, EngineError> {
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
    /// the system browser, refresh tokens kept in the keychain and `plugin` calling the auth
    /// plugins of `plugins`, or failing with the reason one cannot be used.
    pub(crate) fn auth(&self, transport: Arc<dyn Transport>, plugins: &Hosts) -> Arc<dyn AuthProvider> {
        if let Some(auth) = &self.auth {
            return Arc::clone(auth);
        }
        let browser = self.browser.clone().unwrap_or_else(|| Arc::new(SystemBrowser::new(Arc::clone(&self.commands))));
        let plugin = plugins.of(PluginKind::Auth).fold(PluginAuth::new(), PluginAuth::with);
        let plugin = plugins.unusable().fold(plugin, PluginAuth::unusable);
        Arc::new(AuthTypes::standard(transport, browser, self.keychain.clone()).with("plugin", Arc::new(plugin)))
    }

    /// The alert channels (D106): the builder's, else the standard ones, posting through
    /// `transport`.
    pub(crate) fn notifiers(&self, transport: Arc<dyn Transport>) -> Arc<dyn NotifierFactory> {
        self.notifiers.clone().unwrap_or_else(|| Arc::new(NotifierRegistry::standard(transport)))
    }

    /// The alert channel kinds a run can send to, for `vc check`: the builder's, else the
    /// standard ones.
    pub(crate) fn channels(&self) -> Vec<String> {
        self.notifiers.as_ref().map_or_else(NotifierRegistry::standard_channels, |notifiers| notifiers.channels())
    }

    /// The secrets `.env.schema` declares, each of those `read` names with a value unless it is
    /// `# @optional`: a run, call or load that reads one without stops here, before any request
    /// (V5114). A secret nothing it runs reads may be unset (D280).
    ///
    /// # Errors
    ///
    /// [`EngineError::Setting`] when the provider fails or a required name it reads has no value.
    pub(crate) async fn secrets(&self, prepared: &Prepared, read: &BTreeSet<String>) -> Result<Secrets, EngineError> {
        let config = &prepared.program.config.secrets;
        let loaded = self.loaded_secrets(config, &prepared.environment).await?;
        let wanted = |name: &&String| read.contains(*name) && config.requires(name);
        let blank = |name: &&String| loaded.get(name).is_none_or(|secret| secret.expose().trim().is_empty());
        let unset: Vec<&str> = config.declared.iter().filter(wanted).filter(blank).map(String::as_str).collect();
        if unset.is_empty() { Ok(loaded) } else { Err(unset_error(&unset)) }
    }

    /// The secrets `.env.schema` declares, from the provider `secrets.provider` names (D163):
    /// `env` reads them from the run's `environment`, `varlock` runs through the command runner.
    /// Nothing is loaded when nothing is declared, and a name without a value is left out.
    ///
    /// # Errors
    ///
    /// [`EngineError::Setting`] when the provider is unknown or cannot load them.
    pub(crate) async fn loaded_secrets(
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

/// The secrets a run of `plan` reads (D280): what it runs, the api cleanups the cache's sweep
/// can run when the program caches, and the alert channels its end sends to.
pub fn run_reads(program: &Program, plan: &Plan) -> BTreeSet<String> {
    let mut read = vc_planner::secrets(program, plan);
    if crate::body_files::caches(program) {
        read.extend(vc_planner::secrets_from(program, &crate::body_files::cleanups(program)));
    }
    read.extend(vc_planner::alert_secrets(program));
    read
}

/// The declared secrets in `unset` have no value: named, never with any other secret.
fn unset_error(unset: &[&str]) -> EngineError {
    let names = unset.iter().map(|name| format!("`{name}`")).collect::<Vec<_>>().join(", ");
    let (noun, verb) = if unset.len() == 1 { ("secret", "has") } else { ("secrets", "have") };
    EngineError::Setting {
        origin: "secrets".to_owned(),
        reason: format!("{}: the declared {noun} {names} {verb} no value", Code::V5114),
        help: Some("set it in the environment or the project's `.env` (or in the `secrets.provider`), or remove it from `.env.schema`".to_owned()),
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

/// The largest integer a JSON number holds exactly in JavaScript (2^53 - 1), the cap of a fresh
/// seed, so a seed read back from `--json` or MCP replays the same data.
pub const MAX_FRESH_SEED: u64 = (1 << 53) - 1;

/// The seed of the run's random source, `seed` or a fresh one up to [`MAX_FRESH_SEED`], and the
/// source.
pub fn random(seed: Option<u64>) -> (u64, Arc<dyn Random>) {
    let seed = seed.unwrap_or_else(|| RandomState::new().build_hasher().finish() & MAX_FRESH_SEED);
    (seed, Arc::new(SeededRandom::new(seed)))
}

/// The random source of one test's example rows, from the run's `seed` and the test's full
/// `id`: the rows a seed gives a test do not depend on what else the run renders, or in what order.
pub fn random_for(seed: u64, id: &str) -> Arc<dyn Random> {
    Arc::new(SeededRandom::new(seed ^ fnv1a(id)))
}

/// The 64-bit FNV-1a hash of `text`, the same on every build and platform (unlike `DefaultHasher`).
fn fnv1a(text: &str) -> u64 {
    let step = |hash: u64, byte: u8| (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
    text.bytes().fold(0xcbf2_9ce4_8422_2325, step)
}

#[cfg(test)]
#[path = "ports_tests.rs"]
mod tests;
