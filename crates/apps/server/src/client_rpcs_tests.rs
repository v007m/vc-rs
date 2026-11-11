use std::fs;
use std::sync::Arc;

use prost::Message as _;
use vc_contracts::engine::v1 as wire;

use super::explain_api;
use crate::Code;
use crate::authoring_rpcs::call;
use crate::engine_api::EngineApi;
use crate::engine_fixture::{API, CONFIG, SECRET, USERS, project, serving};

fn digest_of(api: &Arc<EngineApi>) -> String {
    api.engine().read_file(API).unwrap().digest
}

fn leaks(response: &impl prost::Message) -> bool {
    let bytes = response.encode_to_vec();
    bytes.windows(SECRET.len()).any(|window| window == SECRET.as_bytes())
}

fn set(pointer: &str, value_json: &str) -> wire::PointerEdit {
    wire::PointerEdit { pointer: pointer.into(), op: wire::EditOp::Set.into(), value_json: value_json.into() }
}

fn drafted(path: &str, digest: String, edits: Vec<wire::PointerEdit>) -> wire::CallRequest {
    let draft = wire::ApiDraft { path: path.into(), digest, edits };
    wire::CallRequest { api: "users#list".into(), draft: Some(draft), ..wire::CallRequest::default() }
}

#[tokio::test]
async fn explains_an_api_as_its_file_writes_it_with_its_place_and_its_secret_masked() {
    let (dir, api) = project();
    let keyed = USERS.replace("    method: POST\n", &format!("    method: POST\n    headers: {{X-Key: {SECRET}}}\n"));
    fs::write(dir.path().join(API), keyed).unwrap();
    let request = wire::ExplainApiRequest { api: "users#create".into(), ..wire::ExplainApiRequest::default() };
    let explained = explain_api(api.clone(), request).await.unwrap();
    assert_eq!((explained.path.as_str(), explained.pointer.as_str()), (API, "/apis/1"));
    assert_eq!(explained.digest, digest_of(&api));
    let written: serde_json::Value = serde_json::from_str(&explained.api_json).unwrap();
    assert_eq!((written["method"].as_str(), written["headers"]["X-Key"].as_str()), (Some("POST"), Some("••••")));
    assert!(!leaks(&explained) && !format!("{explained:?}").contains(SECRET));
    let request = wire::ExplainApiRequest { api: "users#gone".into(), ..wire::ExplainApiRequest::default() };
    assert_eq!(explain_api(api, request).await.unwrap_err().code, Code::InvalidArgument);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_call_with_a_draft_sends_the_api_as_edited_and_writes_nothing() {
    let (dir, api) = project();
    let config = CONFIG.replace("http://127.0.0.1:9", &serving(200, b"[]".to_vec()));
    fs::write(dir.path().join("vibranium.config.yaml"), config).unwrap();
    let edits = vec![set("/apis/0/path", "\"/people\""), set("/apis/0/method", "\"PUT\"")];
    let sent = call(api.clone(), drafted(API, digest_of(&api), edits)).await.unwrap();
    let exchange = sent.exchange.unwrap();
    assert_eq!(exchange.method, "PUT");
    assert!(exchange.url.ends_with("/people"), "{}", exchange.url);
    assert_eq!(fs::read_to_string(dir.path().join(API)).unwrap(), USERS, "the file is as it was");
    let plain = call(api.clone(), wire::CallRequest { api: "users#list".into(), ..wire::CallRequest::default() }).await;
    assert!(plain.unwrap().exchange.unwrap().url.ends_with("/users"), "the next call sends the file");
}

#[tokio::test]
async fn a_draft_over_a_changed_file_or_without_an_op_is_refused() {
    let (dir, api) = project();
    let digest = digest_of(&api);
    let opless = wire::PointerEdit { pointer: "/apis/0/path".into(), ..wire::PointerEdit::default() };
    let refused = call(api.clone(), drafted(API, digest.clone(), vec![opless])).await.unwrap_err();
    assert_eq!(refused.code, Code::InvalidArgument, "{refused:?}");
    assert!(refused.message.contains("edit 1 needs an op"), "{refused:?}");
    fs::write(dir.path().join(API), USERS.replace("/users\n", "/members\n")).unwrap();
    let stale = call(api.clone(), drafted(API, digest, vec![set("/apis/0/path", "\"/x\"")])).await.unwrap_err();
    assert_eq!(stale.code, Code::Aborted, "{stale:?}");
}

#[tokio::test]
async fn a_draft_never_reads_a_file_the_editor_does_not_open() {
    let (dir, api) = project();
    fs::create_dir_all(dir.path().join(".vibranium")).unwrap();
    fs::write(dir.path().join(".vibranium/x.api.yaml"), USERS).unwrap();
    for path in [".env", ".git/config", ".vibranium/x.api.yaml", "../users.api.yaml", "/etc/hosts"] {
        let refused = call(api.clone(), drafted(path, String::new(), Vec::new())).await.unwrap_err();
        assert_eq!(refused.code, Code::InvalidArgument, "{path}: {refused:?}");
        assert!(!format!("{refused:?}").contains(SECRET), "{path}: {refused:?}");
    }
    assert_eq!(fs::read_to_string(dir.path().join(".env")).unwrap(), format!("SHOP_TOKEN={SECRET}\n"));
}
