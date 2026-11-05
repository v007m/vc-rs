use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use http::header::CONTENT_TYPE;
use http::{HeaderMap, HeaderValue, StatusCode};
use serde_json::{Map, Value, json};
use vc_interfaces::{
    AuthError, AuthProvider, AuthRequest, Credentials, Secret, Secrets, SendContext, Transport, TransportError,
};
use vc_model::{
    AuthDef, AuthId, AuthRef, BodySpec, Check, CheckKind, ConnectionInfo, Event, Exchange, Feed, HttpResponse, Matcher,
    Method, Origin, RedactRule, RenderedRequest, TemplateValue, Timing, VarDefs,
};

use super::{Executor, RunOptions};
use crate::fixture::{self, Fixture};
use crate::{ActionCx, ActionExecutors, EventBus, HostLimits, PipelineParts};

const SECRET: &str = "s3cr3t-key-4242";
const TOKEN: &str = "tok-abcdef-99";
const LEAKS: [&str; 4] = [SECRET, TOKEN, "pw-field-1", "123-45-6789"];

/// Answers 200 with a JSON body echoing what it was sent, plus a password and an SSN.
struct Echo;

#[async_trait]
impl Transport for Echo {
    async fn send(&self, request: RenderedRequest, _: &SendContext) -> Result<Exchange, TransportError> {
        let headers: Map<String, Value> = request
            .headers
            .iter()
            .map(|(name, value)| (name.to_string(), json!(value.to_str().unwrap_or_default())))
            .collect();
        let body = json!({
            "uri": request.uri.to_string(),
            "headers": headers,
            "body": String::from_utf8_lossy(&request.body),
            "password": "pw-field-1",
            "ssn": "123-45-6789",
        });
        let body = Bytes::from(body.to_string());
        let mut response_headers = HeaderMap::new();
        response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let wire_size = u64::try_from(body.len()).unwrap();
        let response = HttpResponse { status: StatusCode::OK, headers: response_headers, wire_size, body };
        Ok(Exchange { request, response, timing: Timing::default(), connection: ConnectionInfo::default() })
    }
}

/// Issues a fixed bearer token.
struct Issuer;

#[async_trait]
impl AuthProvider for Issuer {
    async fn credentials(&self, _: &AuthRequest) -> Result<Credentials, AuthError> {
        Ok(Credentials::header("authorization", format!("Bearer {TOKEN}")))
    }
}

/// A test whose call sends the secret in its query, a header and its body, under a bearer
/// auth, and then fails an `Equals` check on the echoed body.
fn fixture() -> (Fixture, vc_model::TestId) {
    let mut fixture = Fixture::new();
    fixture.program.config.redact_fields = vec!["password".to_owned()];
    fixture.program.config.redact_rules = vec![RedactRule::Regex(r"\d{3}-\d{2}-\d{4}".to_owned())];
    let id = AuthId::new(fixture.sym("svc"));
    fixture.program.auth.insert(id, AuthDef::new(id, "bearer"));
    fixture.group_mut().auth = AuthRef::Use { id, vars: VarDefs::default() };
    let call = fixture.call("/users");
    let query = fixture.templates(&[("key", "${secret.API_KEY}")]);
    let headers = fixture.templates(&[("x-api-key", "${secret.API_KEY}")]);
    let body = BodySpec::Text(fixture.template("key=${secret.API_KEY}"));
    let api = fixture.api_mut(call);
    (api.method, api.query, api.headers, api.body) = (Method::Post, query, headers, body);
    let expected = TemplateValue::Map([("ok".to_owned(), TemplateValue::Bool(true))].into());
    let check = Check {
        kind: CheckKind::Equals,
        target: None,
        matcher: Matcher::Equals(expected),
        origin: Origin::Action(call),
        span: fixture::span(),
    };
    fixture.action_mut(call).expect.checks = vec![check];
    let test = fixture.test(vec![call], None);
    (fixture, test)
}

#[tokio::test]
async fn no_event_shows_a_secret_a_token_a_hidden_field_or_a_rule_match() {
    let (fixture, test) = fixture();
    let plan = Arc::new(fixture.plan(&[], &[(test, 1)]));
    let mut secrets = Secrets::new();
    secrets.insert("API_KEY", Secret::new(SECRET));
    let executors = Arc::new(ActionExecutors::standard(PipelineParts::new(Arc::new(Echo), HostLimits::spawn(8))));
    let inputs = fixture::inputs(&[]).with_secrets(secrets);
    let cx = ActionCx::new(Arc::new(fixture.program), executors, inputs).with_auth(Arc::new(Issuer));
    let bus = EventBus::spawn();
    let mut feed = bus.subscribe(10_000);
    let executor = Executor::new(Arc::new(cx), RunOptions::default());
    executor.start(plan, bus.clone()).unwrap().finish().await.unwrap();
    bus.flush().await;
    let events: Vec<Event> = std::iter::from_fn(|| feed.try_recv().ok())
        .filter_map(|feed| if let Feed::Event(sequenced) = feed { Some(sequenced.event) } else { None })
        .collect();
    let checked = events.iter().any(|event| matches!(event, Event::CheckEvaluated { .. }));
    let shown = format!("{events:?}");
    for leak in LEAKS {
        assert!(!shown.contains(leak), "{leak} leaked");
    }
    assert!(checked && shown.contains(crate::MASKED));
}
