//! Fan tests out as tokio tasks; collect results over an mpsc channel and
//! forward them to the reporter and the result store.
use std::sync::Arc;
use std::time::Instant;

use ir::{Program, Test, TestId, Vars};
use ports::{Outcome, RunSummary, TestResult};
use tokio::sync::{Semaphore, mpsc};

use crate::executor::{CallError, execute_call};
use crate::setup_cache::{SetupCache, SetupKey};
use crate::{EngineConfig, Ports, assertions};

/// Read-only state shared by all test tasks of one run.
struct RunContext {
    program: Arc<Program>,
    ports: Ports,
    vars: Vars,
    setups: SetupCache,
}

pub async fn run(program: Arc<Program>, ports: Ports, config: &EngineConfig) -> RunSummary {
    let concurrency = config.concurrency.max(1);
    let runnable: Vec<TestId> = program.runnable().collect();
    let ctx = Arc::new(RunContext { program, ports, vars: config.vars.clone(), setups: SetupCache::default() });
    let limiter = Arc::new(Semaphore::new(concurrency));
    let (results_tx, results_rx) = mpsc::channel(concurrency);
    for id in runnable {
        let (ctx, limiter, results_tx) = (Arc::clone(&ctx), Arc::clone(&limiter), results_tx.clone());
        tokio::spawn(async move {
            let Ok(_permit) = limiter.acquire_owned().await else { return };
            let result = run_test(&ctx, id).await;
            // The collector only stops once every sender is gone, so this cannot fail.
            let _ = results_tx.send(result).await;
        });
    }
    drop(results_tx);
    collect(results_rx, &ctx.ports).await
}

async fn collect(mut results: mpsc::Receiver<TestResult>, ports: &Ports) -> RunSummary {
    let mut summary = RunSummary::default();
    while let Some(result) = results.recv().await {
        ports.reporter.test_finished(&result);
        summary.add(&result.outcome);
        if ports.store.record(result).await.is_err() {
            summary.store_errors += 1;
        }
    }
    ports.reporter.run_finished(&summary);
    summary
}

async fn run_test(ctx: &RunContext, id: TestId) -> TestResult {
    let started = Instant::now();
    let test = ctx.program.test(id);
    let outcome = match outcome(ctx, test).await {
        Ok(outcome) => outcome,
        Err(error) => Outcome::Errored(error.to_string()),
    };
    TestResult { test: id, name: test.spec.name.clone(), outcome, duration: started.elapsed() }
}

async fn outcome(ctx: &RunContext, test: &Test) -> Result<Outcome, CallError> {
    let vars = setup_vars(ctx, test).await?;
    let call = execute_call(&ctx.ports, test, &vars).await?;
    Ok(assertions::evaluate(&test.spec.assertions, &call.response, &call.vars))
}

/// A test reused as setup runs only its call (no assertions), once per scope key.
async fn setup_vars(ctx: &RunContext, test: &Test) -> Result<Vars, CallError> {
    let Some(setup) = test.spec.setup else { return Ok(ctx.vars.clone()) };
    let key = SetupKey::new(setup, test.spec.file, &ctx.vars);
    let setup_test = ctx.program.test(setup.test);
    ctx.setups.get_or_run(key, || async { Ok(execute_call(&ctx.ports, setup_test, &ctx.vars).await?.vars) }).await
}
