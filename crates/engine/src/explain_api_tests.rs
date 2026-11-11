use std::sync::Arc;

use vc_executor::MASKED;
use vc_testkit::{FakeCommands, FakeVcs, MemoryEnvironment, MemorySources};

use super::ApiExplained;
use crate::sample::builder;
use crate::{CallDraft, CallRequest, EditOp, Engine, EngineError, PointerEdit};

const SECRET: &str = "tok-explain-7d20c4";
const SCHEMA: (&str, &[u8]) = (".env.schema", b"SHOP_TOKEN=\nSHOP_CLIENT_ID=\n");
const CONFIG: (&str, &[u8]) = (
    "vibranium.config.yaml",
    b"syntaxVersion: 2
defaults:
  headers:
    accept: application/json
    x-key: ${var.shopKey}
vars:
  usersUrl: http://localhost:8080
  shopKey: tok-explain-7d20c4
  region: us
  unused: nothing
profiles:
  - id: dev
    default: true
    vars:
      usersUrl: http://dev.local
auth:
  - id: shopOauth
    type: oauth2ClientCredentials
    tokenUrl: ${var.usersUrl}/oauth/token
    clientId: ${secret.SHOP_CLIENT_ID}
    clientSecret: ${secret.SHOP_TOKEN}
",
);
const USERS: (&str, &[u8]) = (
    "services/users/users.api.yaml",
    b"baseUrl: ${var.usersUrl}/api
auth: shopOauth
headers:
  x-trace: ${var.region}
apis:
  - id: get
    path: /users/${var.userId}
    query: {lang: en}
    vars: {userId: 1}
    headers:
      accept: text/plain
  - id: open
    path: /health
    auth: none
    headers:
      x-note: it's
",
);

fn project() -> Engine {
    project_with(CONFIG.1)
}

fn project_with(config: &[u8]) -> Engine {
    let environment = [("SHOP_TOKEN", SECRET), ("SHOP_CLIENT_ID", "client-1")];
    builder(FakeVcs::default())
        .sources(Arc::new(MemorySources::new(&[(CONFIG.0, config), SCHEMA, USERS])))
        .environment(Arc::new(MemoryEnvironment::new(&environment)))
        .commands(Arc::new(FakeCommands::new(|_| None)))
        .build()
        .unwrap()
}

fn request(api: &str, vars: &[&str], sets: &[&str]) -> CallRequest {
    CallRequest {
        api: api.into(),
        vars: vars.iter().map(|var| (*var).to_owned()).collect(),
        sets: sets.iter().map(|set| (*set).to_owned()).collect(),
        ..CallRequest::default()
    }
}

fn layers(explained: &ApiExplained, name: &str) -> Vec<(String, String)> {
    let var = explained.vars.iter().find(|var| var.key == name).unwrap();
    var.layers.iter().map(|layer| (layer.value.clone(), layer.source.clone())).collect()
}

fn pairs(layers: &[(&str, &str)]) -> Vec<(String, String)> {
    layers.iter().map(|(value, source)| ((*value).to_owned(), (*source).to_owned())).collect()
}

#[tokio::test]
async fn gives_each_var_the_api_uses_with_its_layers_the_value_in_force_first() {
    let explained = project().explain_api(&request("users#get", &["userId=7"], &["vars.region=eu"])).await.unwrap();
    let names: Vec<&str> = explained.vars.iter().map(|var| var.key.as_str()).collect();
    assert_eq!(names, ["usersUrl", "shopKey", "region", "userId"], "an unused var is left out");
    let users_url = pairs(&[("http://dev.local", "profile dev"), ("http://localhost:8080", "vibranium.config.yaml")]);
    assert_eq!(layers(&explained, "usersUrl"), users_url);
    assert_eq!(layers(&explained, "region"), pairs(&[("eu", "--set"), ("us", "vibranium.config.yaml")]));
    assert_eq!(layers(&explained, "userId"), pairs(&[("7", "call"), ("1", "api users#get")]));
    assert_eq!(layers(&explained, "shopKey"), pairs(&[(MASKED, "vibranium.config.yaml")]));
}

