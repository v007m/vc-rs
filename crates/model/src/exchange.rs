use std::fmt;
use std::net::SocketAddr;

use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode, Uri};

/// A request with every template rendered, ready to send (plain data, D111).
///
/// `Debug` prints header names and the body length, never header values or body bytes, because
/// either can carry a secret before the redactor sees it.
#[derive(Clone, PartialEq, Eq)]
pub struct RenderedRequest {
    /// The HTTP method.
    pub method: Method,
    /// The absolute URL: scheme, host, optional port, path and query.
    pub uri: Uri,
    /// The headers to send. Transport-managed headers (`host`, `content-length`) are added on send.
    pub headers: HeaderMap,
    /// The body, empty for none.
    pub body: Bytes,
}

impl RenderedRequest {
    /// A request with no headers and no body.
    #[must_use]
    pub fn new(method: Method, uri: Uri) -> Self {
        Self { method, uri, headers: HeaderMap::new(), body: Bytes::new() }
    }
}

impl fmt::Debug for RenderedRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RenderedRequest")
            .field("method", &self.method)
            .field("uri", &self.uri)
            .field("headers", &HeaderNames(&self.headers))
            .field("body_len", &self.body.len())
            .finish()
    }
}

/// A response as received, its body decoded and capped (plain data, D111).
///
/// `Debug` hides header values and body bytes, like [`RenderedRequest`].
#[derive(Clone, PartialEq, Eq)]
pub struct HttpResponse {
    /// The status code.
    pub status: StatusCode,
    /// The response headers, `content-encoding` included as sent.
    pub headers: HeaderMap,
    /// The body after decoding (gzip, br, zstd).
    pub body: Bytes,
    /// Bytes received on the wire for the body, before decoding.
    pub wire_size: u64,
}

impl fmt::Debug for HttpResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpResponse")
            .field("status", &self.status)
            .field("headers", &HeaderNames(&self.headers))
            .field("body_len", &self.body.len())
            .field("wire_size", &self.wire_size)
            .finish()
    }
}

/// Prints the names in a header map, never the values.
struct HeaderNames<'a>(&'a HeaderMap);

impl fmt::Debug for HeaderNames<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.0.keys().map(http::HeaderName::as_str)).finish()
    }
}

/// How long each phase of one exchange took.
///
/// `dns`, `connect` and `tls` are `None` when the phase did not happen: a reused connection, an
/// IP-literal host (no DNS), or plain HTTP (no TLS).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Timing {
    /// Resolving the host name.
    pub dns: Option<std::time::Duration>,
    /// Opening the TCP connection.
    pub connect: Option<std::time::Duration>,
    /// The TLS handshake.
    pub tls: Option<std::time::Duration>,
    /// From the start of the send to the response head.
    pub first_byte: std::time::Duration,
    /// Reading and decoding the body after the head.
    pub download: std::time::Duration,
    /// The whole exchange.
    pub total: std::time::Duration,
}

/// The HTTP version an exchange used.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Protocol {
    /// HTTP/1.1 (or 1.0).
    #[default]
    Http1,
    /// HTTP/2, negotiated by ALPN.
    Http2,
}

/// The connection an exchange went over.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConnectionInfo {
    /// `true` when a pooled connection was reused, so no DNS, connect or TLS time was spent.
    pub reused: bool,
    /// The negotiated protocol.
    pub protocol: Protocol,
    /// The peer address, when known (a proxy's address when sent through one).
    pub remote: Option<SocketAddr>,
}

/// One request and its response, with timings: what a `Transport` returns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exchange {
    /// The request as sent, after hooks and auth.
    pub request: RenderedRequest,
    /// The response.
    pub response: HttpResponse,
    /// Per-phase timings.
    pub timing: Timing,
    /// The connection used.
    pub connection: ConnectionInfo,
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use http::header::{AUTHORIZATION, CONTENT_TYPE};
    use http::{HeaderMap, HeaderValue, Method, StatusCode, Uri};

    use super::{HttpResponse, RenderedRequest};

    fn secret_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer s3cret"));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers
    }

    #[test]
    fn debug_shows_header_names_but_never_values_or_body() {
        let mut request = RenderedRequest::new(Method::POST, Uri::from_static("http://h/p"));
        request.headers = secret_headers();
        request.body = Bytes::from_static(b"password=hunter2");
        let text = format!("{request:?}");
        assert!(text.contains("authorization") && text.contains("content-type"), "{text}");
        assert!(text.contains("body_len: 16"), "{text}");
        assert!(!text.contains("s3cret") && !text.contains("hunter2"), "{text}");
    }

    #[test]
    fn response_debug_hides_values_too() {
        let body = Bytes::from_static(b"token=abc");
        let response = HttpResponse { status: StatusCode::OK, headers: secret_headers(), body, wire_size: 7 };
        let text = format!("{response:?}");
        assert!(text.contains("200") && text.contains("authorization") && text.contains("wire_size: 7"), "{text}");
        assert!(!text.contains("s3cret") && !text.contains("token=abc"), "{text}");
    }

    #[test]
    fn a_new_request_has_no_headers_or_body() {
        let request = RenderedRequest::new(Method::GET, Uri::from_static("http://h/"));
        assert!(request.headers.is_empty() && request.body.is_empty());
    }
}
