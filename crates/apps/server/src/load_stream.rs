//! The engine service's `RunLoad` stream (D678): runs a load scenario and streams a tick as
//! each second closes, then the report. Closing the stream interrupts the run once, as one
//! Ctrl-C does, so its `afterAll`, `afterRun` and cleanups still run; a run with a `handle` can
//! also be stopped or abandoned by `Cancel`. The run is driven from the blocking pool, as
//! `Run`'s is, and the relay to the client stays on the async side.

use std::sync::Arc;

use tokio::runtime::Handle;
use tokio::sync::mpsc::{self, Receiver};
use vc_contracts::engine::v1 as wire;
use vc_contracts::engine::v1::run_load_response::Kind;
use vc_engine::{Event, Interrupter, LoadRequest, LoadWatch};

use crate::engine_api::{EngineApi, Rpcs};
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
    api.runs().open(&request.handle, &interrupter).await?;
    let request = load_request(&api, request);
    let watch = LoadWatch { interrupts, ticks: Some(ticks) };
    let running = api.blocking(move |engine| Handle::current().block_on(engine.load(&request, watch)));
    let (ran, relayed) = tokio::join!(running, relay(heard, &sink, &interrupter));
    let report = ran?;
    relayed?;
    sink.send(&wire::RunLoadResponse { kind: Some(Kind::Report(load_report(report))) }).await
}

/// The engine's request for `request`, over the server's profiles and `--set` values.
fn load_request(api: &EngineApi, request: wire::RunLoadRequest) -> LoadRequest {
    let (profiles, sets) = api.layers(request.profiles, request.sets);
    LoadRequest { scenario: request.scenario, profiles, sets, seed: request.seed }
}

/// Sends each tick to `sink` until the run has returned and dropped its sender. When the
/// client goes, or a tick cannot be sent, the run is interrupted once and the rest is drained
/// unsent; the error the first unsent tick met, if any.
async fn relay(
    mut heard: Receiver<Event>,
    sink: &Sink<wire::RunLoadResponse>,
    interrupter: &Interrupter,
) -> Result<(), ConnectError> {
    let (mut open, mut unsent) = (true, Ok(()));
    loop {
        tokio::select! {
            tick = heard.recv() => {
                let Some(tick) = tick else { return unsent };
                let response = wire::RunLoadResponse { kind: Some(Kind::Tick(load_tick(tick))) };
                if open && let Err(error) = sink.send(&response).await {
                    (open, unsent) = (false, Err(error));
                    interrupter.interrupt();
                }
            }
            () = sink.closed(), if open => {
                open = false;
                interrupter.interrupt();
            }
        }
    }
}

#[cfg(test)]
#[path = "load_stream_tests.rs"]
mod tests;
