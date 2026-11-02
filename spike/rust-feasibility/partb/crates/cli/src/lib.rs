//! Composition root. The only crate that knows concrete adapter types.
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use adapters_hooks::{HookConfig, Limits, QjsHookRunner};
use adapters_http::{ReqwestTransport, StaticTokenAuth};
use adapters_store::SqliteStore;
use engine::{Engine, EngineConfig, Ports};
use ir::Vars;
use ports::{Outcome, PortError, Reporter, RunSummary, TestResult};

#[derive(Debug, Clone)]
pub struct Config {
    pub base_url: String,
    pub db_path: PathBuf,
    pub hooks_root: PathBuf,
    pub shared_setup_tests: usize,
    pub concurrency: usize,
    pub secrets: HashMap<String, String>,
}

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error(transparent)]
    Program(#[from] ir::IrError),
    #[error(transparent)]
    Port(#[from] PortError),
    #[error("store writer task failed: {0}")]
    Join(#[from] tokio::task::JoinError),
}

#[derive(Debug, Clone, Copy)]
pub struct Report {
    pub summary: RunSummary,
    pub stored: usize,
}

struct StderrReporter;

impl Reporter for StderrReporter {
    fn test_finished(&self, result: &TestResult) {
        if result.outcome != Outcome::Passed {
            eprintln!("{}: {:?}", result.name, result.outcome);
        }
    }

    fn run_finished(&self, summary: &RunSummary) {
        eprintln!("passed {} failed {} errored {}", summary.passed, summary.failed, summary.errored);
    }
}

/// Compile, wire adapters, run, and wait for the history writer to drain.
///
/// # Errors
/// Program construction, adapter start-up, or history writer failures.
pub async fn run(config: Config) -> Result<Report, CliError> {
    let program = Arc::new(compiler::demo_program(config.shared_setup_tests)?);
    let (store, writer) = SqliteStore::open(&config.db_path, 1024)?;
    let hooks = QjsHookRunner::start(HookConfig {
        root: config.hooks_root.clone(),
        modules: compiler::demo_hook_modules(),
        limits: Limits { timeout: Duration::from_millis(200), memory_bytes: 64 << 20 },
        secrets: config.secrets.clone(),
    })?;
    let ports = Ports {
        transport: Arc::new(ReqwestTransport::new(config.base_url.clone())?),
        auth: Arc::new(StaticTokenAuth::new("demo-token")),
        hooks: Arc::new(hooks),
        store: Arc::new(store),
        reporter: Arc::new(StderrReporter),
    };
    let engine = Engine::new(program, ports, EngineConfig { concurrency: config.concurrency, vars: Vars::default() });
    let summary = engine.run().await;
    drop(engine); // drops the last store sender so the writer can finish
    let stored = tokio::task::spawn_blocking(move || writer.finish()).await??;
    Ok(Report { summary, stored })
}
