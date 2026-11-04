//! The ports a run needs beyond the builder's: the config layers above the project's files, the
//! environment, the transport and the random source. Each is the injected one when the builder
//! was given it, else the system or HTTP adapter.

use std::hash::{BuildHasher as _, Hasher as _, RandomState};
use std::sync::Arc;

use vc_adapter_http::{HttpOptions, HttpTransport, ProxyConfig};
use vc_adapter_system::{DotEnv, SeededRandom, SetFlags, SystemEnvironment, VibVars};
use vc_interfaces::{ConfigEntry, Environment, Random, Transport};

use crate::settings::{self, Settings};
use crate::{Engine, EngineError};

/// The settings of a run and the environment its `${env.X}` reads.
pub struct Layered {
    pub settings: Settings,
    pub environment: Arc<dyn Environment>,
}

impl Engine {
    /// The settings `.env`, the `VIB_*` variables, `flags` and `--set` give (D129), in that
    /// order, and the run's environment: the process environment over the `.env` file.
    ///
    /// # Errors
    ///
    /// [`EngineError::Setting`] when a layer cannot be read or sets something unknown.
    pub(crate) fn layered(&self, sets: &[String], flags: &[ConfigEntry]) -> Result<Layered, EngineError> {
        let dot_env = match &self.environment {
            Some(_) => DotEnv::default(),
            None => DotEnv::read(self.root()).map_err(|error| settings::source_error(&error))?,
        };
        let process = self.environment.clone().unwrap_or_else(|| Arc::new(SystemEnvironment::capture()));
        let vib = VibVars::new(Arc::clone(&process));
        let set = SetFlags::new(sets.to_vec());
        let settings = Settings::resolve(&[&dot_env, &vib, &set], flags)?;
        let environment: Arc<dyn Environment> = match &self.environment {
            Some(injected) => Arc::clone(injected),
            None => Arc::new(SystemEnvironment::capture().with_fallback(dot_env.vars().to_vec())),
        };
        Ok(Layered { settings, environment })
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
}

/// The seed of the run's random source, `seed` or a fresh one, and the source.
pub fn random(seed: Option<u64>) -> (u64, Arc<dyn Random>) {
    let seed = seed.unwrap_or_else(|| RandomState::new().build_hasher().finish());
    (seed, Arc::new(SeededRandom::new(seed)))
}
