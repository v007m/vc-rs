//! The engine service's `Run` stream (C6c): runs a selection and streams its events, then its
//! report. Closing the stream interrupts the run once, as one Ctrl-C does, so its cleanups still
//! run; the feed is drained, unsent, until the run returns. A run with a `handle` can also be
//! stopped or abandoned by `Cancel` while it runs (D551). A run that cannot start ends the
//! stream with the engine's error. The run, which compiles and plans the project before it
//! starts, is driven from the blocking pool, so the compile never holds an async worker; the
//! relay to the client stays on the async side.

use std::path::PathBuf;
use std::sync::Arc;

use prost::Message;
use serde::Serialize;
use tokio::runtime::Handle;
use tokio::sync::mpsc::{self, Receiver};
use vc_contracts::engine::v1 as wire;
use vc_engine::{Interrupter, PlanRequest, ReportFlags, RunRequest, RunWatch};

use crate::engine_api::{EngineApi, Rpcs};
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
    mut request: wire::RunRequest,
    sink: Sink<wire::RunResponse>,
) -> Result<(), ConnectError> {
    let (interrupter, interrupts) = vc_engine::interrupts();
    let (sender, feed) = mpsc::channel(FEED);
    let handle = request.handle.clone();
    let out = client_out(&api, std::mem::take(&mut request.out)).await?;
    let request = run_request(&api, (request, out), RunWatch::new(sender));
    api.runs().open(&handle, &interrupter).await?;
    let running = api.blocking(move |engine| Handle::current().block_on(engine.run(&request, interrupts)));
    let mut wire = WireFeed::default();
    let responses =
        |item| wire.events(item).into_iter().map(|event| wire::RunResponse { event: Some(event) }).collect();
    let (ran, relayed) = tokio::join!(running, relay(feed, (&sink, &interrupter), responses));
    ran?;
    relayed
}

/// The report folder `out` names, `None` when empty; `invalid_argument` (V5132) when it is not
/// a folder inside the project, checked before the run holds its handle (D671).
async fn client_out(api: &Arc<EngineApi>, out: String) -> Result<Option<PathBuf>, ConnectError> {
    if out.is_empty() {
        return Ok(None);
    }
    api.blocking(move |engine| engine.client_report_folder(&out)).await.map(Some)
}

/// The engine's request for `request`, writing its report files to `out`, over the server's
/// profiles and `--set` values, with `watch` following it.
fn run_request(api: &EngineApi, (request, out): (wire::RunRequest, Option<PathBuf>), watch: RunWatch) -> RunRequest {
    let files = ReportFlags { formats: request.reports, out, single_file: request.single_file };
    let (profiles, sets) = api.layers(request.profiles, request.sets);
    let plan = PlanRequest { select: select(request.selection), repeat: request.repeat, profiles, sets };
    RunRequest {
        plan,
        seed: request.seed,
        bail: request.bail,
        concurrency: request.concurrency,
        failed: request.failed,
        no_cache: request.no_cache,
        unrecorded: request.record == Some(false),
        strict: request.strict,
        files,
        watch: Some(watch),
        ..RunRequest::default()
    }
}

/// Sends the messages `messages` makes of each feed item to `sink`, in order, until every
/// sender has gone, which is once the run has returned. When the client goes, or a message
/// cannot be sent, the run is interrupted once and the rest is drained unsent; the error the
/// first unsent message met, if any. `Run` and `RunLoad` share it.
pub async fn relay<T, R: Message + Serialize>(
    mut feed: Receiver<T>,
    (sink, interrupter): (&Sink<R>, &Interrupter),
    mut messages: impl FnMut(T) -> Vec<R>,
) -> Result<(), ConnectError> {
    let (mut open, mut unsent) = (true, Ok(()));
    loop {
        tokio::select! {
            item = feed.recv() => {
                let Some(item) = item else { return unsent };
                if open && let Err(error) = send(sink, messages(item)).await {
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

/// Sends `messages` in order, stopping at the first that cannot be sent.
async fn send<R: Message + Serialize>(sink: &Sink<R>, messages: Vec<R>) -> Result<(), ConnectError> {
    for message in messages {
        sink.send(&message).await?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "run_stream_tests.rs"]
mod tests;
