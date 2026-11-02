//! The engine: runs a compiled [`ir::Program`] using injected [`ports`].
mod assertions;
mod executor;
mod scheduler;
mod setup_cache;

use std::sync::Arc;

use ir::{Program, Vars};
use ports::{AuthProvider, HookRunner, Reporter, ResultStore, RunSummary, Transport};

/// Everything the engine needs from the outside world.
#[derive(Clone)]
pub struct Ports {
    pub transport: Arc<dyn Transport>,
    pub auth: Arc<dyn AuthProvider>,
    pub hooks: Arc<dyn HookRunner>,
    pub store: Arc<dyn ResultStore>,
    pub reporter: Arc<dyn Reporter>,
}

#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Maximum tests in flight.
    pub concurrency: usize,
    /// Variables visible to every test (environment, CLI flags).
    pub vars: Vars,
}

pub struct Engine {
    program: Arc<Program>,
    ports: Ports,
    config: EngineConfig,
}

impl Engine {
    #[must_use]
    pub const fn new(program: Arc<Program>, ports: Ports, config: EngineConfig) -> Self {
        Self { program, ports, config }
    }

    /// Run every runnable test once; setups run at most once per scope key.
    pub async fn run(&self) -> RunSummary {
        scheduler::run(Arc::clone(&self.program), self.ports.clone(), &self.config).await
    }
}

#[cfg(test)]
mod tests;
