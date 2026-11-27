//! The engine service's `RunLoad` stream (D678): runs a load scenario and streams a tick as
//! each second closes, then the report. Closing the stream interrupts the run once, as one
//! Ctrl-C does, so its `afterAll`, `afterRun` and cleanups still run; a run with a `handle` can
//! also be stopped or abandoned by `Cancel`. The run is driven from the blocking pool, as
//! `Run`'s is, and the relay to the client stays on the async side.

use std::sync::Arc;

use tokio::runtime::Handle;
use tokio::sync::mpsc;
use vc_contracts::engine::v1 as wire;
use vc_contracts::engine::v1::run_load_response::Kind;
use vc_engine::{Event, LoadRequest, LoadWatch};

use crate::engine_api::{EngineApi, Rpcs};
use crate::run_stream::relay;
use crate::wire_events::load_tick;
use crate::wire_load::load_report;
use crate::{ConnectError, Sink};

/// Ticks that wait for a slow client; the engine drops the ones after, never the history's.
const TICKS: usize = 16;

/// Serves `RunLoad`.
pub fn register(rpcs: Rpcs) -> Rpcs {
    rpcs.server_stream("RunLoad", run_load)
}

/// Runs the scenario `request` names, streaming its ticks to `sink`, then its report.
async fn run_load(
    api: Arc<EngineApi>,
    request: wire::RunLoadRequest,
    sink: Sink<wire::RunLoadResponse>,
) -> Result<(), ConnectError> {
    let (interrupter, interrupts) = vc_engine::interrupts();
    let (ticks, heard) = mpsc::channel(TICKS);
    let handle = request.handle.clone();
    let request = load_request(&api, request)?;
    api.runs().open(&handle, &interrupter).await?;
    let watch = LoadWatch { interrupts, ticks: Some(ticks) };
    let running = api.blocking(move |engine| Handle::current().block_on(engine.load(&request, watch)));
    let (ran, relayed) = tokio::join!(running, relay(heard, (&sink, &interrupter), tick_response));
    let report = ran?;
    relayed?;
    sink.send(&wire::RunLoadResponse { kind: Some(Kind::Report(load_report(report))) }).await
}

/// The engine's request for `request`, over the server's profiles and `--set` values; refused
/// before the run holds its handle when a set moves the report folder out (D685).
fn load_request(api: &EngineApi, request: wire::RunLoadRequest) -> Result<LoadRequest, ConnectError> {
    let (profiles, sets) = api.layers(request.profiles, request.sets)?;
    Ok(LoadRequest { scenario: request.scenario, profiles, sets, seed: request.seed })
}

/// The message that streams one tick.
fn tick_response(tick: Event) -> Vec<wire::RunLoadResponse> {
    vec![wire::RunLoadResponse { kind: Some(Kind::Tick(load_tick(tick))) }]
}

#[cfg(test)]
#[path = "load_stream_tests.rs"]
mod tests;
