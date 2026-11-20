use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use async_trait::async_trait;
use http::HeaderValue;
use http::header::AUTHORIZATION;
use indexmap::IndexMap;
use vc_diagnostics::Code;
use vc_interfaces::{AuthError, AuthProvider, AuthRequest, Clock, Credentials, RequestSigner, Secret, Signer};
use vc_model::{AuthDef, AuthId, AuthRef, Date, HttpResponse, OwnerKey, RenderedRequest, VarDefs};
use vc_testkit::{FakeTransport, RecordingTransport, Recordings};

use crate::fixture::{self, Fixture};
use crate::{ActionCx, ActionExecutors, ActionReport, ActionRequest, Frame, HostLimits, PipelineParts};

const KEY_ID: &str = "AKIDSIGNTEST";
const SECRET: &str = "s3cret-signing-key";

/// Puts `authorization: SIG <now> <header names>` on the request, or fails on `/refused`.
struct Tagger;

impl RequestSigner for Tagger {
    fn sign(&self, mut request: RenderedRequest, now_ms: u64) -> Result<RenderedRequest, AuthError> {
        if request.uri.path() == "/refused" {
            return Err(AuthError::Malformed { auth: "svc".into(), missing: "a signable path".into() });
        }
        let mut names: Vec<_> = request.headers.keys().map(http::HeaderName::as_str).collect();
        names.sort_unstable();
        let mut value = HeaderValue::try_from(format!("SIG {now_ms} {} {KEY_ID}", names.join(";")))
            .map_err(|_| AuthError::Malformed { auth: "svc".into(), missing: "a header".into() })?;
        value.set_sensitive(true);
        request.headers.insert(AUTHORIZATION, value);
        Ok(request)
    }
}

/// Serves a [`Tagger`] signer, counting its fetches; it says it signs when `signs` is set.
struct Signing {
    signs: bool,
    fetched: AtomicUsize,
}

impl Signing {
    fn new(signs: bool) -> Arc<Self> {
        Arc::new(Self { signs, fetched: AtomicUsize::new(0) })
    }
}

#[async_trait]
impl AuthProvider for Signing {
    async fn credentials(&self, _request: &AuthRequest) -> Result<Credentials, AuthError> {
        self.fetched.fetch_add(1, Ordering::SeqCst);
        let hidden = vec![Secret::new(KEY_ID), Secret::new(SECRET)];
        Ok(Credentials::signed(Signer::new(Arc::new(Tagger), hidden)))
    }

    fn signs(&self, _request: &AuthRequest) -> bool {
        self.signs
    }
}

/// A clock a second further on each time it is read.
#[derive(Default)]
struct Stepping(AtomicU64);

impl Clock for Stepping {
    fn today(&self) -> Date {
        Date::UNIX_EPOCH
    }

    fn now_ms(&self) -> u64 {
        (self.0.fetch_add(1, Ordering::SeqCst) + 1) * 1000
    }
}

/// A call to `path` with an `x-trace` header using an auth of type `sig`, its context over a transport answering
/// `answers`, signed by `provider` at the time `clock` tells.
fn signed_call(
    path: &str,
    provider: Arc<dyn AuthProvider>,
    (answers, clock): (Vec<HttpResponse>, Option<Arc<dyn Clock>>),
) -> (Arc<ActionCx>, vc_model::ActionId, Recordings) {
    let mut fixture = Fixture::new();
    let id = AuthId::new(fixture.sym("svc"));
    fixture.program.auth.insert(id, AuthDef::new(id, "sig"));
    let call = fixture.call(path);
    fixture.api_mut(call).auth = AuthRef::Use { id, vars: VarDefs { values: IndexMap::new(), ..VarDefs::default() } };
    fixture.api_mut(call).headers = fixture.templates(&[("x-trace", "t1")]);
    let fake = FakeTransport::scripted(answers.into_iter().map(Ok).collect());
    let (transport, recordings) = RecordingTransport::new(Arc::new(fake));
    let mut parts = PipelineParts::new(Arc::new(transport), HostLimits::spawn(8));
    if let Some(clock) = clock {
        parts = parts.with_clock(clock);
    }
    let executors = Arc::new(ActionExecutors::standard(parts));
    let cx = ActionCx::new(Arc::new(fixture.program), executors, fixture::inputs(&[])).with_auth(provider);
    (Arc::new(cx), call, recordings)
}

