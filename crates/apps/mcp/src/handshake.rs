//! The stdio handshake. An `initialize` without `clientInfo` is served as one from `agent`, and
//! a client that sends something else before `initialize` ends the server with a user error.

use rmcp::RoleServer;
use rmcp::model::{ClientJsonRpcMessage, ClientRequest, JsonRpcMessage};
use rmcp::service::TxJsonRpcMessage;
use rmcp::transport::Transport;
use serde_json::{Map, Value};

use crate::McpError;

/// The agent's name when its client gives none.
pub const UNNAMED: &str = "agent";

/// A transport that fills in a missing `clientInfo`, or its `name` or `version`, in an
/// `initialize`, which rmcp would otherwise not read as one.
pub struct Named<T> {
    inner: T,
}

impl<T> Named<T> {
    /// `inner`, with an unnamed client's `initialize` filled in.
    pub const fn new(inner: T) -> Self {
        Self { inner }
    }
}

impl<T: Transport<RoleServer>> Transport<RoleServer> for Named<T> {
    type Error = T::Error;

    fn send(
        &mut self,
        item: TxJsonRpcMessage<RoleServer>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'static {
        self.inner.send(item)
    }

    async fn receive(&mut self) -> Option<ClientJsonRpcMessage> {
        self.inner.receive().await.map(named)
    }

    fn close(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.inner.close()
    }
}

/// `message`, with `clientInfo` filled in when it is an `initialize` that rmcp could not read
/// for want of it.
pub fn named(message: ClientJsonRpcMessage) -> ClientJsonRpcMessage {
    if !unread_initialize(&message) {
        return message;
    }
    with_client_info(&message).unwrap_or(message)
}

/// The error for a client that sent `sent` where `initialize` belonged.
pub fn broken(sent: Option<&ClientJsonRpcMessage>) -> McpError {
    let method = sent
        .and_then(|message| serde_json::to_value(message).ok())
        .and_then(|value| value.get("method")?.as_str().map(str::to_owned));
    let sent = match (sent, method) {
        (_, Some(method)) => format!("`{method}`"),
        (Some(_), None) => "a response".to_owned(),
        (None, None) => "nothing".to_owned(),
    };
    McpError::Handshake { sent }
}

fn unread_initialize(message: &ClientJsonRpcMessage) -> bool {
    matches!(message, JsonRpcMessage::Request(request)
        if matches!(&request.request, ClientRequest::CustomRequest(custom) if custom.method == "initialize"))
}

fn with_client_info(message: &ClientJsonRpcMessage) -> Option<ClientJsonRpcMessage> {
    let mut value = serde_json::to_value(message).ok()?;
    let params = value.get_mut("params")?.as_object_mut()?;
    let info = params.entry("clientInfo").or_insert_with(|| Value::Object(Map::new())).as_object_mut()?;
    info.entry("name").or_insert_with(|| UNNAMED.into());
    info.entry("version").or_insert_with(|| "".into());
    let filled: ClientJsonRpcMessage = serde_json::from_value(value).ok()?;
    let read = matches!(&filled, JsonRpcMessage::Request(request)
        if matches!(request.request, ClientRequest::InitializeRequest(_)));
    read.then_some(filled)
}

#[cfg(test)]
#[path = "handshake_tests.rs"]
mod tests;
