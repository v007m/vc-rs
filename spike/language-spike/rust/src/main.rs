mod adapters;
mod compiler;
mod diag;
mod executor;
mod load;
mod model;
mod ports;
mod runtime;

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use clap::{Parser, ValueEnum};
use serde_json::json;

use crate::adapters::{json_to_cel, CelRustEngine, ConsoleReporter, JsonSchemaValidator, ReqwestTransport};
use crate::executor::{ApiCaller, Executor, MemoGivenResolver};

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum Cmd {
    Compile,
    Run,
    Load,
    BenchCel,
}

#[derive(Parser, Debug)]
#[command(name = "vib", version = "0.0.1", about = "vibranium-rust spike")]
struct Cli {
    #[arg(value_enum)]
    cmd: Cmd,
    target: PathBuf,
    #[arg(long, default_value = "http://127.0.0.1:18080")]
    base_url: String,
    #[arg(long, default_value_t = 32)]
    concurrency: usize,
    #[arg(long, default_value_t = 100)]
    vus: usize,
    /// Max in-flight requests in load mode (0 = one per VU)
    #[arg(long, default_value_t = 0)]
    conns: usize,
    #[arg(long, default_value_t = 10)]
    duration: u64,
    #[arg(long, default_value_t = 1_000_000)]
    iterations: u64,
    #[arg(long)]
    verbose: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let engine = Arc::new(CelRustEngine::default());

    if cli.cmd == Cmd::BenchCel {
        return bench_cel(&engine, &cli);
    }

    let t0 = Instant::now();
    let res = match compiler::compile(&cli.target, engine.as_ref()) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(3);
        }
    };
    let compile_ms = t0.elapsed().as_secs_f64() * 1e3;
    if !res.diagnostics.is_empty() {
        println!("{}", res.format());
        println!("\n{} error(s) in {} files", res.diagnostics.len(), res.file_count);
        return ExitCode::from(1);
    }
    let program = Arc::new(res.program);
    if cli.cmd == Cmd::Compile {
        let stats = json!({"files": res.file_count, "apis": program.apis.len(), "givens": program.givens.len(), "tests": program.tests.len(), "compileMs": compile_ms});
        println!("RESULT {stats}");
        return ExitCode::SUCCESS;
    }

    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("tokio runtime");
    rt.block_on(async {
        match cli.cmd {
            Cmd::Run => {
                let caller = Arc::new(ApiCaller { http: Arc::new(ReqwestTransport::new(cli.concurrency)), base_url: cli.base_url.clone() });
                let exec = Arc::new(Executor {
                    program: program.clone(),
                    caller: caller.clone(),
                    givens: Arc::new(MemoGivenResolver::new(program.clone(), caller)),
                    exprs: engine.clone(),
                    schemas: Arc::new(JsonSchemaValidator::default()),
                    reporter: Arc::new(ConsoleReporter { verbose: cli.verbose }),
                    expr_cache: Mutex::new(HashMap::new()),
                });
                if exec.run(cli.concurrency).await.failed == 0 { ExitCode::SUCCESS } else { ExitCode::from(1) }
            }
            Cmd::Load => {
                let max_conns = if cli.conns > 0 { cli.conns } else { cli.vus };
                let caller = Arc::new(ApiCaller { http: Arc::new(ReqwestTransport::with_limit(max_conns)), base_url: cli.base_url.clone() });
                match load::run(program, caller, cli.vus, Duration::from_secs(cli.duration)).await {
                    Ok(r) => {
                        println!("RESULT {}", serde_json::to_string(&r).expect("serialize"));
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("{e}");
                        ExitCode::from(3)
                    }
                }
            }
            Cmd::Compile | Cmd::BenchCel => unreachable!(),
        }
    })
}

fn bench_cel(engine: &CelRustEngine, cli: &Cli) -> ExitCode {
    let body: serde_json::Value = match std::fs::read_to_string(&cli.target).map_err(|e| e.to_string()).and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string())) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(3);
        }
    };
    let expr = engine.compile_concrete("size(body.items) > 0 && body.items.all(i, i.price > 0)").expect("compile");
    // Convert the JSON body to CEL values once (the "parsed body" in the benchmark spec).
    let body = adapters::CelVars::new(json_to_cel(&body), 200).expect("convert");
    let mut ok = 0u64;
    for _ in 0..10_000 {
        ok += u64::from(expr(&body) == Ok(true));
    }
    let start = Instant::now();
    for _ in 0..cli.iterations {
        ok += u64::from(expr(std::hint::black_box(&body)) == Ok(true));
    }
    let ns = start.elapsed().as_nanos() as f64;
    println!("RESULT {}", json!({"iterations": cli.iterations, "nsPerOp": ns / cli.iterations as f64, "allTrue": ok == cli.iterations + 10_000}));
    ExitCode::SUCCESS
}
