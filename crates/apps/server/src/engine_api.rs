//! The engine service (M9b): `vibranium.engine.v1.EngineService`, each RPC a thin adapter over
//! one [`Engine`] facade method. The engine's blocking methods run on Tokio's blocking pool, and
//! an [`EngineError`] becomes a [`ConnectError`] with the engine's own message and help, as the
//! CLI prints them, and the Connect code that fits.

use std::sync::Arc;

use serde::Serialize;
use serde::de::DeserializeOwned;
use vc_contracts::engine::v1 as wire;
use vc_engine::{Engine, EngineError};

use crate::cancel_rpcs::Runs;
use crate::live_rpcs::LiveStreams;
use crate::{
    Code, ConnectError, ConnectService, McpAccess, Sink, authoring_rpcs, cancel_rpcs, client_rpcs, compare_rpcs,
    edit_rpcs, editor_rpcs, file_rpcs, history_rpcs, live_rpcs, load_stream, onboarding_rpcs, plugin_rpcs,
    project_rpcs, run_stream, schedule_rpcs, scheduler, user_settings_rpcs, vcs_rpcs,
};

/// The service's full protobuf name.
const SERVICE: &str = "vibranium.engine.v1.EngineService";

/// The engine `vc ui` opened, and the `--profile` and `--set` layers it was started with.
pub struct EngineApi {
    engine: Arc<Engine>,
    profiles: Vec<String>,
    sets: Vec<String>,
    live: LiveStreams,
    runs: Runs,
}

impl EngineApi {
    /// The API over `engine`. A call that names no profile uses `profiles` (the config's default
    /// ones when empty), and every call's `--set` values come after `sets`. Built on a Tokio
    /// runtime, it keeps its live streams and running runs there (D543, D551); built outside one, it
    /// opens none and cancels none.
    #[must_use]
    pub fn new(engine: Arc<Engine>, profiles: Vec<String>, sets: Vec<String>) -> Self {
        Self { engine, profiles, sets, live: LiveStreams::spawn(), runs: Runs::spawn() }
    }

    /// The Connect service that serves every RPC.
    #[must_use]
    pub fn into_service(self) -> ConnectService {
        Self::serving(Arc::new(self))
    }

    /// The Connect service, with `vc ui`'s scheduler running the config's schedules beside it
    /// until the service is dropped (D703). Built outside a Tokio runtime, nothing is scheduled.
    #[must_use]
    pub fn into_scheduled_service(self) -> ConnectService {
        let api = Arc::new(self);
        scheduler::spawn(Arc::downgrade(&api));
        Self::serving(api)
    }

    fn serving(api: Arc<Self>) -> ConnectService {
        let rpcs = user_settings_rpcs::register(Rpcs { service: ConnectService::new(SERVICE), api });
        let rpcs = onboarding_rpcs::register(plugin_rpcs::register(schedule_rpcs::register(rpcs)));
        let rpcs = project_rpcs::register(client_rpcs::register(edit_rpcs::register(rpcs)));
        let rpcs = run_stream::register(authoring_rpcs::register(history_rpcs::register(cancel_rpcs::register(rpcs))));
        let rpcs = load_stream::register(compare_rpcs::register(vcs_rpcs::register(live_rpcs::register(rpcs))));
        file_rpcs::register(editor_rpcs::register(rpcs)).service
    }

    /// The engine.
    #[cfg(test)]
    pub(crate) fn engine(&self) -> &Engine {
        &self.engine
    }

    /// The open live streams.
    pub(crate) const fn live(&self) -> &LiveStreams {
        &self.live
    }

    /// The running runs a `Cancel` can stop.
    pub(crate) const fn runs(&self) -> &Runs {
        &self.runs
    }

    /// A call's profiles and `--set` values over the server's. The call's own sets come from a
    /// client, so a `reports.dir` among them must name a folder inside the project; the server's
    /// own, from `vc ui --set`, are trusted (D327, D685). Only such a set reads the disk.
    ///
    /// # Errors
    ///
    /// `invalid_argument` (V5132) when one of the call's sets moves the report folder out; the
    /// error `vc run` gives for a set that is not `key=value`.
    pub(crate) fn layers(&self, profiles: Vec<String>, sets: Vec<String>) -> Result<Layers, ConnectError> {
        self.engine.client_sets(&sets).map_err(|error| failed(&error))?;
        let profiles = if profiles.is_empty() { self.profiles.clone() } else { profiles };
        Ok((profiles, self.sets.iter().cloned().chain(sets).collect()))
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

/// A call's profiles and `--set` values, over the server's.
type Layers = (Vec<String>, Vec<String>);

/// The service as its RPCs are added, each handler given the API.
pub struct Rpcs {
    service: ConnectService,
    api: Arc<EngineApi>,
}

impl Rpcs {
    /// Serves `method` as a unary call to `handler`.
    pub fn unary<Req, Resp, F, Fut>(self, method: &'static str, handler: F) -> Self
    where
        Req: prost::Message + Default + DeserializeOwned + 'static,
        Resp: prost::Message + Serialize + 'static,
        F: Fn(Arc<EngineApi>, Req) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Resp, ConnectError>> + Send + 'static,
    {
        let api = Arc::clone(&self.api);
        Self { service: self.service.unary(method, move |request| handler(Arc::clone(&api), request)), api: self.api }
    }

    /// Serves `method` as a server stream from `handler`.
    pub fn server_stream<Req, Resp, F, Fut>(self, method: &'static str, handler: F) -> Self
    where
        Req: prost::Message + Default + DeserializeOwned + 'static,
        Resp: prost::Message + Serialize + 'static,
        F: Fn(Arc<EngineApi>, Req, Sink<Resp>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), ConnectError>> + Send + 'static,
    {
        let api = Arc::clone(&self.api);
        let service = self.service.server_stream(method, move |request, sink| handler(Arc::clone(&api), request, sink));
        Self { service, api: self.api }
    }
}

/// `service` and, when it is the engine service and the server serves `/mcp`, its `Connection`
/// RPC: where an MCP client connects and the agent token, for the Connect panel. Only the
/// browser's cookie reaches it, as it does every `/api` route (D239).
pub fn connected(service: ConnectService, mcp: Option<&McpAccess>) -> ConnectService {
    let Some(access) = mcp.filter(|_| service.name() == SERVICE) else { return service };
    let reply = wire::ConnectionResponse { mcp_url: access.url().to_owned(), agent_token: access.token().to_owned() };
    service
        .unary("Connection", move |_: wire::ConnectionRequest| std::future::ready(Ok::<_, ConnectError>(reply.clone())))
}

/// `error` as the Connect error the web UI shows: the engine's message, then its help.
pub fn failed(error: &EngineError) -> ConnectError {
    ConnectError::new(code(error), message(error))
}

/// `error` with its help, as a client shows it.
pub fn message(error: &EngineError) -> String {
    error.help().map_or_else(|| error.to_string(), |help| format!("{error}\nhelp: {help}"))
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
