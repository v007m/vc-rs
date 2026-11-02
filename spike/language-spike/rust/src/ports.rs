//! Ports: the core depends only on these traits; adapters implement them.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: &'static str,
    pub url: String,
    pub body: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    /// Decoded JSON (`serde_json::Value` is the dynamic "unknown" type).
    pub body: Value,
}

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("transport: {0}")]
    Io(String),
}

#[async_trait]
pub trait HttpTransport: Send + Sync {
    async fn send(&self, req: HttpRequest) -> Result<HttpResponse, TransportError>;
}

/// Values a given exposes to consumers. `Arc` so memoized results are shared, not cloned.
pub type Exposed = Arc<BTreeMap<String, Value>>;

#[derive(Debug, Clone)]
pub struct GivenScope {
    pub file: Arc<str>,
    pub instance: String,
}

#[async_trait]
pub trait GivenResolver: Send + Sync {
    async fn resolve(&self, given_id: &str, scope: &GivenScope) -> Result<Exposed, String>;
}

#[derive(Debug, Clone)]
pub struct TestResult {
    pub id: String,
    pub iteration: u32,
    pub passed: bool,
    pub failures: Vec<String>,
    pub duration_ms: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct RunSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub duration_ms: f64,
}

pub trait Reporter: Send + Sync {
    fn test_finished(&self, result: &TestResult);
    fn run_finished(&self, summary: &RunSummary);
}

pub trait CompiledExpr: Send + Sync {
    fn eval_bool(&self, body: &Value, status: u16) -> Result<bool, String>;
}

pub trait ExprEngine: Send + Sync {
    fn compile(&self, source: &str) -> Result<Arc<dyn CompiledExpr>, String>;
}

pub trait SchemaValidator: Send + Sync {
    /// Returns violations; empty when valid.
    fn validate(&self, schema: &Value, value: &Value) -> Result<Vec<String>, String>;
}
