//! HTTP adapters: a reqwest-backed [`Transport`] and a static bearer-token [`AuthProvider`].
use async_trait::async_trait;
use ir::{HeaderName, Method, StatusCode};
use ports::{AuthProvider, HttpRequest, HttpResponse, PortError, PortErrorKind, Transport};
use serde_json::Value;

fn transport_error(e: &reqwest::Error) -> PortError {
    PortError::new(PortErrorKind::Transport, e.to_string())
}

pub struct ReqwestTransport {
    client: reqwest::Client,
    base_url: String,
}

impl ReqwestTransport {
    /// # Errors
    /// If the HTTP client cannot be constructed.
    pub fn new(base_url: impl Into<String>) -> Result<Self, PortError> {
        let client = reqwest::Client::builder().no_proxy().build().map_err(|e| transport_error(&e))?;
        Ok(Self { client, base_url: base_url.into() })
    }
}

const fn reqwest_method(method: Method) -> reqwest::Method {
    match method {
        Method::Get => reqwest::Method::GET,
        Method::Post => reqwest::Method::POST,
        Method::Put => reqwest::Method::PUT,
        Method::Patch => reqwest::Method::PATCH,
        Method::Delete => reqwest::Method::DELETE,
    }
}

fn parse_body(bytes: &[u8]) -> Value {
    if bytes.is_empty() {
        return Value::Null;
    }
    serde_json::from_slice(bytes).unwrap_or_else(|_| Value::String(String::from_utf8_lossy(bytes).into_owned()))
}

#[async_trait]
impl Transport for ReqwestTransport {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, PortError> {
        let url = format!("{}{}", self.base_url, request.path);
        let mut builder = self.client.request(reqwest_method(request.method), url);
        for (name, value) in &request.headers {
            builder = builder.header(name.as_str(), value);
        }
        if let Some(body) = &request.body {
            builder = builder.header("content-type", "application/json").body(body.to_string());
        }
        let response = builder.send().await.map_err(|e| transport_error(&e))?;
        let status = StatusCode::new(response.status().as_u16());
        let headers = response
            .headers()
            .iter()
            .map(|(k, v)| (HeaderName::new(k.as_str()), String::from_utf8_lossy(v.as_bytes()).into_owned()))
            .collect();
        let bytes = response.bytes().await.map_err(|e| transport_error(&e))?;
        Ok(HttpResponse { status, headers, body: parse_body(&bytes) })
    }
}

/// Adds `authorization: Bearer <token>` to every request.
pub struct StaticTokenAuth {
    token: String,
}

impl StaticTokenAuth {
    #[must_use]
    pub fn new(token: impl Into<String>) -> Self {
        Self { token: token.into() }
    }
}

#[async_trait]
impl AuthProvider for StaticTokenAuth {
    async fn authorize(&self, mut request: HttpRequest) -> Result<HttpRequest, PortError> {
        request.headers.push((HeaderName::new("authorization"), format!("Bearer {}", self.token)));
        Ok(request)
    }
}
