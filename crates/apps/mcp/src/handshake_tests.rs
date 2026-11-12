use rmcp::model::{ClientJsonRpcMessage, ClientRequest, JsonRpcMessage};
use serde_json::{Value, json};

use super::{broken, named};

fn message(value: Value) -> ClientJsonRpcMessage {
    serde_json::from_value(value).unwrap()
}

fn initialize(params: Value) -> ClientJsonRpcMessage {
    let mut request = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize"});
    request["params"] = params;
    message(request)
}

/// The client name rmcp reads from `message`, when it reads it as an `initialize`.
fn client(message: &ClientJsonRpcMessage) -> Option<(String, String)> {
    match message {
        JsonRpcMessage::Request(request) => match &request.request {
            ClientRequest::InitializeRequest(init) => {
                Some((init.params.client_info.name.clone(), init.params.client_info.version.clone()))
            }
            _ => None,
        },
        _ => None,
    }
}

#[test]
fn an_initialize_without_client_info_is_read_as_agent() {
    let unnamed = initialize(json!({"protocolVersion": "2025-11-25", "capabilities": {}}));
    assert_eq!(client(&unnamed), None, "rmcp alone does not read it as an initialize");
    assert_eq!(client(&named(unnamed)), Some(("agent".to_owned(), String::new())));
}

#[test]
fn a_client_info_without_a_name_or_version_is_filled_in() {
    let nameless = initialize(json!({"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {}}));
    assert_eq!(client(&named(nameless)), Some(("agent".to_owned(), String::new())));
    let versionless =
        initialize(json!({"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "claude"}}));
    assert_eq!(client(&named(versionless)), Some(("claude".to_owned(), String::new())));
}

#[test]
fn a_named_client_and_other_messages_pass_as_they_came() {
    let info = json!({"name": "claude", "version": "1.0.0"});
    let full = initialize(json!({"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": info}));
    assert_eq!(client(&named(full)), Some(("claude".to_owned(), "1.0.0".to_owned())));
    let list = message(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}));
    let passed = serde_json::to_value(named(list.clone())).unwrap();
    assert_eq!(passed, serde_json::to_value(list).unwrap());
}

#[test]
fn an_initialize_missing_more_than_the_client_stays_unread() {
    let bare = initialize(json!({"clientInfo": {"name": "claude", "version": "1"}}));
    assert_eq!(client(&named(bare)), None);
    let no_params = message(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize"}));
    assert_eq!(client(&named(no_params)), None);
}

#[test]
fn a_broken_handshake_names_what_the_client_sent() {
    let initialized = message(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    let error = broken(Some(&initialized)).to_string();
    assert_eq!(
        error,
        "the MCP client broke the handshake: it sent `notifications/initialized` before an `initialize` was answered"
    );
    let response = message(json!({"jsonrpc": "2.0", "id": 1, "result": {}}));
    assert!(broken(Some(&response)).to_string().contains("it sent a response before"));
    assert!(broken(None).to_string().contains("it sent nothing before"));
}
