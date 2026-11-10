//! Helpers the unit tests share: requests, a token endpoint and the form a request sent.

use std::sync::Arc;

use bytes::Bytes;
use serde_json::Value;
use vc_interfaces::AuthRequest;
use vc_model::{HttpResponse, RenderedRequest};
use vc_testkit::{FakeTransport, RecordingTransport, Recordings, status};

/// What the fake endpoint's `/token` answers.
pub const TOKEN: &str =
    r#"{"access_token":"t0k3n-abc","token_type":"bearer","expires_in":60,"refresh_token":"r3fresh-1"}"#;

/// A request for definition `api` of type `kind`, with the object `settings`.
pub fn request(kind: &str, settings: &Value) -> AuthRequest {
    AuthRequest::new("api", kind, settings.as_object().cloned().unwrap_or_default())
}

/// A response with `code` and `body`.
pub fn answering(code: u16, body: &str) -> HttpResponse {
    HttpResponse { body: Bytes::from(body.to_owned()), ..status(code) }
}

/// A token endpoint: `/token` answers `body`, `/refuse` 401, `/busy` 503 and other paths 404.
pub fn token_server(body: &'static str) -> (Arc<RecordingTransport>, Recordings) {
    let fake = FakeTransport::new(move |request: &RenderedRequest, _| {
        let path = request.uri.path();
        let answer = if path == "/token" { answering(200, body) } else { status(404) };
        let answer = if path == "/busy" { status(503) } else { answer };
        Ok(if path == "/refuse" { status(401) } else { answer })
    });
    let (recording, recordings) = RecordingTransport::new(Arc::new(fake));
    (Arc::new(recording), recordings)
}

/// The form fields `request` sent, decoded.
pub fn form(request: &RenderedRequest) -> Vec<(String, String)> {
    let body = String::from_utf8_lossy(&request.body).into_owned();
    body.split('&').filter_map(|pair| pair.split_once('=')).map(|(key, value)| (decode(key), decode(value))).collect()
}

/// The value of `key` among `pairs`.
pub fn field<'p>(pairs: &'p [(String, String)], key: &str) -> Option<&'p str> {
    pairs.iter().find(|(name, _)| name == key).map(|(_, value)| value.as_str())
}

fn decode(text: &str) -> String {
    percent_encoding::percent_decode_str(text).decode_utf8_lossy().into_owned()
}
