//! MCP over streamable HTTP, which `vc ui` mounts at `/mcp` behind its token, `Host` and `Origin`
//! checks (D197). The tools are the same as over stdio, and so is the handshake: an `initialize`
//! without `clientInfo`, or its `name` or `version`, is served as one from `agent`.

use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use bytes::Bytes;
use http::{Method, Request, Response, StatusCode, header};
use http_body::Body;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt as _, Full, LengthLimitError, Limited};
use rmcp::model::ClientJsonRpcMessage;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};

use crate::VcServer;
use crate::handshake::filled;

type Rmcp = StreamableHttpService<VcServer, LocalSessionManager>;

/// What the service answers with.
type HttpResponse = Response<BoxBody<Bytes, Infallible>>;

/// The MCP service the web server mounts at `/mcp`: rmcp's, with an unnamed client's
/// `initialize` filled in before rmcp reads it.
#[derive(Clone)]
pub struct HttpService {
    rmcp: Rmcp,
    max: usize,
}

impl VcServer {
    /// This server as MCP over streamable HTTP, and the call that ends its sessions and open
    /// streams so the web server can stop. rmcp's own `Host` check keeps its loopback default.
    pub fn into_http(self) -> (HttpService, impl FnOnce() + Send + 'static) {
        let config = StreamableHttpServerConfig::default();
        let (stop, max) = (config.cancellation_token.clone(), config.max_request_body_bytes);
        let rmcp = StreamableHttpService::new(move || Ok(self.clone()), Arc::default(), config);
        (HttpService { rmcp, max }, move || stop.cancel())
    }
}

impl<B> tower_service::Service<Request<B>> for HttpService
where
    B: Body<Data = Bytes> + Send + 'static,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>> + std::fmt::Display,
{
    type Response = HttpResponse;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<HttpResponse, Infallible>> + Send>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Infallible>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: Request<B>) -> Self::Future {
        let service = self.clone();
        Box::pin(async move { Ok(service.serve(request).await) })
    }
}

impl HttpService {
    /// rmcp's answer to `request`, a `POST`'s body read first, up to rmcp's own limit, so an
    /// unnamed client's `initialize` can be filled in.
    async fn serve<B>(&self, request: Request<B>) -> HttpResponse
    where
        B: Body<Data = Bytes> + Send + 'static,
        B::Error: Into<Box<dyn std::error::Error + Send + Sync>> + std::fmt::Display,
    {
        if request.method() != Method::POST {
            return self.rmcp.handle(request).await;
        }
        let (mut parts, body) = request.into_parts();
        let read = match Limited::new(body, self.max).collect().await {
            Ok(read) => read.to_bytes(),
            Err(error) => return self.unread(&*error),
        };
        let body = match serde_json::from_slice::<ClientJsonRpcMessage>(&read).ok().as_ref().and_then(filled) {
            Some(message) => {
                parts.headers.remove(header::CONTENT_LENGTH);
                serde_json::to_vec(&message).map_or(read, Bytes::from)
            }
            None => read,
        };
        self.rmcp.handle(Request::from_parts(parts, Full::new(body))).await
    }

    /// The answer to a body that could not be read: too large, as rmcp words it, or broken off.
    fn unread(&self, error: &(dyn std::error::Error + 'static)) -> HttpResponse {
        let (status, text) = if error.is::<LengthLimitError>() {
            (StatusCode::PAYLOAD_TOO_LARGE, format!("Payload Too Large: request body exceeds {} bytes", self.max))
        } else {
            (StatusCode::BAD_REQUEST, "Bad Request: the request body could not be read".to_owned())
        };
        let mut response = Response::new(Full::new(Bytes::from(text)).boxed());
        *response.status_mut() = status;
        response
    }
}

#[cfg(test)]
#[path = "http_tests.rs"]
mod tests;
