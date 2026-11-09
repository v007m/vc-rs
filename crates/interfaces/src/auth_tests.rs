use std::time::Duration;

use serde_json::{Map, Value, json};
use vc_diagnostics::Code;

use super::{AuthError, AuthRequest, Credentials};

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
