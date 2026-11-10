//! The engine service's `Run` stream (C6c): runs a selection and streams its events, then its
//! report. Closing the stream interrupts the run once, as one Ctrl-C does, so its cleanups still
//! run; the feed is drained, unsent, until the run returns. A run that cannot start ends the
//! stream with the engine's error.

use std::sync::Arc;

use tokio::sync::mpsc::{self, Receiver};
use vc_contracts::engine::v1 as wire;
use vc_engine::{Interrupter, PlanRequest, RunFeed, RunRequest, RunWatch};

use crate::engine_api::{EngineApi, Rpcs, failed};
use crate::project_rpcs::select;
use crate::wire_events::WireFeed;
use crate::{ConnectError, Sink};

/// Feed items that wait for a slow client before the engine counts the rest as missed.
const FEED: usize = 256;

/// Serves `Run`.
pub fn register(rpcs: Rpcs) -> Rpcs {
    rpcs.server_stream("Run", run)
}

/// Runs what `request` selects, streaming its feed to `sink` while it runs.
async fn run(
    api: Arc<EngineApi>,
    request: wire::RunRequest,
    sink: Sink<wire::RunResponse>,
) -> Result<(), ConnectError> {
    let (sender, feed) = mpsc::channel(FEED);
    let request = run_request(&api, request, RunWatch::new(sender));
    let (interrupter, interrupts) = vc_engine::interrupts();
    let running = async move { api.engine().run(&request, interrupts).await };
    let (ran, ()) = tokio::join!(running, relay(feed, &sink, &interrupter));
    ran.map_err(|error| failed(&error))?;
    Ok(())
}

/// The engine's request for `request`, over the server's profiles and `--set` values, with
/// `watch` following it.
fn run_request(api: &EngineApi, request: wire::RunRequest, watch: RunWatch) -> RunRequest {
    let (profiles, sets) = api.layers(request.profiles, request.sets);
    let plan = PlanRequest { select: select(request.selection), repeat: request.repeat, profiles, sets };
    RunRequest {
        plan,
        seed: request.seed,
        bail: request.bail,
        concurrency: request.concurrency,
        failed: request.failed,
        no_cache: request.no_cache,
        watch: Some(watch),
        ..RunRequest::default()
    }
}

/// Sends the feed's events to `sink` until every sender has gone, which is once the run has
/// returned. When the client goes, the run is interrupted once and the rest is drained unsent.
async fn relay(mut feed: Receiver<RunFeed>, sink: &Sink<wire::RunResponse>, interrupter: &Interrupter) {
    let (mut wire, mut open) = (WireFeed::default(), true);
    loop {
        tokio::select! {
            item = feed.recv() => {
                let Some(item) = item else { return };
                if open && !send(sink, wire.events(item)).await {
                    open = false;
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

/// Sends `events` in order; false once the client has gone.
async fn send(sink: &Sink<wire::RunResponse>, events: Vec<wire::RunEvent>) -> bool {
    for event in events {
        if !sink.send(&wire::RunResponse { event: Some(event) }).await {
            return false;
        }
    }
    true
}

#[cfg(test)]
#[path = "run_stream_tests.rs"]
mod tests;
