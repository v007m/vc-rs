use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use http::header::{AUTHORIZATION, CONTENT_TYPE, COOKIE};
use http::{HeaderMap, HeaderValue, StatusCode};
use indexmap::IndexMap;
use serde_json::{Value, json};
use vc_diagnostics::Code;
use vc_interfaces::{
    AuthError, AuthProvider, AuthRequest, Credentials, Secret, SendContext, Transport, TransportError,
};
use vc_model::{
    ActionKind, AuthDef, AuthId, AuthRef, BodySpec, Exchange, HttpResponse, InlineRequest, Method, OwnerKey,
    RenderedRequest, TemplateValue, VarDefs, VarName,
};
use vc_testkit::{FakeTransport, Recorded, RecordingTransport};

use super::{NoAuth, chosen, flow::FlowRunner};
use crate::fixture::{self, Fixture};
use crate::{ActionCx, ActionExecutors, ActionReport, ActionRequest, Frame, HostLimits, PipelineParts, Saved};

/// Issues `Bearer <token>-<n>` for the `n`th fetch; a `broken` auth is rejected, an `apiKey`
/// one puts a query parameter and a cookie on the request, and a `flow` runs its steps.
#[derive(Default)]
struct Issuer {
    fetched: AtomicUsize,
}

impl Issuer {
    fn fetched(&self) -> usize {
        self.fetched.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl AuthProvider for Issuer {
    async fn credentials(&self, request: &AuthRequest) -> Result<Credentials, AuthError> {
        let n = self.fetched.fetch_add(1, Ordering::SeqCst) + 1;
        if let Some(flow) = &request.flow {
            return flow.run().await;
        }
        let token = request.text("token").unwrap_or_default();
        if request.kind == "broken" {
            return Err(AuthError::Rejected { auth: request.auth.clone(), status: 401 });
        }
        if request.kind == "apiKey" {
            let mut credentials = Credentials::query("api_key", format!("{token} {n}"));
            credentials.cookies = vec![("sid".to_owned(), Secret::new("cookie-value"))];
            return Ok(credentials);
        }
        Ok(Credentials::header("authorization", format!("Bearer {token}-{n}")))
    }
}

fn json_response(code: u16, body: &Value) -> HttpResponse {
    let body = Bytes::from(body.to_string());
    let headers = HeaderMap::from_iter([(CONTENT_TYPE, HeaderValue::from_static("application/json"))]);
    HttpResponse { status: StatusCode::from_u16(code).unwrap(), headers, wire_size: body.len() as u64, body }
}

/// Defines the auth `svc` of `kind`, whose `token` setting is its `token` var (default `t`).
fn define(fixture: &mut Fixture, kind: &str) -> AuthId {
    let id = AuthId::new(fixture.sym("svc"));
    let mut def = AuthDef::new(id, kind);
    def.settings = [("token".to_owned(), fixture.value("${var.token}"))].into();
    def.vars.values = fixture.vars(&[("token", "t")]);
    fixture.program.auth.insert(id, def);
    id
}

fn uses(id: AuthId, vars: IndexMap<VarName, TemplateValue>) -> AuthRef {
    AuthRef::Use { id, vars: VarDefs { values: vars, ..VarDefs::default() } }
}

/// A run's context over a transport answering `answers` in turn, with `provider` for auth.
fn context(
    fixture: Fixture,
    provider: Arc<dyn AuthProvider>,
    answers: Vec<HttpResponse>,
) -> (Arc<ActionCx>, vc_testkit::Recordings) {
    let fake = FakeTransport::scripted(answers.into_iter().map(Ok).collect());
    let (transport, recordings) = RecordingTransport::new(Arc::new(fake));
    (over(fixture, provider, Arc::new(transport)), recordings)
}

/// A run's context sending through `transport`, with `provider` for auth.
fn over(fixture: Fixture, provider: Arc<dyn AuthProvider>, transport: Arc<dyn Transport>) -> Arc<ActionCx> {
    let executors = ActionExecutors::standard(PipelineParts::new(transport, HostLimits::spawn(8)));
    Arc::new(ActionCx::new(Arc::new(fixture.program), Arc::new(executors), fixture::inputs(&[])).with_auth(provider))
}

async fn run(cx: &Arc<ActionCx>, action: vc_model::ActionId) -> ActionReport {
    let request = ActionRequest { action, owner: OwnerKey::Run, cx: Arc::clone(cx), frame: Frame::default() };
    cx.executors.execute(request).await
}

fn header(recorded: &Recorded, name: &http::HeaderName) -> Option<String> {
    recorded.request.headers.get(name).and_then(|value| value.to_str().ok()).map(str::to_owned)
}

fn code(report: &ActionReport) -> Option<String> {
    report.outcome.failure().and_then(|failure| failure.code.clone())
}

#[tokio::test]
async fn calls_carry_the_credentials_fetched_once_for_the_run() {
    let mut fixture = Fixture::new();
    let id = define(&mut fixture, "bearer");
    let call = fixture.call("/users");
    fixture.api_mut(call).auth = uses(id, IndexMap::new());
    let issuer = Arc::new(Issuer::default());
    let answers = vec![vc_testkit::status(200), vc_testkit::status(200)];
    let (cx, recordings) = context(fixture, Arc::clone(&issuer) as Arc<dyn AuthProvider>, answers);
    assert!(run(&cx, call).await.outcome.failure().is_none());
    assert!(run(&cx, call).await.outcome.failure().is_none());
    let sent = recordings.take();
    assert_eq!(
        sent.iter().map(|recorded| header(recorded, &AUTHORIZATION)).collect::<Vec<_>>(),
        [Some("Bearer t-1".to_owned()), Some("Bearer t-1".to_owned())]
    );
    assert!(sent[0].request.headers.get(AUTHORIZATION).is_some_and(HeaderValue::is_sensitive));
    assert_eq!(issuer.fetched(), 1);
}

#[test]
fn the_closest_auth_that_does_not_inherit_is_chosen() {
    let mut fixture = Fixture::new();
    let id = define(&mut fixture, "bearer");
    let call = fixture.call("/users");
    let program = &mut fixture.program;
    let chosen_vars = |program: &vc_model::Program, call| {
        let action = &program.actions[usize::try_from(vc_model::ActionId::index(call)).unwrap()];
        let ActionKind::Call(api) = action.kind else { unreachable!("a call") };
        chosen(program, (&program.apis[&api], action)).map(|(def, vars)| (def.id, vars.values.len()))
    };
    assert_eq!(chosen_vars(program, call), None);
    let group = *program.api_groups.keys().next().unwrap();
    program.api_groups[&group].auth = uses(id, IndexMap::new());
    assert_eq!(chosen_vars(program, call), Some((id, 0)));
    let given = fixture.vars(&[("token", "api")]);
    fixture.api_mut(call).auth = uses(id, given);
    assert_eq!(chosen_vars(&fixture.program, call), Some((id, 1)));
    fixture.action_mut(call).auth = Some(AuthRef::None);
    assert_eq!(chosen_vars(&fixture.program, call), None);
    fixture.action_mut(call).auth = Some(AuthRef::Inherit);
    assert_eq!(chosen_vars(&fixture.program, call), Some((id, 1)));
    fixture.program.auth.clear();
    assert_eq!(chosen_vars(&fixture.program, call), None);
}

#[tokio::test]
async fn use_site_vars_win_and_each_set_of_vars_has_its_own_credentials() {
    let mut fixture = Fixture::new();
    let id = define(&mut fixture, "bearer");
    let (first, second) = (fixture.call("/a"), fixture.call("/b"));
    fixture.api_mut(first).auth = uses(id, fixture.vars(&[("token", "alice")]));
    fixture.api_mut(second).auth = uses(id, fixture.vars(&[("token", "bob")]));
    let issuer = Arc::new(Issuer::default());
    let (cx, recordings) =
        context(fixture, Arc::clone(&issuer) as Arc<dyn AuthProvider>, vec![vc_testkit::status(200); 3]);
    for call in [first, second, first] {
        assert!(run(&cx, call).await.outcome.failure().is_none());
    }
    let sent: Vec<_> = recordings.take().iter().map(|recorded| header(recorded, &AUTHORIZATION)).collect();
    assert_eq!(sent, [Some("Bearer alice-1".into()), Some("Bearer bob-2".into()), Some("Bearer alice-1".into())]);
    assert_eq!(issuer.fetched(), 2);
}

#[tokio::test]
async fn an_auth_var_reads_the_other_vars_of_its_definition_and_use_site() {
    let mut fixture = Fixture::new();
    let id = define(&mut fixture, "bearer");
    let vars = fixture.vars(&[("token", "${var.user}-key"), ("user", "ada")]);
    fixture.program.auth.get_mut(&id).unwrap().vars.values = vars;
    let call = fixture.call("/users");
    fixture.api_mut(call).auth = uses(id, fixture.vars(&[("user", "${var.region}-bob"), ("region", "ops")]));
    let issuer = Arc::new(Issuer::default());
    let (cx, recordings) = context(fixture, issuer as Arc<dyn AuthProvider>, vec![vc_testkit::status(200)]);
    let report = run(&cx, call).await;
    assert!(report.outcome.failure().is_none(), "{:?}", report.outcome.failure());
    let sent = recordings.take();
    assert_eq!(header(&sent[0], &AUTHORIZATION), Some("Bearer ops-bob-key-1".to_owned()));
}

/// What a bearer `svc` sends when its vars are `definition`, its use site's are `site` and the
/// call's are `caller`.
async fn sent_with(definition: &[(&str, &str)], site: &[(&str, &str)], caller: &[(&str, &str)]) -> Option<String> {
    let mut fixture = Fixture::new();
    let id = define(&mut fixture, "bearer");
    let vars = fixture.vars(definition);
    fixture.program.auth.get_mut(&id).unwrap().vars.values = vars;
    let call = fixture.call("/users");
    fixture.action_mut(call).vars.values = fixture.vars(caller);
    fixture.api_mut(call).auth = uses(id, fixture.vars(site));
    let issuer = Arc::new(Issuer::default());
    let (cx, recordings) = context(fixture, issuer as Arc<dyn AuthProvider>, vec![vc_testkit::status(200)]);
    let report = run(&cx, call).await;
    assert!(report.outcome.failure().is_none(), "{:?}", report.outcome.failure());
    header(&recordings.take()[0], &AUTHORIZATION)
}

#[tokio::test]
async fn a_use_site_var_falls_back_to_the_definition_s_vars_and_reads_its_own_default() {
    let tenant = [("token", "${var.user}"), ("tenant", "acme")];
    assert_eq!(sent_with(&tenant, &[("user", "${var.tenant}-bob")], &[]).await.as_deref(), Some("Bearer acme-bob-1"));
    let token = [("token", "t")];
    assert_eq!(sent_with(&token, &[("token", "${var.token}-v2")], &[]).await.as_deref(), Some("Bearer t-v2-1"));
}

#[tokio::test]
async fn a_use_site_var_reading_its_own_name_finds_the_caller_s_value_before_the_definition_s() {
    let site = [("token", "${var.token}-v2")];
    assert_eq!(sent_with(&[("token", "t")], &site, &[("token", "c")]).await.as_deref(), Some("Bearer c-v2-1"));
}

/// The call and the definition each have a fake `salt`; the call sends its own as `x-salt`.
#[tokio::test]
async fn a_use_site_var_reads_the_caller_s_fake_over_a_same_named_one_of_the_definition() {
    let mut fixture = Fixture::new();
    let id = define(&mut fixture, "bearer");
    let own = fixture.vars(&[("token", "${var.k}"), ("salt", "${fake.string.uuid}")]);
    fixture.program.auth.get_mut(&id).unwrap().vars.values = own;
    let call = fixture.call("/users");
    fixture.action_mut(call).vars.values = fixture.vars(&[("salt", "${fake.string.uuid}")]);
    fixture.api_mut(call).headers = fixture.templates(&[("x-salt", "${var.salt}")]);
    fixture.api_mut(call).auth = uses(id, fixture.vars(&[("k", "${var.salt}")]));
    let issuer = Arc::new(Issuer::default());
    let (cx, recordings) = context(fixture, issuer as Arc<dyn AuthProvider>, vec![vc_testkit::status(200)]);
    let report = run(&cx, call).await;
    assert!(report.outcome.failure().is_none(), "{:?}", report.outcome.failure());
    let sent = recordings.take();
    let salt = header(&sent[0], &http::HeaderName::from_static("x-salt")).unwrap();
    assert_eq!(header(&sent[0], &AUTHORIZATION), Some(format!("Bearer {salt}-1")), "the call's one draw");
}

#[tokio::test]
async fn a_fake_definition_var_the_use_site_reads_is_drawn_once() {
    let salted = [("token", "${var.salt}|${var.k}"), ("salt", "${fake.string.uuid}")];
    let bearer = sent_with(&salted, &[("k", "${var.salt}")], &[]).await.unwrap();
    let token = bearer.strip_prefix("Bearer ").and_then(|token| token.strip_suffix("-1")).unwrap();
    let [salt, k] = token.split('|').collect::<Vec<_>>()[..] else { panic!("{token}") };
    assert_eq!(salt, k, "the use site and the definition read one draw");
}

/// The call draws `nonce` and sends it as `x-nonce`; `who` is `alice` where the call renders.
/// The definition's `salt` is a fake, its `who` is `bob`; the use site passes `n` and `k`.
#[tokio::test]
async fn auth_vars_render_in_the_call_s_render_and_a_use_site_var_in_the_caller_s_scope() {
    let mut fixture = Fixture::new();
    let id = define(&mut fixture, "bearer");
    let own = [("token", "${var.salt}|${var.salt}|${var.n}|${var.k}"), ("salt", "${fake.string.uuid}"), ("who", "bob")];
    fixture.program.auth.get_mut(&id).unwrap().vars.values = fixture.vars(&own);
    let call = fixture.call("/users");
    fixture.action_mut(call).vars.values = fixture.vars(&[("nonce", "${fake.string.uuid}"), ("who", "alice")]);
    fixture.api_mut(call).headers = fixture.templates(&[("x-nonce", "${var.nonce}")]);
    fixture.api_mut(call).auth = uses(id, fixture.vars(&[("n", "${var.nonce}"), ("k", "k-${var.who}")]));
    let issuer = Arc::new(Issuer::default());
    let (cx, recordings) = context(fixture, issuer as Arc<dyn AuthProvider>, vec![vc_testkit::status(200)]);
    let report = run(&cx, call).await;
    assert!(report.outcome.failure().is_none(), "{:?}", report.outcome.failure());
    let sent = recordings.take();
    let nonce = header(&sent[0], &http::HeaderName::from_static("x-nonce")).unwrap();
    let bearer = header(&sent[0], &AUTHORIZATION).unwrap();
    let token = bearer.strip_prefix("Bearer ").and_then(|token| token.strip_suffix("-1")).unwrap();
    let [salt, again, n, k] = token.split('|').collect::<Vec<_>>()[..] else { panic!("{token}") };
    assert_eq!((salt, n, k), (again, nonce.as_str(), "k-alice"), "one salt, the call's nonce, the caller's who");
    assert!(!report.fakes.contains_key("salt"), "an auth var's draw stays out of the call's memo");
}

#[tokio::test]
async fn a_use_site_var_reading_a_value_a_function_saved_undeclared_is_a_v5108_warning() {
    let mut fixture = Fixture::new();
    let id = define(&mut fixture, "bearer");
    let seed =
        fixture.action(ActionKind::Function(vc_model::HookRef { file: "hooks.ts".into(), export: "seed".into() }));
    let call = fixture.call("/users");
    fixture.api_mut(call).auth = uses(id, fixture.vars(&[("token", "${var.secretish}")]));
    let issuer = Arc::new(Issuer::default());
    let (cx, recordings) = context(fixture, issuer as Arc<dyn AuthProvider>, vec![vc_testkit::status(200)]);
    let mut frame = Frame::default();
    let left: Saved = std::iter::once(("secretish".to_owned(), json!("s"))).collect();
    frame.absorb(&cx.program, seed, &ActionReport { saved: left, ..ActionReport::passed() });
    let report =
        cx.executors.execute(ActionRequest { action: call, owner: OwnerKey::Run, cx: Arc::clone(&cx), frame }).await;
    assert_eq!(header(&recordings.take()[0], &AUTHORIZATION), Some("Bearer s-1".to_owned()));
    let warned: Vec<_> = report.warnings.iter().map(|warning| (warning.code.clone(), warning.action)).collect();
    assert_eq!(warned, [(Some(Code::V5108.to_string()), Some(seed))]);
}

#[tokio::test]
async fn a_refresh_on_status_fetches_again_and_retries_once() {
    let mut fixture = Fixture::new();
    let id = define(&mut fixture, "bearer");
    let call = fixture.call("/users");
    fixture.api_mut(call).auth = uses(id, IndexMap::new());
    let issuer = Arc::new(Issuer::default());
    let answers =
        vec![vc_testkit::status(401), vc_testkit::status(200), vc_testkit::status(401), vc_testkit::status(401)];
    let (cx, recordings) = context(fixture, Arc::clone(&issuer) as Arc<dyn AuthProvider>, answers);
    assert!(run(&cx, call).await.outcome.failure().is_none());
    assert!(run(&cx, call).await.outcome.failure().is_some());
    let sent: Vec<_> = recordings.take().iter().map(|recorded| header(recorded, &AUTHORIZATION)).collect();
    let bearer = |n: usize| Some(format!("Bearer t-{n}"));
    assert_eq!(sent, [bearer(1), bearer(2), bearer(2), bearer(3)]);
    assert_eq!(issuer.fetched(), 3);
}

/// Holds each request carrying `Bearer t-1` until all have come, then answers them 401 one
/// at a time, each once the one before has been retried with a new token, which gets 200.
struct RejectedInTurn {
    barrier: tokio::sync::Barrier,
    turn: tokio::sync::Semaphore,
    inner: FakeTransport,
}

#[async_trait]
impl Transport for RejectedInTurn {
    async fn send(&self, request: RenderedRequest, ctx: &SendContext) -> Result<Exchange, TransportError> {
        if request.headers.get(AUTHORIZATION).is_some_and(|value| value == "Bearer t-1") {
            if !self.barrier.wait().await.is_leader() {
                self.turn.acquire().await.unwrap().forget();
            }
        } else {
            self.turn.add_permits(1);
        }
        self.inner.send(request, ctx).await
    }
}

#[tokio::test]
async fn concurrent_refresh_on_statuses_fetch_the_credentials_again_once() {
    let mut fixture = Fixture::new();
    let id = define(&mut fixture, "bearer");
    let call = fixture.call("/users");
    fixture.api_mut(call).auth = uses(id, IndexMap::new());
    let issuer = Arc::new(Issuer::default());
    let inner = FakeTransport::new(|request, _| {
        let stale = request.headers.get(AUTHORIZATION).is_some_and(|value| value == "Bearer t-1");
        Ok(vc_testkit::status(if stale { 401 } else { 200 }))
    });
    let transport =
        RejectedInTurn { barrier: tokio::sync::Barrier::new(4), turn: tokio::sync::Semaphore::new(0), inner };
    let cx = over(fixture, Arc::clone(&issuer) as Arc<dyn AuthProvider>, Arc::new(transport));
    let calls = async { tokio::join!(run(&cx, call), run(&cx, call), run(&cx, call), run(&cx, call)) };
    let reports = tokio::time::timeout(Duration::from_secs(10), calls).await.unwrap();
    for report in [reports.0, reports.1, reports.2, reports.3] {
        assert!(report.outcome.failure().is_none(), "{report:?}");
    }
    assert_eq!(issuer.fetched(), 2, "one fetch for the run and one after the four 401s");
}

#[tokio::test]
async fn credentials_that_cannot_be_obtained_fail_the_call_with_v5114_before_sending() {
    let mut fixture = Fixture::new();
    let id = define(&mut fixture, "broken");
    let call = fixture.call("/users");
    fixture.api_mut(call).auth = uses(id, fixture.vars(&[("token", "hunter22")]));
    let (cx, recordings) = context(fixture, Arc::new(Issuer::default()), vec![vc_testkit::status(200)]);
    let report = run(&cx, call).await;
    assert_eq!(code(&report), Some(Code::V5114.to_string()));
    let message = &report.outcome.failure().unwrap().message;
    assert!(message.contains("svc") && !message.contains("hunter22"), "{message}");
    assert_eq!(recordings.take().len(), 0);
}

#[tokio::test]
async fn a_run_without_an_auth_provider_knows_no_auth_type() {
    let mut fixture = Fixture::new();
    let id = define(&mut fixture, "bearer");
    let call = fixture.call("/users");
    fixture.api_mut(call).auth = uses(id, IndexMap::new());
    let (cx, _) = context(fixture, Arc::new(NoAuth), vec![vc_testkit::status(200)]);
    let report = run(&cx, call).await;
    assert_eq!(code(&report), Some(Code::V5114.to_string()));
    assert!(report.outcome.failure().unwrap().message.contains("bearer"));
}

#[tokio::test]
async fn query_and_cookie_credentials_join_what_the_request_has() {
    let mut fixture = Fixture::new();
    let id = define(&mut fixture, "apiKey");
    let call = fixture.call("/users?page=1");
    fixture.api_mut(call).auth = uses(id, IndexMap::new());
    fixture.action_mut(call).headers = fixture.templates(&[("cookie", "theme=dark")]);
    let (cx, recordings) = context(fixture, Arc::new(Issuer::default()), vec![vc_testkit::status(200)]);
    assert!(run(&cx, call).await.outcome.failure().is_none());
    let sent = recordings.take();
    assert_eq!(sent[0].request.uri.to_string(), "http://api.test/users?page=1&api_key=t%201");
    assert_eq!(header(&sent[0], &COOKIE).as_deref(), Some("theme=dark; sid=cookie-value"));
}

#[tokio::test]
async fn credentials_that_cannot_go_on_a_request_fail_with_v5114() {
    struct Unusable;
    #[async_trait]
    impl AuthProvider for Unusable {
        async fn credentials(&self, _: &AuthRequest) -> Result<Credentials, AuthError> {
            Ok(Credentials::header("not a header", "x"))
        }
    }
    let mut fixture = Fixture::new();
    let id = define(&mut fixture, "bearer");
    let call = fixture.call("/users");
    fixture.api_mut(call).auth = uses(id, IndexMap::new());
    let (cx, recordings) = context(fixture, Arc::new(Unusable), vec![vc_testkit::status(200)]);
    assert_eq!(code(&run(&cx, call).await), Some(Code::V5114.to_string()));
    assert_eq!(recordings.take().len(), 0);
}

/// A `flow` auth `svc` whose one inline step (`login`) posts the `user` var to `/login` and
/// saves the token it answers with; `apply` puts it in `authorization`.
fn flow(fixture: &mut Fixture, expires_in: Option<&str>) -> AuthId {
    let id = AuthId::new(fixture.sym("svc"));
    let body = TemplateValue::Map([("user".to_owned(), fixture.value("${var.user}"))].into());
    let url = fixture.template("http://id.test/login");
    let login = fixture.action(ActionKind::Inline(Box::new(InlineRequest {
        method: Method::Post,
        url,
        query: IndexMap::new(),
        headers: IndexMap::new(),
        body: BodySpec::Json(body),
    })));
    fixture.action_mut(login).id = Some(fixture.sym("login"));
    fixture.action_mut(login).save.values = fixture.vars(&[("token", "${response.body.access_token}")]);
    let mut def = AuthDef::new(id, "flow");
    def.steps = vec![login];
    def.vars.values = fixture.vars(&[("user", "ada")]);
    let headers = TemplateValue::Map([("authorization".to_owned(), fixture.value("Bearer ${var.token}"))].into());
    def.settings = [("apply".to_owned(), TemplateValue::Map([("headers".to_owned(), headers)].into()))].into();
    def.expires_in = expires_in.map(|text| fixture.template(text));
    fixture.program.auth.insert(id, def);
    id
}

#[tokio::test]
async fn a_flow_runs_its_steps_and_applies_what_they_saved() {
    let mut fixture = Fixture::new();
    let id = flow(&mut fixture, None);
    let call = fixture.call("/users");
    fixture.api_mut(call).auth = uses(id, IndexMap::new());
    let answers = vec![json_response(200, &json!({"access_token": "tok-9"})), vc_testkit::status(200)];
    let (cx, recordings) = context(fixture, Arc::new(Issuer::default()), answers);
    let report = run(&cx, call).await;
    assert!(report.outcome.failure().is_none(), "{:?}", report.outcome);
    let sent = recordings.take();
    assert_eq!(sent[0].request.uri.to_string(), "http://id.test/login");
    assert_eq!(sent[0].request.body, Bytes::from(json!({"user": "ada"}).to_string()));
    assert_eq!(header(&sent[1], &AUTHORIZATION).as_deref(), Some("Bearer tok-9"));
    assert!(!cx.redactor.text("got tok-9").contains("tok-9"));
}

#[tokio::test]
async fn a_failing_flow_step_fails_the_call_naming_the_step_but_not_its_response() {
    let mut fixture = Fixture::new();
    let id = flow(&mut fixture, None);
    let call = fixture.call("/users");
    fixture.api_mut(call).auth = uses(id, IndexMap::new());
    let answers = vec![json_response(500, &json!({"error": "hunter22 is wrong"}))];
    let (cx, recordings) = context(fixture, Arc::new(Issuer::default()), answers);
    let report = run(&cx, call).await;
    assert_eq!(code(&report), Some(Code::V5114.to_string()));
    let message = &report.outcome.failure().unwrap().message;
    assert!(message.contains("step `login` failed") && !message.contains("hunter22"), "{message}");
    assert_eq!(recordings.take().len(), 1);
}

/// The vars a `flow` runner gets from the definition: `user` is `ada`.
fn ada() -> Arc<Saved> {
    Arc::new(Saved::from_iter([("user".to_owned(), json!("ada"))]))
}

#[tokio::test]
async fn a_flow_renders_when_its_credentials_expire() {
    for (expires_in, seconds) in [("${var.token}", 90), ("2m", 120)] {
        let mut fixture = Fixture::new();
        let id = flow(&mut fixture, Some(expires_in));
        let def = fixture.program.auth[&id].clone();
        let answers = vec![json_response(200, &json!({"access_token": 90}))];
        let (cx, _) = context(fixture, Arc::new(Issuer::default()), answers);
        let runner = FlowRunner::new(Arc::clone(&cx), &def, ada());
        let credentials = vc_interfaces::ActionRunner::run(&runner).await.unwrap();
        assert_eq!(credentials.expires_in, Some(Duration::from_secs(seconds)));
    }
}

#[tokio::test]
async fn a_flow_fails_on_an_expiry_that_is_no_duration_or_an_empty_list_of_steps() {
    let mut fixture = Fixture::new();
    let id = flow(&mut fixture, Some("soon"));
    let mut def = fixture.program.auth[&id].clone();
    let answers = vec![json_response(200, &json!({"access_token": "t"}))];
    let (cx, _) = context(fixture, Arc::new(Issuer::default()), answers);
    let runner = FlowRunner::new(Arc::clone(&cx), &def, ada());
    let error = vc_interfaces::ActionRunner::run(&runner).await.unwrap_err();
    assert!(matches!(&error, AuthError::Flow { reason, .. } if reason.contains("expiresIn")), "{error}");
    def.steps.clear();
    let runner = FlowRunner::new(cx, &def, ada());
    let error = vc_interfaces::ActionRunner::run(&runner).await.unwrap_err();
    assert!(matches!(&error, AuthError::Flow { reason, .. } if reason.contains("no steps")), "{error}");
}

#[tokio::test]
async fn the_start_up_check_reaches_a_flow_s_steps() {
    let mut fixture = Fixture::new();
    let id = flow(&mut fixture, None);
    let call = fixture.call("/users");
    fixture.api_mut(call).auth = uses(id, IndexMap::new());
    let login = fixture.program.auth[&id].steps[0];
    let parts = PipelineParts::new(Arc::new(FakeTransport::scripted(Vec::new())), HostLimits::spawn(1));
    let calls_only = ActionExecutors::new().with(crate::ActionKindTag::Call, Arc::new(crate::CallExecutor::new(parts)));
    let error = calls_only.check(&fixture.program, vec![call]).unwrap_err();
    assert!(matches!(error, crate::ExecutorError::NoExecutor { action, .. } if action == login));
}

fn probing(cx: &Arc<ActionCx>, action: vc_model::ActionId) -> ActionRequest {
    ActionRequest { action, owner: OwnerKey::Run, cx: Arc::clone(cx), frame: Frame::default() }
}

#[tokio::test]
async fn a_probe_names_what_the_auth_puts_on_a_request_and_fetches_through_the_run_s_cache() {
    let mut fixture = Fixture::new();
    let id = define(&mut fixture, "apiKey");
    let call = fixture.call("/users");
    fixture.api_mut(call).auth = uses(id, fixture.vars(&[("token", "probe-token")]));
    let issuer = Arc::new(Issuer::default());
    let (cx, recordings) =
        context(fixture, Arc::clone(&issuer) as Arc<dyn AuthProvider>, vec![vc_testkit::status(200)]);
    let probed = super::probe_auth(&probing(&cx, call)).await.unwrap().unwrap();
    assert!(!format!("{probed:?}").contains("probe-token"), "{probed:?}");
    assert_eq!(
        (probed.headers.len(), probed.query, probed.cookies, probed.expires_in),
        (0, vec!["api_key".to_owned()], vec!["sid".to_owned()], None)
    );
    let masked = cx.redactor.masked_text("got probe-token 1").await;
    assert!(!masked.contains("probe-token"), "what a probe fetched is masked from then on: {masked}");
    assert!(super::probe_auth(&probing(&cx, call)).await.unwrap().is_some());
    assert_eq!((issuer.fetched(), recordings.take().len()), (1, 0), "one fetch, and nothing is sent");
}

#[tokio::test]
async fn a_probe_of_credentials_that_cannot_be_obtained_is_a_v5114_failure_without_the_vars() {
    let mut fixture = Fixture::new();
    let id = define(&mut fixture, "broken");
    let call = fixture.call("/users");
    fixture.api_mut(call).auth = uses(id, fixture.vars(&[("token", "hunter22")]));
    let (cx, _) = context(fixture, Arc::new(Issuer::default()), Vec::new());
    let failure = super::probe_auth(&probing(&cx, call)).await.unwrap_err();
    assert_eq!(failure.code, Some(Code::V5114.to_string()));
    assert!(failure.message.contains("svc") && !failure.message.contains("hunter22"), "{}", failure.message);
}

#[tokio::test]
async fn a_probe_of_a_call_without_auth_is_none() {
    let mut fixture = Fixture::new();
    let call = fixture.call("/users");
    let (cx, _) = context(fixture, Arc::new(Issuer::default()), Vec::new());
    assert_eq!(super::probe_auth(&probing(&cx, call)).await.unwrap(), None);
}

#[test]
fn chosen_auth_names_the_auth_a_call_uses() {
    let mut fixture = Fixture::new();
    let id = define(&mut fixture, "bearer");
    let call = fixture.call("/users");
    fixture.api_mut(call).auth = uses(id, IndexMap::new());
    let program = &fixture.program;
    let action = &program.actions[usize::try_from(vc_model::ActionId::index(call)).unwrap()];
    let ActionKind::Call(api) = action.kind else { unreachable!("a call") };
    assert_eq!(super::chosen_auth(program, (&program.apis[&api], action)), Some(id));
}

/// The origin a call to `/users` on `base` sends to, or why it has none.
fn origin_on(base: Option<&str>) -> Result<String, String> {
    let mut fixture = Fixture::new();
    let base = base.map(|base| fixture.template(base));
    fixture.group_mut().base_url = base;
    let call = fixture.call("/users");
    let (cx, recordings) = context(fixture, Arc::new(Issuer::default()), Vec::new());
    let origin = super::probe_origin(&probing(&cx, call)).map_err(|failure| failure.message);
    assert!(recordings.take().is_empty(), "nothing is sent");
    origin
}

#[tokio::test]
async fn a_probe_names_the_origin_in_lower_case_with_the_scheme_s_port() {
    assert_eq!(origin_on(Some("https://API.Test")), Ok("https://api.test:443".to_owned()));
    assert_eq!(origin_on(Some("HTTP://api.test")), Ok("http://api.test:80".to_owned()));
    assert_eq!(origin_on(Some("http://api.test:8080/v1")), Ok("http://api.test:8080".to_owned()));
    assert_eq!(origin_on(Some("ws://api.test")), Ok("ws://api.test".to_owned()), "no port is assumed");
    assert!(origin_on(None).is_err(), "a call with no base URL names no origin");
}
