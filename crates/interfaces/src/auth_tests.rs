use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use serde_json::{Map, Value, json};
use vc_diagnostics::Code;
use vc_model::RenderedRequest;

use super::{AuthError, AuthProvider, AuthRequest, Credentials, RequestSigner, Signer};
use crate::{CertChoice, CertId, Secret};

/// Puts the time it signs at in the body.
struct Stamp;

impl RequestSigner for Stamp {
    fn sign(&self, mut request: RenderedRequest, now_ms: u64) -> Result<RenderedRequest, AuthError> {
        request.body = Bytes::from(now_ms.to_string());
        Ok(request)
    }
}

/// A provider that keeps the default answer to `signs`.
struct Plain;

#[async_trait]
impl AuthProvider for Plain {
    async fn credentials(&self, _: &AuthRequest) -> Result<Credentials, AuthError> {
        Ok(Credentials::default())
    }
}

fn settings() -> Map<String, Value> {
    let Value::Object(map) = json!({"clientId": "shop", "clientSecret": "s3cr3t-value"}) else { return Map::new() };
    map
}

#[test]
fn debug_shows_setting_keys_and_hides_credential_values() {
    let request = AuthRequest::new("svc", "oauth2ClientCredentials", settings());
    let shown = format!("{request:?}");
    assert!(shown.contains("clientSecret") && !shown.contains("s3cr3t-value"), "{shown}");
    assert_eq!(request.text("clientId"), Some("shop"));
    assert_eq!(request.text("missing"), None);
    let credentials = Credentials::header("authorization", "Bearer s3cr3t-value");
    assert!(!format!("{credentials:?}").contains("s3cr3t-value"));
}

#[test]
fn credentials_list_every_value_and_their_expiry() {
    let mut credentials = Credentials::query("api_key", "k1").expiring(Some(Duration::from_secs(5)));
    credentials.headers = Credentials::header("x-a", "h1").headers;
    credentials.cookies = Credentials::header("sid", "c1").headers;
    assert_eq!(credentials.values().collect::<Vec<_>>(), ["h1", "k1", "c1"]);
    assert_eq!(credentials.expires_in, Some(Duration::from_secs(5)));
}

#[test]
fn a_signer_signs_hides_its_values_and_equals_only_itself() {
    let stamp: Arc<dyn RequestSigner> = Arc::new(Stamp);
    let signer = Signer::new(Arc::clone(&stamp), vec![Secret::new("AKIDEXAMPLE"), Secret::new("wJalr-secret")]);
    let mut credentials = Credentials::signed(signer.clone());
    credentials.headers = Credentials::header("x-a", "h1").headers;
    assert_eq!(credentials.values().collect::<Vec<_>>(), ["h1", "AKIDEXAMPLE", "wJalr-secret"]);
    let shown = format!("{credentials:?}");
    assert!(!shown.contains("AKIDEXAMPLE") && !shown.contains("wJalr-secret") && shown.contains("Signer"), "{shown}");
    let request = RenderedRequest::new("GET".parse().unwrap(), "http://api.test/x".parse().unwrap());
    assert_eq!(signer.sign(request, 42).map(|request| request.body), Ok(Bytes::from("42")));
    assert_eq!(credentials.signer.as_ref(), Some(&signer), "a clone shares the signer");
    assert_ne!(Signer::new(stamp, Vec::new()), Signer::new(Arc::new(Stamp), Vec::new()));
}

#[test]
fn a_provider_does_not_sign_unless_it_says_so() {
    assert!(!Plain.signs(&AuthRequest::new("svc", "bearer", settings())));
}

#[test]
fn every_cause_is_v5114_with_its_own_help() {
    let auth = || "svc".to_owned();
    let causes = [
        AuthError::Rejected { auth: auth(), status: 401 },
        AuthError::Unreachable { auth: auth(), reason: "refused".into() },
        AuthError::Malformed { auth: auth(), missing: "access_token".into() },
        AuthError::Missing { auth: auth(), key: "tokenUrl".into() },
        AuthError::Unknown { kind: "kerberos".into() },
        AuthError::Browser { auth: auth(), reason: "state mismatch".into() },
        AuthError::Flow { auth: auth(), reason: "step login failed".into() },
        AuthError::Plugin { auth: auth(), reason: "no auth plugin `acmeSso` is ready".into() },
    ];
    assert!(causes.iter().all(|cause| cause.code() == Code::V5114));
    let mut helps: Vec<_> = causes.iter().map(AuthError::help).collect();
    helps.sort_unstable();
    helps.dedup();
    assert_eq!(helps.len(), causes.len());
    assert_eq!(causes[0].to_string(), "the token endpoint for `svc` answered 401");
}

#[test]
fn a_refusal_is_told_from_a_failure_that_may_pass() {
    let rejected = |status| AuthError::Rejected { auth: "svc".into(), status };
    for status in [400, 401, 403] {
        assert!(rejected(status).refused() && !rejected(status).may_pass(), "{status}");
    }
    for status in [408, 429, 500, 503, 599] {
        assert!(rejected(status).may_pass() && !rejected(status).refused(), "{status}");
    }
    let unreachable = AuthError::Unreachable { auth: "svc".into(), reason: "timed out".into() };
    assert!(unreachable.may_pass() && !unreachable.refused());
    let flow = AuthError::Flow { auth: "svc".into(), reason: "step login failed".into() };
    assert!(!rejected(404).refused() && !rejected(404).may_pass() && !flow.refused() && !flow.may_pass());
}

#[test]
fn a_request_lets_the_host_pick_its_client_certificate_unless_given_one() {
    let request = AuthRequest::new("svc", "oauth2ClientCredentials", settings());
    assert_eq!(request.client_cert, CertChoice::Host);
    let named = request.with_client_cert(CertChoice::Named(CertId::new(2)));
    assert_eq!(named.client_cert, CertChoice::Named(CertId::new(2)));
    assert!(format!("{named:?}").contains("client_cert: Named"), "{named:?}");
}
