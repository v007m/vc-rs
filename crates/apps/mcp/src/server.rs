//! The MCP server: `tools/list` from the registry, and `tools/call` run against the engine.
//! A refusal or error comes back as a tool result with `isError` set, so the agent reads why.

use std::sync::Arc;

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, Implementation, ListToolsResult, PaginatedRequestParams,
    ServerCapabilities, ServerConfig,
};
use rmcp::service::{MaybeSendFuture, RequestContext};
use rmcp::{ErrorData, RoleServer, ServerHandler, ServiceExt as _};
use serde_json::Value;
use vc_engine::{AgentCall, AgentOutcome, Engine};

use crate::McpError;
use crate::args::{Args, known};
use crate::tools::{Cx, Tools};

/// The agent's name when its client gives none.
const UNNAMED: &str = "agent";

/// What the server tells the agent about itself.
const INSTRUCTIONS: &str = "Vibranium tests HTTP APIs from YAML files. List or search the definitions, \
    check the project, run tests and read their results. Calls, runs and writes follow the project's \
    agent rules, which may refuse them or wait for a person's approval.";

/// The MCP server over one project.
#[derive(Clone)]
pub struct VcServer {
    engine: Arc<Engine>,
    tools: Arc<Tools>,
    layers: (Vec<String>, Vec<String>),
}

impl VcServer {
    /// A server for `engine` with every standard tool, judged under `profiles` and with the
    /// `--set` values `sets`.
    #[must_use]
    pub fn new(engine: Engine, profiles: Vec<String>, sets: Vec<String>) -> Self {
        Self { engine: Arc::new(engine), tools: Arc::new(Tools::standard()), layers: (profiles, sets) }
    }

    /// The engine, shared with the web UI's engine service so both serve one project state.
    #[must_use]
    pub fn engine(&self) -> Arc<Engine> {
        Arc::clone(&self.engine)
    }

    /// Serves MCP over this process's stdin and stdout until the agent closes them.
    ///
    /// # Errors
    ///
    /// [`McpError::Serve`] when the session cannot start or fails.
    pub async fn serve_stdio(self) -> Result<(), McpError> {
        let running = self.serve(rmcp::transport::stdio()).await.map_err(serve)?;
        running.waiting().await.map_err(serve)?;
        Ok(())
    }

    /// Runs the tool `name` for `agent`: its result, or its error as a refusal.
    ///
    /// # Errors
    ///
    /// An [`ErrorData`] when no tool is called `name`.
    pub async fn answer(&self, name: &str, agent: &str, args: Args) -> Result<CallToolResult, ErrorData> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| ErrorData::invalid_params(format!("no tool is called `{name}`"), None))?;
        let cx = Cx {
            engine: self.engine.clone(),
            agent: agent.to_owned(),
            profiles: self.layers.0.clone(),
            sets: self.layers.1.clone(),
        };
        let mut outcome = match known(&tool.schema(), &args) {
            Ok(()) => tool.call(&cx, &args).await,
            Err(error) => Err(error),
        };
        if tool.reads() {
            outcome = cx.logged(tool.name(), args, outcome).await;
        }
        Ok(outcome.map_or_else(|error| CallToolResult::structured_error(error.refusal()), CallToolResult::structured))
    }
}

impl Cx {
    /// Logs a call to the reading tool `tool`, which no gate logs (D195); `outcome`, or the
    /// log's error.
    async fn logged(&self, tool: &str, args: Args, outcome: Result<Value, McpError>) -> Result<Value, McpError> {
        let result = outcome.as_ref().map_or_else(|error| format!("failed: {error}"), |_| "ok".to_owned());
        let call = AgentCall {
            agent: self.agent.clone(),
            tool: tool.to_owned(),
            profiles: self.profiles.clone(),
            summary: tool.to_owned(),
            arguments: Value::Object(args),
            ..AgentCall::default()
        };
        self.engine.record_agent_action(&call, &AgentOutcome { result, approved_by: None }).await?;
        outcome
    }
}

impl ServerHandler for VcServer {
    fn get_info(&self) -> ServerConfig {
        let capabilities = ServerCapabilities::builder().enable_tools().build();
        let server = Implementation::new("vibranium", env!("CARGO_PKG_VERSION"));
        ServerConfig::new(capabilities).with_instructions(INSTRUCTIONS).with_server_info(server)
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, ErrorData>> + MaybeSendFuture + '_ {
        std::future::ready(Ok(ListToolsResult::with_all_items(self.tools.specs())))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let agent = context.peer.peer_info().map_or_else(|| UNNAMED.to_owned(), |info| info.client_info.name.clone());
        let args = request.arguments.unwrap_or_default();
        self.answer(&request.name, &agent, args).await.map(CallToolResponse::from)
    }
}

fn serve(error: impl std::fmt::Display) -> McpError {
    McpError::Serve { reason: error.to_string() }
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
