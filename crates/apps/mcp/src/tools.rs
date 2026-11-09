//! The MCP tools: one [`Tool`] per engine method an agent may use, kept in the [`Tools`]
//! registry by name (D197). Each tool's input schema is written by hand.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Map, Value};
use vc_engine::Engine;

use crate::McpError;
use crate::args::Args;
use crate::{act, read};

/// What a tool call runs against: the engine, the agent calling and the profiles `vc mcp`
/// was started with. The agent cannot choose its own profile: the profiles' `agents` rules
/// judge it.
#[non_exhaustive]
pub struct Cx {
    /// The project's engine.
    pub engine: Arc<Engine>,
    /// The agent, as its MCP client names itself.
    pub agent: String,
    /// `vc mcp --profile`.
    pub profiles: Vec<String>,
    /// `vc mcp --set`.
    pub sets: Vec<String>,
}

/// One MCP tool.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Its name in `tools/list` and `tools/call`.
    fn name(&self) -> &'static str;

    /// What it does, for the agent.
    fn description(&self) -> &'static str;

    /// Its input schema: a JSON Schema object.
    fn schema(&self) -> Value;

    /// Whether it only reads. A reading tool is logged here; the others are gated and logged
    /// by the engine (D195).
    fn reads(&self) -> bool {
        true
    }

    /// Runs it; the result is a JSON object.
    async fn call(&self, cx: &Cx, args: &Args) -> Result<Value, McpError>;
}

/// The tools, by name.
#[derive(Clone)]
pub struct Tools {
    tools: BTreeMap<&'static str, Arc<dyn Tool>>,
}

impl Tools {
    /// Every tool `vc mcp` serves.
    #[must_use]
    pub fn standard() -> Self {
        let mut tools = Self { tools: BTreeMap::new() };
        read::register(&mut tools);
        act::register(&mut tools);
        tools
    }

    /// Adds `tool`, replacing one with the same name.
    pub fn add(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name(), tool);
    }

    /// The tool called `name`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Arc<dyn Tool>> {
        self.tools.get(name)
    }

    /// Every tool's MCP description, by name.
    #[must_use]
    pub fn specs(&self) -> Vec<rmcp::model::Tool> {
        self.tools.values().map(|tool| spec(tool.as_ref())).collect()
    }
}

fn spec(tool: &dyn Tool) -> rmcp::model::Tool {
    let schema = match tool.schema() {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    let hints = rmcp::model::ToolAnnotations::new().read_only(tool.reads()).destructive(!tool.reads());
    rmcp::model::Tool::new(tool.name(), tool.description(), Arc::new(schema)).annotate(hints)
}

#[cfg(test)]
#[path = "tools_tests.rs"]
mod tests;