async fn run(cx: &Arc<ActionCx>, action: vc_model::ActionId) -> ActionReport {
    let request = ActionRequest { action, owner: OwnerKey::Run, cx: Arc::clone(cx), frame: Frame::default() };
    cx.executors.execute(request).await
}

fn signatures(recordings: &Recordings) -> Vec<Option<String>> {
    let sent = recordings.take();
    sent.iter()
        .map(|sent| sent.request.headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok()).map(Into::into))
        .collect()
}

fn code(report: &ActionReport) -> Option<String> {
    report.outcome.failure().and_then(|failure| failure.code.clone())
}

#[tokio::test]
async fn each_attempt_is_signed_as_it_leaves_with_the_credentials_fetched_once() {
    let provider = Signing::new(true);
    let answers = vec![vc_testkit::status(401), vc_testkit::status(200)];
    let clock: Arc<dyn Clock> = Arc::new(Stepping::default());
    let (cx, call, recordings) = signed_call("/users", Arc::clone(&provider) as _, (answers, Some(clock)));
    assert!(run(&cx, call).await.outcome.failure().is_none());
    let signed = signatures(&recordings);
    let at = |now: u64| Some(format!("SIG {now} x-trace {KEY_ID}"));
    assert_eq!(signed, [at(1000), at(2000)], "the retry after the refresh is signed again, later");
    assert_eq!(provider.fetched.load(Ordering::SeqCst), 2, "the refreshOn status fetched again");
}

#[tokio::test]
async fn what_the_signer_hides_is_masked_from_then_on() {
    let clock: Arc<dyn Clock> = Arc::new(Stepping::default());
    let (cx, call, recordings) =
        signed_call("/users", Signing::new(true), (vec![vc_testkit::status(200)], Some(clock)));
    assert!(run(&cx, call).await.outcome.failure().is_none());
    let sent = recordings.take();
    assert!(sent[0].request.headers.get(AUTHORIZATION).is_some_and(HeaderValue::is_sensitive));
    let masked = cx.redactor.masked_text(&format!("{KEY_ID} and {SECRET}")).await;
    assert!(!masked.contains(KEY_ID) && !masked.contains(SECRET), "{masked}");
}

#[tokio::test]
async fn a_signer_that_fails_or_a_host_with_no_clock_fails_the_call_with_v5114() {
    let clock: Arc<dyn Clock> = Arc::new(Stepping::default());
    let (cx, call, recordings) =
        signed_call("/refused", Signing::new(true), (vec![vc_testkit::status(200)], Some(clock)));
    let report = run(&cx, call).await;
    assert_eq!(code(&report), Some(Code::V5114.to_string()), "{:?}", report.outcome);
    assert!(recordings.take().is_empty(), "nothing is sent unsigned");
    let (cx, call, recordings) = signed_call("/users", Signing::new(true), (vec![vc_testkit::status(200)], None));
    let report = run(&cx, call).await;
    assert_eq!(code(&report), Some(Code::V5114.to_string()));
    let message = report.outcome.failure().map(|failure| failure.message.clone()).unwrap_or_default();
    assert!(message.contains("no auth type is called `sig`"), "{message}");
    assert!(recordings.take().is_empty());
}

#[tokio::test]
async fn a_type_that_does_not_sign_leaves_the_request_unsigned() {
    let clock: Arc<dyn Clock> = Arc::new(Stepping::default());
    let (cx, call, recordings) =
        signed_call("/users", Signing::new(false), (vec![vc_testkit::status(200)], Some(clock)));
    assert!(run(&cx, call).await.outcome.failure().is_none());
    assert_eq!(signatures(&recordings), [None]);
}
