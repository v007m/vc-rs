//! Engine-neutral hook API + measurement driver shared by the QuickJS and V8 prototypes.
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha2::Sha256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marshal {
    /// Walk serde_json::Value and build engine objects directly.
    Direct,
    /// Serialize to text and use the engine's JSON.parse / JSON.stringify.
    JsonText,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    Load,
    Thrown,
    Timeout,
    OutOfMemory,
}

#[derive(Debug, Clone)]
pub struct HookError {
    pub kind: ErrorKind,
    /// Message plus stack, already remapped to the original .ts positions when possible.
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckOutcome {
    Pass,
    Fail(String),
}

pub struct Limits {
    pub timeout: Duration,
    pub memory_bytes: usize,
}

/// What the engine crate exposes; this is the shape of the future `HookRunner` port.
pub trait HookEngine {
    fn load(&mut self, path: &Path) -> Result<(), HookError>;
    fn before_request(&mut self, module: &Path, export: &str, request: &Value) -> Result<Value, HookError>;
    fn check(&mut self, module: &Path, export: &str, request: &Value, response: &Value) -> Result<CheckOutcome, HookError>;
}

pub fn check_outcome(v: &Value) -> CheckOutcome {
    match v {
        Value::Bool(true) => CheckOutcome::Pass,
        Value::String(s) => CheckOutcome::Fail(s.clone()),
        other => CheckOutcome::Fail(format!("check returned {other}")),
    }
}

pub fn hmac_sha256_hex(key: &str, data: &str) -> String {
    let Ok(mut mac) = <Hmac<Sha256> as Mac>::new_from_slice(key.as_bytes()) else { return String::new() };
    mac.update(data.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

pub fn secrets() -> std::collections::HashMap<String, String> {
    std::collections::HashMap::from([("UPLOAD_KEY".to_owned(), "s3cr3t-upload-key".to_owned())])
}

/// Deadline shared with an interrupt handler / watchdog: nanos since `epoch`, 0 = disarmed.
#[derive(Clone)]
pub struct Deadline {
    epoch: Instant,
    at: Arc<AtomicU64>,
}

impl Default for Deadline {
    fn default() -> Self {
        Self { epoch: Instant::now(), at: Arc::new(AtomicU64::new(0)) }
    }
}

impl Deadline {
    fn now_nanos(&self) -> u64 {
        u64::try_from(self.epoch.elapsed().as_nanos()).unwrap_or(u64::MAX)
    }
    pub fn arm(&self, budget: Duration) {
        let budget = u64::try_from(budget.as_nanos()).unwrap_or(u64::MAX);
        self.at.store(self.now_nanos().saturating_add(budget), Ordering::Relaxed);
    }
    pub fn disarm(&self) {
        self.at.store(0, Ordering::Relaxed);
    }
    pub fn expired(&self) -> bool {
        let at = self.at.load(Ordering::Relaxed);
        at != 0 && self.now_nanos() > at
    }
}

// ---------------------------------------------------------------- payloads

pub fn sample_request() -> Value {
    json!({
        "method": "POST",
        "path": "/v1/uploads/7f3a9c2e-5b1d-4e8f-a6c4-91d2b3e4f5a6/parts/3",
        "headers": {
            "content-type": "application/json", "accept": "application/json",
            "authorization": "Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ1c2VyLTQyIn0.sig",
            "x-request-id": "req-000123", "x-tenant": "acme", "user-agent": "vibranium/2.0"
        },
        "query": { "dryRun": "false", "region": "eu-west-1" },
        "body": {"id": "bench-1", "name": "bench", "active": true, "total": 101.5,
                 "items": [{"sku": "a", "price": 9.5}, {"sku": "b", "price": 12.25}, {"sku": "c", "price": 30.0},
                           {"sku": "d", "price": 0.5}, {"sku": "e", "price": 49.25}]}
    })
}

pub fn sample_response() -> Value {
    let items: Vec<Value> = (0..20)
        .map(|i| json!({"sku": format!("sku-{i:03}"), "currency": "EUR", "price": 9.5 + f64::from(i), "qty": i % 4 + 1, "tags": ["sale", "new"]}))
        .collect();
    json!({"status": 200, "headers": {"content-type": "application/json", "x-request-id": "req-000123", "etag": "\"abc123\"", "cache-control": "no-store", "date": "Thu, 23 Oct 2026 07:00:00 GMT"},
           "body": {"items": items, "count": 20, "next": "/v1/products?page=2"}})
}

// ---------------------------------------------------------------- driver

pub struct Args {
    pub mode: String,
    pub hooks: PathBuf,
    pub iterations: u32,
    pub runs: u32,
    pub marshal: Marshal,
}

pub fn parse_args() -> Args {
    let a: Vec<String> = std::env::args().collect();
    let get = |i: usize, d: &str| a.get(i).cloned().unwrap_or_else(|| d.to_owned());
    Args {
        mode: get(1, "first-call"),
        hooks: PathBuf::from(get(2, "hooks")),
        iterations: get(3, "100000").parse().unwrap_or(100_000),
        runs: get(4, "3").parse().unwrap_or(3),
        marshal: if get(5, "direct") == "json" { Marshal::JsonText } else { Marshal::Direct },
    }
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn time_ns_per_call(n: u32, mut f: impl FnMut()) -> f64 {
    let start = Instant::now();
    for _ in 0..n {
        f();
    }
    start.elapsed().as_secs_f64() * 1e9 / f64::from(n)
}

pub fn run<E: HookEngine>(engine_name: &str, make: impl FnOnce(Marshal) -> E) {
    let process_start = Instant::now();
    let args = parse_args();
    let mut engine = make(args.marshal);
    let created = process_start.elapsed();
    let out = match args.mode.as_str() {
        "first-call" => first_call(&mut engine, &args, process_start, created),
        "bench" => bench(&mut engine, &args),
        "safety" => safety(&mut engine, &args),
        "errors" => errors(&mut engine, &args),
        other => json!({"error": format!("unknown mode {other}")}),
    };
    println!("{}", json!({"engine": engine_name, "mode": args.mode, "marshal": format!("{:?}", args.marshal), "result": out}));
}

fn load_or_panic<E: HookEngine>(engine: &mut E, p: &Path) {
    if let Err(e) = engine.load(p) {
        eprintln!("load failed: {}", e.message);
        std::process::exit(2);
    }
}

fn first_call<E: HookEngine>(engine: &mut E, args: &Args, t0: Instant, created: Duration) -> Value {
    let sign = args.hooks.join("sign.ts");
    load_or_panic(engine, &sign);
    let loaded = t0.elapsed();
    let signed = engine.before_request(&sign, "addSignature", &sample_request());
    let first = t0.elapsed();
    let expected = hmac_sha256_hex("s3cr3t-upload-key", "/v1/uploads/7f3a9c2e-5b1d-4e8f-a6c4-91d2b3e4f5a6/parts/3");
    let sig_ok = signed.as_ref().is_ok_and(|v| v["headers"]["x-signature"] == Value::String(expected.clone()));
    json!({"engine_created_us": created.as_micros(), "module_loaded_us": loaded.as_micros(),
           "first_call_done_us": first.as_micros(), "signature_correct": sig_ok,
           "x_signature": signed.ok().map(|v| v["headers"]["x-signature"].clone())})
}

fn bench<E: HookEngine>(engine: &mut E, args: &Args) -> Value {
    let (sign, checks) = (args.hooks.join("sign.ts"), args.hooks.join("checks.ts"));
    load_or_panic(engine, &sign);
    load_or_panic(engine, &checks);
    let (req, resp) = (sample_request(), sample_response());
    assert_eq!(engine.check(&checks, "currencyIsConsistent", &req, &resp).ok(), Some(CheckOutcome::Pass));
    let mut br = Vec::new();
    let mut ck = Vec::new();
    for _ in 0..args.runs {
        br.push(time_ns_per_call(args.iterations, || {
            let out = engine.before_request(&sign, "addSignature", &req);
            assert!(out.is_ok());
        }));
        ck.push(time_ns_per_call(args.iterations, || {
            let out = engine.check(&checks, "currencyIsConsistent", &req, &resp);
            assert!(out.is_ok());
        }));
    }
    json!({"iterations": args.iterations, "runs": args.runs, "before_request_ns_runs": br, "check_ns_runs": ck,
           "before_request_ns_median": median(br.clone()), "check_ns_median": median(ck.clone())})
}

fn probe<E: HookEngine>(engine: &mut E, file: &Path, export: &str) -> Value {
    let start = Instant::now();
    let r = engine.check(file, export, &sample_request(), &sample_response());
    let ms = start.elapsed().as_secs_f64() * 1e3;
    match r {
        Ok(o) => json!({"outcome": format!("{o:?}"), "ms": ms}),
        Err(e) => json!({"error_kind": format!("{:?}", e.kind), "message": e.message.lines().take(4).collect::<Vec<_>>().join(" | "), "ms": ms}),
    }
}

fn safety<E: HookEngine>(engine: &mut E, args: &Args) -> Value {
    let (hostile, sign) = (args.hooks.join("hostile.ts"), args.hooks.join("sign.ts"));
    load_or_panic(engine, &hostile);
    load_or_panic(engine, &sign);
    let spin = probe(engine, &hostile, "spin");
    let hog = probe(engine, &hostile, "hog");
    let escape = probe(engine, &hostile, "escape");
    let import_fs = probe(engine, &hostile, "importFs");
    let still_usable = engine.before_request(&sign, "addSignature", &sample_request()).is_ok();
    json!({"infinite_loop": spin, "memory_hog": hog, "global_escape_probe": escape, "dynamic_import_fs": import_fs,
           "engine_usable_after": still_usable})
}

fn errors<E: HookEngine>(engine: &mut E, args: &Args) -> Value {
    let throws = args.hooks.join("throws.ts");
    load_or_panic(engine, &throws);
    match engine.check(&throws, "totalIsPositive", &sample_request(), &sample_response()) {
        Ok(o) => json!({"unexpected": format!("{o:?}")}),
        Err(e) => json!({"kind": format!("{:?}", e.kind), "message": e.message}),
    }
}