#[tokio::test]
async fn gives_the_headers_it_inherits_masked_and_its_auth_by_name_never_a_secret() {
    let explained = project().explain_api(&request("users#get", &["userId=7"], &["vars.region=eu"])).await.unwrap();
    let headers: Vec<(&str, &str, &str)> =
        explained.headers.iter().map(|h| (h.name.as_str(), h.value.as_str(), h.from.as_str())).collect();
    assert_eq!(headers, [("x-key", MASKED, "vibranium.config.yaml"), ("x-trace", "eu", "file users")]);
    let auth = explained.auth.as_ref().unwrap();
    assert_eq!((auth.id.as_str(), auth.kind.as_str()), ("shopOauth", "oauth2ClientCredentials"));
    assert_eq!(auth.token_url.as_deref(), Some("${var.usersUrl}/oauth/token"));
    assert_eq!(auth.secrets, ["SHOP_CLIENT_ID", "SHOP_TOKEN"]);
    assert!(!format!("{explained:?}").contains(SECRET), "{explained:?}");
}

#[tokio::test]
async fn gives_a_curl_command_with_every_header_masked_and_names_the_auth_vc_applies() {
    let explained = project().explain_api(&request("users#get", &["userId=7"], &["vars.region=eu"])).await.unwrap();
    let curl = "# auth shopOauth (oauth2ClientCredentials) is applied by vc
curl -X GET 'http://dev.local/api/users/7?lang=en' \\
  -H 'accept: text/plain' \\
  -H 'x-key: ••••' \\
  -H 'x-trace: eu'";
    assert_eq!(explained.curl, curl);
    let open = project().explain_api(&request("users#open", &[], &[])).await.unwrap();
    assert_eq!(open.auth, None, "`auth: none` sends no auth");
    assert!(open.curl.starts_with("curl -X GET"), "{}", open.curl);
    assert!(open.curl.ends_with("-H 'x-note: it'\\''s'"), "{}", open.curl);
    assert!(open.headers.iter().all(|header| header.name != "x-note"), "the api writes it itself");
}

#[tokio::test]
async fn an_unknown_api_or_a_var_that_is_not_name_value_is_refused() {
    let engine = project();
    let unknown = engine.explain_api(&request("users#gte", &[], &[])).await.unwrap_err();
    assert!(matches!(&unknown, EngineError::Selection { help: Some(help), .. } if help.contains("users#get")));
    let bad = engine.explain_api(&request("users#get", &["userId=1", SECRET], &[])).await.unwrap_err();
    assert!(matches!(&bad, EngineError::Setting { reason, .. } if reason == "var 2 is not `name=value`"), "{bad:?}");
}

#[tokio::test]
async fn an_api_key_auth_is_named_never_sent_as_a_bearer_header() {
    let config = String::from_utf8_lossy(CONFIG.1).replace(
        "    type: oauth2ClientCredentials\n    tokenUrl: ${var.usersUrl}/oauth/token\n    clientId: ${secret.SHOP_CLIENT_ID}\n    clientSecret: ${secret.SHOP_TOKEN}\n",
        "    type: apiKey\n    in: query\n    name: key\n    value: ${secret.SHOP_TOKEN}\n",
    );
    let engine = project_with(config.as_bytes());
    let explained = engine.explain_api(&request("users#get", &[], &[])).await.unwrap();
    assert!(explained.curl.starts_with("# auth shopOauth (apiKey) is applied by vc\ncurl "), "{}", explained.curl);
    assert!(!explained.curl.contains("Authorization"), "{}", explained.curl);
}

#[tokio::test]
async fn a_var_or_query_value_named_in_redact_fields_fills_the_url_masked() {
    let config = format!("{}redact:\n  fields: [lang, userid]\n", String::from_utf8_lossy(CONFIG.1));
    let engine = project_with(config.as_bytes());
    let explained = engine.explain_api(&request("users#get", &["userId=7"], &[])).await.unwrap();
    let url = explained.curl.lines().nth(1).unwrap();
    assert_eq!(url, "curl -X GET 'http://dev.local/api/users/••••?lang=••••' \\", "{}", explained.curl);
}

#[tokio::test]
async fn explains_the_draft_a_call_would_send_in_place_of_the_file() {
    let engine = project();
    let digest = engine.read_file(USERS.0).unwrap().digest;
    let edits = vec![PointerEdit {
        pointer: "/apis/0/path".into(),
        op: EditOp::Set,
        value: "\"/people/${var.userId}\"".into(),
    }];
    let mut drafted = request("users#get", &["userId=7"], &[]);
    drafted.draft = Some(CallDraft { path: USERS.0.into(), digest, edits });
    let explained = engine.explain_api(&drafted).await.unwrap();
    assert!(explained.curl.contains("'http://dev.local/api/people/7?lang=en'"), "{}", explained.curl);
    drafted.draft = Some(CallDraft { digest: "stale".into(), ..drafted.draft.unwrap() });
    assert!(matches!(engine.explain_api(&drafted).await.unwrap_err(), EngineError::Stale { .. }));
}
