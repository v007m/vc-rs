//! The load budget: 5,000 virtual users, each sending a request about once a second, against the
//! mock server, which runs in a child process so the CPU the bench reads is the tool's alone. A
//! sample is the tool's CPU time per second of a run, so its median in ms is thousandths of a
//! core: `scripts/bench-budgets.sh` holds it under 5,600 (70% of 8 cores). A run that falls
//! behind the users' pace fails instead, since an idle tool would flatter the budget. Unix only:
//! the CPU clock is `clock_gettime(CLOCK_PROCESS_CPUTIME_ID)`.

#[cfg(unix)]
mod load {
    use std::fmt::Display;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::process::{Child, Command, Stdio};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use criterion::{Criterion, SamplingMode};
    use rustix::process::{Resource, Rlimit, getrlimit, setrlimit};
    use rustix::time::{ClockId, clock_gettime};
    use tokio::runtime::Runtime;
    use vc_adapter_http::{HttpOptions, HttpTransport};
    use vc_engine::{Engine, EngineBuilder, LoadRequest, LoadWatch};
    use vc_mock_server::MockServer;
    use vc_testkit::{MemoryCompileCache, MemorySources};

    /// Set in the child process, which serves the mock instead of benching.
    const MOCK: &str = "VC_BENCH_LOAD_MOCK";

    /// The least iterations a second over a whole run (5,000 users, a 1 s ramp, then 15 s).
    const LEAST_RPS: f64 = 4_500.0;

    /// The open-file limit the bench asks for, within what macOS allows a process.
    const OPEN_FILES: u64 = 10_240;

    const API: &str = "baseUrl: ${var.baseUrl}\napis:\n  - id: send\n    method: POST\n    path: /echo\n";
    const TEST: &str = "tests:\n  - id: sendsEcho\n    call: echo#send\n";
    const SCENARIO: &str = "mix:\n  - {test: echo.traffic#sendsEcho, weight: 1}\nstages:\n  \
                            - {duration: 1s, users: 5000}\n  - {duration: 15s, users: 5000}\nthinkTime: 1s\n";

    pub fn main() {
        raise_open_files();
        if std::env::var_os(MOCK).is_some() {
            serve_mock();
            return;
        }
        let mut criterion = Criterion::default().configure_from_args();
        bench(&mut criterion);
        criterion.final_summary();
    }

    fn bench(c: &mut Criterion) {
        let mock = Mock::spawn();
        let runtime = or_exit(Runtime::new());
        let engine = engine(&mock.base_url);
        let mut group = c.benchmark_group("load");
        group.sample_size(10).sampling_mode(SamplingMode::Flat);
        group.warm_up_time(Duration::from_millis(1)).measurement_time(Duration::from_secs(1));
        group.bench_function("5k_users", |b| {
            b.iter_custom(|iters| (0..iters).map(|_| cpu_per_second(&runtime, &engine)).sum());
        });
        group.finish();
    }

    /// The tool's CPU time per second of one run of the scenario.
    fn cpu_per_second(runtime: &Runtime, engine: &Engine) -> Duration {
        let request = LoadRequest { scenario: "echo.peak".to_owned(), ..LoadRequest::default() };
        let (_interrupter, interrupts) = vc_engine::interrupts();
        let (cpu, started) = (cpu_time(), Instant::now());
        let report = or_exit(runtime.block_on(engine.load(&request, LoadWatch { interrupts, ticks: None })));
        let (cpu, wall) = (cpu_time().saturating_sub(cpu), started.elapsed());
        if report.exit != 0 || report.rps < LEAST_RPS {
            eprintln!("load bench fell short (at least {LEAST_RPS} rps):\n{}", report.text());
            std::process::exit(1);
        }
        cpu.div_f64(wall.as_secs_f64())
    }

    /// An engine over a project whose one test posts to the mock at `base`, and the scenario.
    fn engine(base: &str) -> Engine {
        let config =
            format!("syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {{baseUrl: '{base}'}}\n");
        let files: [(&str, &[u8]); 4] = [
            ("vibranium.config.yaml", config.as_bytes()),
            ("services/echo/echo.api.yaml", API.as_bytes()),
            ("services/echo/traffic.test.yaml", TEST.as_bytes()),
            ("load/echo/peak.load.yaml", SCENARIO.as_bytes()),
        ];
        let builder = EngineBuilder::new(std::env::temp_dir().join("vc-bench-load"))
            .sources(Arc::new(MemorySources::new(&files)))
            .compile_cache(Arc::new(MemoryCompileCache::new()))
            .transport(Arc::new(or_exit(HttpTransport::new(&HttpOptions::default()))));
        or_exit(builder.build())
    }

    /// The mock server in a child process; dropping it closes the child's stdin, which ends it.
    struct Mock {
        child: Child,
        base_url: String,
    }

    impl Mock {
        fn spawn() -> Self {
            let exe = or_exit(std::env::current_exe());
            let mut command = Command::new(exe);
            command.env(MOCK, "1").stdin(Stdio::piped()).stdout(Stdio::piped());
            let mut child = or_exit(command.spawn());
            let mut base_url = String::new();
            if let Some(stdout) = child.stdout.take() {
                or_exit(BufReader::new(stdout).read_line(&mut base_url));
            }
            Self { child, base_url: base_url.trim().to_owned() }
        }
    }

    impl Drop for Mock {
        fn drop(&mut self) {
            drop(self.child.stdin.take());
            let _waited = self.child.wait();
        }
    }

    /// The child: serves the mock, says its base URL and stops once the bench closes stdin.
    fn serve_mock() {
        let runtime = or_exit(Runtime::new());
        let server = or_exit(runtime.block_on(MockServer::start()));
        or_exit(writeln!(std::io::stdout().lock(), "{}", server.base_url()));
        let _closed = std::io::stdin().read_to_end(&mut Vec::new());
        drop(server);
    }

    /// The soft open-file limit is often 256 or 1,024; a burst of users can need more sockets.
    fn raise_open_files() {
        let limit = getrlimit(Resource::Nofile);
        let wanted = limit.maximum.map_or(OPEN_FILES, |maximum| maximum.min(OPEN_FILES));
        if limit.current.is_some_and(|current| current < wanted) {
            let _raised = setrlimit(Resource::Nofile, Rlimit { current: Some(wanted), maximum: limit.maximum });
        }
    }

    fn cpu_time() -> Duration {
        let now = clock_gettime(ClockId::ProcessCPUTime);
        Duration::new(u64::try_from(now.tv_sec).unwrap_or(0), u32::try_from(now.tv_nsec).unwrap_or(0))
    }

    /// A bench cannot measure anything once its setup failed, so it says why and stops.
    fn or_exit<T, E: Display>(result: Result<T, E>) -> T {
        result.unwrap_or_else(|error| {
            eprintln!("load bench setup failed: {error}");
            std::process::exit(1)
        })
    }
}

fn main() {
    #[cfg(unix)]
    load::main();
}
