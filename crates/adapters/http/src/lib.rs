//! HTTP adapter: hyper and rustls transport with phase timings.
//!
//! [`HttpTransport`] implements `vc_interfaces::Transport` with hyper's pooled client over a
//! connector that times DNS (the system resolver, through tokio), TCP connect, the proxy tunnel
//! and the TLS handshake (rustls with ring, the OS certificate store plus extra roots, webpki
//! verification always on, ALPN h2 and HTTP/1.1). [`TlsOptions`] adds PEM CA bundles and
//! [`ClientIdentity`] client certificates (PEM or PKCS#12), each with its own connection pool, picked per request
//! by `SendContext::client_cert`: named, or by matching the URL's host. Bodies are read up to a
//! cap and decoded (gzip, br, zstd, deflate) with the decoded size capped too. Proxies come from
//! [`ProxyConfig`], which the engine reads from `HTTP_PROXY`, `HTTPS_PROXY` and `NO_PROXY`
//! through its environment source. Header values never appear in errors or `Debug` output.

mod alert;
mod body;
mod connector;
mod error;
mod hosts;
mod identity;
mod pkcs12;
mod pkcs12_reason;
mod proxy;
mod stream;
mod tls;
mod transport;

pub use error::HttpAdapterError;
pub use identity::ClientIdentity;
pub use proxy::ProxyConfig;
pub use transport::{HttpOptions, HttpTransport, TlsOptions};
