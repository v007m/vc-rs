//! Posting a channel's JSON and reading whether it was taken.

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use http::header::CONTENT_TYPE;
use http::{HeaderValue, Method, Uri};
use serde_json::{Map, Value};
use vc_interfaces::{Notice, Notifier, NotifyError, SendContext, Transport};
use vc_model::RenderedRequest;

use crate::registry::Payload;

/// A notifier that posts its channel's [`Payload`] to one URL.
pub struct HttpNotifier {
    /// The channel kind, for errors.
    pub channel: String,
    /// Where to post.
    pub url: Uri,
    /// The channel's settings.
    pub settings: Map<String, Value>,
    /// What to post.
    pub payload: Arc<dyn Payload>,
    /// How to post.
    pub transport: Arc<dyn Transport>,
}

#[async_trait]
impl Notifier for HttpNotifier {
    async fn notify(&self, notice: &Notice) -> Result<(), NotifyError> {
        let body = self.payload.body(notice, &self.settings);
        let mut request = RenderedRequest::new(Method::POST, self.url.clone());
        request.headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        request.body = Bytes::from(body.to_string());
        let failed = |reason: String| NotifyError::Send { channel: self.channel.clone(), reason };
        let exchange = self.transport.send(request, &SendContext::default()).await;
        let exchange = exchange.map_err(|error| failed(self.scrub(&error.to_string())))?;
        let status = exchange.response.status;
        if status.is_success() { Ok(()) } else { Err(failed(format!("HTTP {}", status.as_u16()))) }
    }
}

impl HttpNotifier {
    /// `reason` without the URL's path and query, which may be a secret.
    fn scrub(&self, reason: &str) -> String {
        let secret = self.url.path_and_query().map(http::uri::PathAndQuery::as_str).filter(|path| path.len() > 1);
        secret.map_or_else(|| reason.to_owned(), |secret| reason.replace(secret, "/…"))
    }
}
