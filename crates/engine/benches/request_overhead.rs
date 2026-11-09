//! Overhead per request outside the network (render, checks, events): `vc run` against the
//! in-process mock server, less the time spent in the transport, per request. Runs of `REQUESTS`
//! and twice that many are compared, so the run's fixed cost (compile, plan, reports) cancels
//! out. `scripts/bench-budgets.sh` holds the median to the D120 budget of 50 µs.

use std::fmt::Display;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use criterion::{Criterion, criterion_group, criterion_main};
use tokio::runtime::Runtime;
use vc_adapter_http::{HttpOptions, HttpTransport};
use vc_engine::{Engine, EngineBuilder, RunRequest};
use vc_interfaces::{SendContext, Transport, TransportError};
use vc_mock_server::{MockHandle, MockServer};
use vc_model::{Exchange, RenderedRequest};
use vc_testkit::{MemoryCompileCache, MemorySources};

/// Requests in the shorter run; the longer run sends twice as many.
const REQUESTS: u32 = 500;

const API: &str = "baseUrl: ${var.baseUrl}\napis:\n  - id: send\n    method: POST\n    path: /echo\n";

/// Sends over HTTP and adds the time each send took to `spent`.
struct Timed {
    inner: HttpTransport,
    spent: Arc<AtomicU64>,
}

#[async_trait]
impl Transport for Timed {
    async fn send(&self, request: RenderedRequest, ctx: &SendContext) -> Result<Exchange, TransportError> {
        let started = Instant::now();
        let exchange = self.inner.send(request, ctx).await;
        let nanos = u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX);
        self.spent.fetch_add(nanos, Ordering::Relaxed);
        exchange
    }
}

/// The mock server, and engines over a project that sends `REQUESTS` and `2 * REQUESTS` requests.
struct Bench {
    runtime: Runtime,
    _server: MockHandle,
    spent: Arc<AtomicU64>,
    short: Engine,
    long: Engine,
}

impl Bench {
    fn prepare() -> Self {
        let runtime = or_exit(Runtime::new());
        let server = or_exit(runtime.block_on(MockServer::start()));
        let spent = Arc::new(AtomicU64::new(0));
        let engine = |requests| engine(&server.base_url(), requests, &spent);
        let (short, long) = (engine(REQUESTS), engine(2 * REQUESTS));
        Self { runtime, _server: server, spent, short, long }
    }

    /// The time per request outside the network: the extra `REQUESTS` of the longer run.
    fn per_request(&self) -> Duration {
        let extra = self.outside(&self.long).saturating_sub(self.outside(&self.short));
        extra / REQUESTS
    }

    /// How long a run of `engine` took, less the time its requests spent in the transport.
    fn outside(&self, engine: &Engine) -> Duration {
        self.spent.store(0, Ordering::Relaxed);
        let (_interrupter, interrupts) = vc_engine::interrupts();
        let request = RunRequest { seed: Some(1), concurrency: Some(1), ..RunRequest::default() };
        let started = Instant::now();
        let report = or_exit(self.runtime.block_on(engine.run(&request, interrupts)));
        let total = started.elapsed();
        if report.exit != 0 {
            eprintln!("bench run failed with exit {}: {:?}", report.exit, report.notices);
            std::process::exit(1);
        }
        total.saturating_sub(Duration::from_nanos(self.spent.load(Ordering::Relaxed)))
    }
}

/// An engine over a project whose one test sends `requests` requests to the mock at `base`.
fn engine(base: &str, requests: u32, spent: &Arc<AtomicU64>) -> Engine {
    let config = format!(
        "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {{baseUrl: '{base}', tenantId: acme}}\n"
    );
    let test = format!(
        "tests:\n  - id: sendsRequests\n    steps:\n      - call: echo#send\n        repeat: {requests}\n        \
         query: {{n: '${{iteration.index}}'}}\n        body: {{tenant: '${{var.tenantId}}'}}\n        \
         expect: {{status: 200}}\n"
    );
    let files: [(&str, &[u8]); 3] = [
        ("vibranium.config.yaml", config.as_bytes()),
        ("services/echo/echo.api.yaml", API.as_bytes()),
        ("services/echo/overhead.test.yaml", test.as_bytes()),
    ];
    let transport = Timed { inner: or_exit(HttpTransport::new(&HttpOptions::default())), spent: Arc::clone(spent) };
    let builder = EngineBuilder::new(std::env::temp_dir().join("vc-bench-request"))
        .sources(Arc::new(MemorySources::new(&files)))
        .compile_cache(Arc::new(MemoryCompileCache::new()))
        .transport(Arc::new(transport));
    or_exit(builder.build())
}

fn overhead(c: &mut Criterion) {
    let bench = Bench::prepare();
    bench.per_request();
    let mut group = c.benchmark_group("request");
    group.sample_size(10);
    group.bench_function("overhead", |b| b.iter_custom(|iters| (0..iters).map(|_| bench.per_request()).sum()));
    group.finish();
}

/// A bench cannot measure anything once its setup failed, so it says why and stops.
fn or_exit<T, E: Display>(result: Result<T, E>) -> T {
    result.unwrap_or_else(|error| {
        eprintln!("bench setup failed: {error}");
        std::process::exit(1)
    })
}

criterion_group!(benches, overhead);
criterion_main!(benches);
