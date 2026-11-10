//! The engine service (M9b): `vibranium.engine.v1.EngineService`, each RPC a thin adapter over
//! one [`Engine`] facade method. The engine's blocking methods run on Tokio's blocking pool, and
//! an [`EngineError`] becomes a [`ConnectError`] with the engine's own message and help, as the
//! CLI prints them, and the Connect code that fits.

use std::sync::Arc;

use vc_engine::{Engine, EngineError};

use crate::{Code, ConnectError, ConnectService, Sink, project_rpcs};

/// The service's full protobuf name.
const SERVICE: &str = "vibranium.engine.v1.EngineService";

/// The engine `vc ui` opened, and the `--profile` and `--set` layers it was started with.
pub struct EngineApi {
    engine: Arc<Engine>,
    profiles: Vec<String>,
    sets: Vec<String>,
}

impl EngineApi {
    /// The API over `engine`. A call that names no profile uses `profiles` (the config's default
    /// ones when empty), and every call's `--set` values come after `sets`.
    #[must_use]
    pub const fn new(engine: Arc<Engine>, profiles: Vec<String>, sets: Vec<String>) -> Self {
        Self { engine, profiles, sets }
    }

    /// The Connect service that serves every RPC.
    #[must_use]
    pub fn into_service(self) -> ConnectService {
        let rpcs = Rpcs { service: ConnectService::new(SERVICE), api: Arc::new(self) };
        let rpcs = project_rpcs::register(rpcs);
        rpcs.service
    }

    /// The engine.
    pub(crate) fn engine(&self) -> &Engine {
        &self.engine
    }

    /// A call's profiles and `--set` values over the server's.
    pub(crate) fn layers(&self, profiles: Vec<String>, sets: Vec<String>) -> (Vec<String>, Vec<String>) {
        let profiles = if profiles.is_empty() { self.profiles.clone() } else { profiles };
        (profiles, self.sets.iter().cloned().chain(sets).collect())
    }

    /// Runs `work` with the engine on the blocking pool.
    pub(crate) async fn blocking<T, W>(self: &Arc<Self>, work: W) -> Result<T, ConnectError>
    where
        T: Send + 'static,
        W: FnOnce(&Engine) -> Result<T, EngineError> + Send + 'static,
    {
        let api = Arc::clone(self);
        let joined = tokio::task::spawn_blocking(move || work(&api.engine)).await;
        joined
            .map_err(|_| ConnectError::new(Code::Internal, "the engine stopped while answering"))?
            .map_err(|error| failed(&error))
    }
}

/// The service as its RPCs are added, each handler given the API.
pub struct Rpcs {
    service: ConnectService,
    api: Arc<EngineApi>,
}

impl Rpcs {
    /// Serves `method` as a unary call to `handler`.
    pub fn unary<Req, Resp, F, Fut>(self, method: &'static str, handler: F) -> Self
    where
        Req: prost::Message + Default + 'static,
        Resp: prost::Message + 'static,
        F: Fn(Arc<EngineApi>, Req) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Resp, ConnectError>> + Send + 'static,
    {
        let api = Arc::clone(&self.api);
        Self { service: self.service.unary(method, move |request| handler(Arc::clone(&api), request)), api: self.api }
    }

    /// Serves `method` as a server stream from `handler`.
    pub fn server_stream<Req, Resp, F, Fut>(self, method: &'static str, handler: F) -> Self
    where
        Req: prost::Message + Default + 'static,
        Resp: prost::Message + 'static,
        F: Fn(Arc<EngineApi>, Req, Sink<Resp>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), ConnectError>> + Send + 'static,
    {
        let api = Arc::clone(&self.api);
        let service = self.service.server_stream(method, move |request, sink| handler(Arc::clone(&api), request, sink));
        Self { service, api: self.api }
    }
}

/// `error` as the Connect error the web UI shows: the engine's message, then its help.
pub fn failed(error: &EngineError) -> ConnectError {
    let message = error.help().map_or_else(|| error.to_string(), |help| format!("{error}\nhelp: {help}"));
    ConnectError::new(code(error), message)
}

/// The Connect code for `error`.
const fn code(error: &EngineError) -> Code {
    match error {
        EngineError::Stale { .. } => Code::Aborted,
        EngineError::NoFilesMatch { .. } => Code::NotFound,
        EngineError::Edit { .. }
        | EngineError::Selection { .. }
        | EngineError::Setting { .. }
        | EngineError::Reporter { .. } => Code::InvalidArgument,
        EngineError::Refused { .. } => Code::PermissionDenied,
        EngineError::NotBuilt { .. } => Code::Unimplemented,
        EngineError::Internal { .. } => Code::Internal,
        _ => Code::FailedPrecondition,
    }
}

#[cfg(test)]
#[path = "engine_api_tests.rs"]
mod tests;
