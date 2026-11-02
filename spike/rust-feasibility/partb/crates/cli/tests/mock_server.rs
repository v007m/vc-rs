//! End-to-end against the Go mock server: 50 tests share one file-scoped setup.
use std::collections::HashMap;
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

struct MockServer {
    child: Child,
    base_url: String,
}

impl Drop for MockServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn start_mock() -> std::io::Result<MockServer> {
    let port = TcpListener::bind("127.0.0.1:0")?.local_addr()?.port();
    let addr = format!("127.0.0.1:{port}");
    let bin = std::env::var("MOCK_BIN").map_or_else(|_| workspace_root().join("../../mock-server/mock"), PathBuf::from);
    let child = Command::new(bin).args(["-addr", &addr]).spawn()?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while TcpStream::connect(&addr).is_err() {
        assert!(Instant::now() < deadline, "mock server did not start");
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(MockServer { child, base_url: format!("http://{addr}") })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fifty_tests_sharing_a_file_setup_make_exactly_one_setup_request() {
    let mock = start_mock().unwrap();
    let db = std::env::temp_dir().join(format!("vibranium-it-{}.db", std::process::id()));
    let config = cli::Config {
        base_url: mock.base_url.clone(),
        db_path: db.clone(),
        hooks_root: workspace_root().join("hooks"),
        shared_setup_tests: 50,
        concurrency: 32,
        secrets: HashMap::from([("UPLOAD_KEY".to_owned(), "k".to_owned())]),
    };
    let report = cli::run(config).await.unwrap();
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let stats_text = client.get(format!("{}/__stats", mock.base_url)).send().await.unwrap().text().await.unwrap();
    let stats: HashMap<String, u64> = serde_json::from_str(&stats_text).unwrap();
    let _ = std::fs::remove_file(db);
    println!("mock stats: {stats:?}\nreport: {report:?}");
    assert_eq!(stats.get("POST /slow/sessions"), Some(&1), "setup must run exactly once");
    assert_eq!(stats.get("GET /sessions/:id"), Some(&50));
    assert_eq!((report.summary.passed, report.summary.failed, report.summary.errored), (53, 0, 0));
    assert_eq!(report.stored, 53);
}
