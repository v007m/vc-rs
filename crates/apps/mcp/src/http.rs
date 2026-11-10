//! MCP over streamable HTTP, which `vc ui` mounts at `/mcp` behind its token, `Host` and `Origin`
//! checks (D197). The tools are the same as over stdio.

use std::sync::Arc;

use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};

use crate::VcServer;

/// The MCP service the web server mounts at `/mcp`.
pub type HttpService = StreamableHttpService<VcServer, LocalSessionManager>;

impl VcServer {
    /// This server as MCP over streamable HTTP, and the call that ends its sessions and open
    /// streams so the web server can stop. rmcp's own `Host` check keeps its loopback default.
    pub fn into_http(self) -> (HttpService, impl FnOnce() + Send + 'static) {
        let config = StreamableHttpServerConfig::default();
        let stop = config.cancellation_token.clone();
        let service = StreamableHttpService::new(move || Ok(self.clone()), Arc::default(), config);
        (service, move || stop.cancel())
    }
}
