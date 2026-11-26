//! The YAML editor's RPCs (ui.md §10 row 4): `Explain`, a test's or api's layering or a
//! diagnostic code's page, as `vc explain` gives them, and `ApplyFixes`, the certain fixes as
//! `vc check --fix` makes them, previewed or written as a skip is. Both read and compile the
//! project on the blocking pool.

use std::sync::Arc;

use tokio::runtime::Handle;
use vc_contracts::engine::v1 as wire;
use vc_engine::{CheckRequest, Code as DiagnosticCode, ExplainRequest};

use crate::authoring_rpcs::{previewed, response};
use crate::engine_api::{EngineApi, Rpcs};
use crate::{Code, ConnectError};

/// Adds the editor's RPCs.
pub fn register(rpcs: Rpcs) -> Rpcs {
    rpcs.unary("Explain", explain).unary("ApplyFixes", apply_fixes)
}

/// `vc explain <code|id>` for the web UI: a code's Markdown page, or the test's or api's
/// explanation under the request's profiles and sets over the server's, masked (D679).
async fn explain(api: Arc<EngineApi>, request: wire::ExplainRequest) -> Result<wire::ExplainResponse, ConnectError> {
    if is_code(&request.target) {
        return code_page(&request.target);
    }
    let (profiles, sets) = api.layers(request.profiles, request.sets);
    let layers = ExplainRequest { profiles, sets };
    let target = request.target;
    let report =
        api.blocking(move |engine| Handle::current().block_on(engine.explain_masked(&target, &layers))).await?;
    Ok(wire::ExplainResponse { text: report.text, markdown: false, notices: report.notices })
}

/// `V` or `v` and four ASCII digits, as `vc explain` reads a code (D180).
fn is_code(target: &str) -> bool {
    target
        .strip_prefix(['V', 'v'])
        .is_some_and(|digits| digits.len() == 4 && digits.bytes().all(|byte| byte.is_ascii_digit()))
}

/// The page of the code `target`; its title alone when its page is still to come.
fn code_page(target: &str) -> Result<wire::ExplainResponse, ConnectError> {
    let unknown = |_| ConnectError::new(Code::NotFound, "no diagnostic has that code");
    let code = target.to_ascii_uppercase().parse::<DiagnosticCode>().map_err(unknown)?;
    let text = code.explanation().map_or_else(|| format!("# {code}: {}\n", code.title()), str::to_owned);
    Ok(wire::ExplainResponse { text, markdown: true, notices: Vec::new() })
}

/// `vc check --fix` for the web UI: the certain fixes of the diagnostics with the request's
/// codes on its paths, previewed, or written when `write` holds the preview's digest (D680).
async fn apply_fixes(
    api: Arc<EngineApi>,
    request: wire::ApplyFixesRequest,
) -> Result<wire::ApplyFixesResponse, ConnectError> {
    let codes = codes(&request.codes)?;
    let expected = request.write.then(|| request.expected_digest.unwrap_or_default());
    let check = CheckRequest { paths: request.paths, ..CheckRequest::default() };
    let fixed =
        move |engine: &vc_engine::Engine| previewed(engine, engine.fix_codes(&check, &codes)?.changes, expected);
    let (changes, digest) = api.blocking(fixed).await?;
    let wire::ImportResponse { files, titles } = response(changes);
    Ok(wire::ApplyFixesResponse { files, titles, digest })
}

/// The codes `written` names; one that is no diagnostic code is refused by its place.
fn codes(written: &[String]) -> Result<Vec<DiagnosticCode>, ConnectError> {
    let parsed = written.iter().enumerate().map(|(index, code)| {
        let refused = |_| ConnectError::new(Code::InvalidArgument, format!("code {} is no diagnostic code", index + 1));
        code.to_ascii_uppercase().parse::<DiagnosticCode>().map_err(refused)
    });
    parsed.collect()
}

#[cfg(test)]
#[path = "editor_rpcs_tests.rs"]
mod tests;
