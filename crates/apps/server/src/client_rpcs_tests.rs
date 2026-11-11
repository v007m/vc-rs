use std::fs;
use std::sync::Arc;

use vc_contracts::engine::v1 as wire;

use super::{explain_api, read_definition};
use crate::Code;
use crate::authoring_rpcs::call;
use crate::engine_api::EngineApi;
use crate::engine_fixture::{API, CONFIG, SECRET, TESTS, USERS, project, serving};

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

fn read(id: &str) -> wire::ReadDefinitionRequest {
    wire::ReadDefinitionRequest { id: id.into() }
}

#[tokio::test]
async fn reads_an_api_as_its_file_writes_it_with_its_place_and_its_secret_masked() {
    let (dir, api) = project();
    let keyed = USERS.replace("    method: POST\n", &format!("    method: POST\n    headers: {{X-Key: {SECRET}}}\n"));
    fs::write(dir.path().join(API), keyed).unwrap();
    let read = read_definition(api.clone(), read("users#create")).await.unwrap();
    assert_eq!((read.path.as_str(), read.pointer.as_str()), (API, "/apis/1"));
    assert_eq!(read.digest, digest_of(&api));
    let written: serde_json::Value = serde_json::from_str(&read.json).unwrap();
    assert_eq!((written["method"].as_str(), written["headers"]["X-Key"].as_str()), (Some("POST"), Some("••••")));
    assert!(!leaks(&read) && !format!("{read:?}").contains(SECRET));
}

#[tokio::test]
async fn reads_a_flow_test_with_its_steps_and_its_secret_masked() {
    let (dir, api) = project();
    let flow = format!(
        "tests:\n  - id: listsTwice\n    steps:\n      - call: users#list\n        headers: {{X-Key: {SECRET}}}\n      - call: users#list\n        repeat: 2\n"
    );
    fs::write(dir.path().join(TESTS), flow).unwrap();
    let read = read_definition(api.clone(), read("users.crud#listsTwice")).await.unwrap();
    assert_eq!((read.path.as_str(), read.pointer.as_str()), (TESTS, "/tests/0"));
    let written: serde_json::Value = serde_json::from_str(&read.json).unwrap();
    assert_eq!(written["steps"][0]["headers"]["X-Key"].as_str(), Some("••••"));
    assert_eq!(written["steps"][1]["repeat"].as_u64(), Some(2));
    assert!(!leaks(&read) && !format!("{read:?}").contains(SECRET));
}

#[tokio::test]
async fn an_id_of_no_api_or_test_reads_nothing_and_never_names_a_path_outside_the_root() {
    let (dir, api) = project();
    fs::create_dir_all(dir.path().join(".vibranium")).unwrap();
    fs::write(dir.path().join(".vibranium/x.api.yaml"), USERS).unwrap();
    let ids =
        ["users#gone", "users", "users.crud", "vibranium.config", ".env", "../users#list", "x#list", "/etc/hosts"];
    for id in ids {
        let refused = read_definition(api.clone(), read(id)).await.unwrap_err();
        assert_eq!(refused.code, Code::InvalidArgument, "{id}: {refused:?}");
        assert!(!format!("{refused:?}").contains(SECRET), "{id}: {refused:?}");
    }
}

fn explain(id: &str, vars: &[&str]) -> wire::ExplainApiRequest {
    let vars = vars.iter().map(|var| (*var).to_owned()).collect();
    wire::ExplainApiRequest { api: id.into(), vars, ..wire::ExplainApiRequest::default() }
}

#[tokio::test(flavor = "multi_thread")]
async fn explains_an_api_with_its_vars_inherited_headers_and_curl_every_secret_masked() {
    let (dir, api) = project();
    let keyed = USERS.replace("apis:\n", "headers:\n  X-Note: ${var.note}\napis:\n");
    fs::write(dir.path().join(API), keyed).unwrap();
    let explained = explain_api(api, explain("users#create", &["note=tok-fixture-5c2e81"])).await.unwrap();
    let keys: Vec<&str> = explained.vars.iter().map(|var| var.key.as_str()).collect();
    assert_eq!(keys, ["baseUrl", "note"]);
    let note: Vec<&str> = explained.vars[1].layers.iter().map(|layer| layer.source.as_str()).collect();
    assert_eq!(note, ["call", "vibranium.config.yaml"]);
    let header = &explained.headers[0];
    assert_eq!((header.name.as_str(), header.value.as_str(), header.from.as_str()), ("X-Note", "••••", "file users"));
    assert_eq!(explained.auth, None);
    assert_eq!(explained.curl, "curl -X POST 'http://127.0.0.1:9/users' \\\n  -H 'X-Note: ••••'");
    assert!(!leaks(&explained) && !format!("{explained:?}").contains(SECRET));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_id_of_no_api_explains_nothing_and_never_echoes_a_secret() {
    let (_dir, api) = project();
    for id in ["users#gone", "users.crud#listsUsers", ".env", "../users#list", "/etc/hosts"] {
        let refused = explain_api(api.clone(), explain(id, &[])).await.unwrap_err();
        assert_eq!(refused.code, Code::InvalidArgument, "{id}: {refused:?}");
        assert!(!format!("{refused:?}").contains(SECRET), "{id}: {refused:?}");
    }
    let bad = explain_api(api, explain("users#list", &["tok-fixture-5c2e81"])).await.unwrap_err();
    assert_eq!(bad.code, Code::InvalidArgument, "{bad:?}");
    assert!(!format!("{bad:?}").contains(SECRET), "{bad:?}");
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
