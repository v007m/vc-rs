//! Ports: the traits the engine depends on. Adapters implement them; the
//! engine never names an adapter type.
use std::time::Duration;

use async_trait::async_trait;
use ir::{HeaderName, HookRef, Method, StatusCode, TestId, TestName, UrlPath};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: Method,
    pub path: UrlPath,
    pub headers: Vec<(HeaderName, String)>,
    pub body: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: StatusCode,
    pub headers: Vec<(HeaderName, String)>,
    pub body: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortErrorKind {
    Transport,
    Auth,
    Hook,
    Store,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{kind:?} error: {message}")]
pub struct PortError {
    pub kind: PortErrorKind,
    pub message: String,
}

impl PortError {
    #[must_use]
    pub fn new(kind: PortErrorKind, message: impl Into<String>) -> Self {
        Self { kind, message: message.into() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Passed,
    Failed(Vec<String>),
    Errored(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestResult {
    pub test: TestId,
    pub name: TestName,
    pub outcome: Outcome,
    pub duration: Duration,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RunSummary {
    pub passed: usize,
    pub failed: usize,
    pub errored: usize,
    pub store_errors: usize,
}

impl RunSummary {
    pub const fn add(&mut self, outcome: &Outcome) {
        match outcome {
            Outcome::Passed => self.passed += 1,
            Outcome::Failed(_) => self.failed += 1,
            Outcome::Errored(_) => self.errored += 1,
        }
    }
}

#[async_trait]
pub trait Transport: Send + Sync {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, PortError>;
}

#[async_trait]
pub trait AuthProvider: Send + Sync {
    async fn authorize(&self, request: HttpRequest) -> Result<HttpRequest, PortError>;
}

#[async_trait]
pub trait HookRunner: Send + Sync {
    async fn before_request(&self, hook: &HookRef, request: HttpRequest) -> Result<HttpRequest, PortError>;
}

#[async_trait]
pub trait ResultStore: Send + Sync {
    async fn record(&self, result: TestResult) -> Result<(), PortError>;
}

pub trait Reporter: Send + Sync {
    fn test_finished(&self, result: &TestResult);
    fn run_finished(&self, summary: &RunSummary);
}
