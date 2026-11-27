//! Onboarding (D716, D717): `Init`, the wizard's `vc init`, and `Demo`, the demo project, each a
//! preview of the new project's files with its digest, written only with that digest, in the
//! folder `vc ui` was started in and at paths the engine fixes. `Demo` also says whether
//! `vc demo`'s service answers; the server never starts a process for a client.

use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use vc_contracts::engine::v1 as wire;
use vc_engine::{InitChange, InitChoices};

use crate::ConnectError;
use crate::authoring_rpcs::response;
use crate::engine_api::{EngineApi, Rpcs};

/// Where the sample calls `vc demo`'s service.
const DEMO_URL: &str = "http://localhost:7421";

/// The port of [`DEMO_URL`].
const DEMO_PORT: u16 = 7421;

/// The command that starts the demo service in the background.
const DEMO_START: &str = "vc demo start --detach";

/// How long the demo service has to answer.
const PROBE: Duration = Duration::from_millis(500);

/// Adds the onboarding RPCs.
pub fn register(rpcs: Rpcs) -> Rpcs {
    rpcs.unary("Init", init).unary("Demo", demo)
}

/// The wizard's project: a preview, or it written with the preview's digest.
pub async fn init(api: Arc<EngineApi>, request: wire::InitRequest) -> Result<wire::InitResponse, ConnectError> {
    let expected = request.write.then(|| request.expected_digest.unwrap_or_default());
    let choices = InitChoices {
        name: request.name,
        service: request.service,
        base_url: request.base_url,
        profile: request.profile,
        sample: request.sample,
    };
    let change = api.blocking(move |engine| engine.client_init(&choices, expected.as_deref())).await?;
    Ok(init_response(change))
}

/// The demo project, previewed or written, and whether the demo service answers.
pub async fn demo(api: Arc<EngineApi>, request: wire::DemoRequest) -> Result<wire::DemoResponse, ConnectError> {
    let expected = request.write.then(|| request.expected_digest.unwrap_or_default());
    let change = api.blocking(move |engine| engine.client_demo(expected.as_deref())).await?;
    let running = tokio::time::timeout(PROBE, answers(DEMO_PORT)).await.unwrap_or(false);
    Ok(wire::DemoResponse {
        project: Some(init_response(change)),
        running,
        url: DEMO_URL.to_owned(),
        start: DEMO_START.to_owned(),
    })
}

fn init_response(change: InitChange) -> wire::InitResponse {
    let InitChange { changes, digest, root, .. } = change;
    let wire::ImportResponse { files, titles } = response(changes);
    wire::InitResponse { files, titles, digest, root }
}

/// Whether the demo service answers on 127.0.0.1 at `port`: its `/echo` gives back the path it was asked
/// for, which a program that only holds the port does not.
async fn answers(port: u16) -> bool {
    let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)).await else { return false };
    let asked = stream.write_all(b"GET /echo HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").await;
    let mut reply = Vec::new();
    let read = (&mut stream).take(64 * 1024).read_to_end(&mut reply).await;
    let reply = String::from_utf8_lossy(&reply);
    asked.is_ok() && read.is_ok() && reply.starts_with("HTTP/1.1 200") && reply.contains(r#""path":"/echo""#)
}

#[cfg(test)]
#[path = "onboarding_rpcs_tests.rs"]
mod tests;
