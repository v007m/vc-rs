//! The demo's test routes (D329), called by the real `vc` on the real `vc demo`: `http.retry`
//! recovers from `/flaky`, a timeout trips on `/slow`, a test retry ends flaky, a poll reaches
//! done, and a short load test keeps or breaches its threshold.

use serde_json::Value;

use crate::demo::{exit_code, wait_until};
use crate::port::free_port;
use crate::{CONFIG, Project};

/// `demo#slow`, `demo#flaky` and `demo#poll`, each taking its query from vars.
const APIS: (&str, &str) = (
    "services/demo/demo.api.yaml",
    concat!(
        "baseUrl: ${var.baseUrl}\napis:\n",
        "  - id: slow\n    path: /slow\n    vars: {ms: 0}\n    query: {ms: '${var.ms}'}\n",
        "  - id: flaky\n    path: /flaky\n    vars: {key: k, failTimes: 0}\n",
        "    query: {key: '${var.key}', failTimes: '${var.failTimes}'}\n",
        "  - id: poll\n    path: /poll/job\n    vars: {after: 0}\n    query: {after: '${var.after}'}\n",
    ),
);

/// A demo on a free port, started for one test and stopped when it ends, passed or failed.
pub struct Demo {
    control: Project,
    pub port: u16,
}

impl Demo {
    pub fn start() -> Self {
        let demo = Self { control: Project::empty(), port: free_port() };
        let started = exit_code(&demo.control, &["demo", "start", "--port", &demo.port.to_string(), "--detach"]);
        assert_eq!(started, Some(0), "`vc demo start --detach` exits 0");
        assert!(wait_until(demo.port, true), "the demo answers on {}", demo.port);
        demo
    }

    /// A project of [`APIS`] and `files`, with `config` added to the config, calling this demo.
    pub fn project(&self, config: &str, files: &[(&str, &str)]) -> Project {
        let project = Project::new(&[APIS]);
        let base = CONFIG.1.replace("http://x", &format!("http://127.0.0.1:{}", self.port));
        project.write(CONFIG.0, &format!("{base}{config}"));
        for (path, text) in files {
            project.write(path, text);
        }
        project
    }
}

impl Drop for Demo {
    fn drop(&mut self) {
        let _stopped = exit_code(&self.control, &["demo", "stop", "--port", &self.port.to_string()]);
    }
}

/// The exit code of `vc run --reporter json` in `project`, and its `testFinished` events.
pub fn run(project: &Project) -> (Option<i32>, Vec<Value>) {
    let output = project.vc(&["run", "--reporter", "json"]).output().unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    let events = text.lines().filter_map(|line| serde_json::from_str::<Value>(line).ok());
    (output.status.code(), events.filter(|event| event["event"] == "testFinished").collect())
}

#[test]
fn http_retry_recovers_from_flaky() {
    let demo = Demo::start();
    let retry =
        "http:\n  retry:\n    attempts: 2\n    on: [503]\n    backoff: {base: 10ms, max: 10ms, jitter: false}\n";
    let test = "tests:\n  - id: passesOnTheThirdSend\n    call: demo#flaky\n    vars: {failTimes: 2}\n\
                \x20   expect: {status: 200, body: {match: {attempt: 3}}}\n";
    let (code, finished) = run(&demo.project(retry, &[("services/demo/retry.test.yaml", test)]));
    assert_eq!(code, Some(0), "{finished:#?}");
    assert_eq!((&finished[0]["status"], &finished[0]["attempts"]), (&"passed".into(), &1.into()), "{finished:#?}");
}

#[test]
fn a_timeout_trips_on_slow() {
    let demo = Demo::start();
    let test = "tests:\n  - id: givesUp\n    call: demo#slow\n    vars: {ms: 1000}\n    timeout: 200ms\n";
    let project = demo.project("", &[("services/demo/timeout.test.yaml", test)]);
    let output = project.vc(&["run"]).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(crate::said(&output).contains("V5104"), "{}", crate::said(&output));
}

#[test]
fn a_test_retry_ends_flaky() {
    let demo = Demo::start();
    let test = "tests:\n  - id: passesSecondTime\n    call: demo#flaky\n    vars: {failTimes: 1}\n\
                \x20   expect: {status: 200}\n    retry: {attempts: 2, delay: 10ms}\n";
    let (code, finished) = run(&demo.project("", &[("services/demo/flaky.test.yaml", test)]));
    assert_eq!(code, Some(0), "a flaky test counts as a pass: {finished:#?}");
    assert_eq!((&finished[0]["status"], &finished[0]["attempts"]), (&"flaky".into(), &2.into()), "{finished:#?}");
}

#[test]
fn a_poll_reaches_done() {
    let demo = Demo::start();
    let test = "tests:\n  - id: waitsForTheJob\n    call: demo#poll\n    vars: {after: 3}\n\
                \x20   poll: {until: {body: {match: {status: done}}}, every: 10ms, timeout: 5s}\n\
                \x20   expect: {body: {match: {attempt: 3}}}\n";
    let (code, finished) = run(&demo.project("", &[("services/demo/poll.test.yaml", test)]));
    assert_eq!((code, &finished[0]["status"]), (Some(0), &"passed".into()), "{finished:#?}");
}

/// `vc load demo.peak --json`: two seconds of up to two users calling `/slow?ms=20`, with `p90` under
/// `limit`. The exit code and the breached metrics.
fn load(demo: &Demo, limit: &str) -> (Option<i32>, Value) {
    let sleeps = "tests:\n  - id: sleeps\n    call: demo#slow\n    vars: {ms: 20}\n";
    let peak = format!(
        "name: Peak\nmix:\n  - test: demo.sleep#sleeps\n    weight: 1\nstages:\n  - {{duration: 2s, users: 2}}\n\
         thresholds: {{p90: {{lt: {limit}}}, errorRate: {{lt: 1%}}}}\n"
    );
    let project = demo.project("", &[("services/demo/sleep.test.yaml", sleeps), ("load/demo/peak.load.yaml", &peak)]);
    let output = project.vc(&["load", "demo.peak", "--json"]).output().unwrap();
    let report: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|_| panic!("{}", crate::said(&output)));
    assert!(report["iterations"].as_u64() > Some(0), "the scenario ran: {report:#}");
    let breached = report["breaches"].as_array().map(|all| all.iter().map(|breach| breach["metric"].clone()).collect());
    (output.status.code(), breached.unwrap_or_default())
}

#[test]
fn a_short_load_test_keeps_or_breaches_its_thresholds() {
    let demo = Demo::start();
    assert_eq!(load(&demo, "5s"), (Some(0), Value::Array(Vec::new())), "20 ms sleeps keep a 5 s p90");
    assert_eq!(load(&demo, "5ms"), (Some(3), Value::Array(vec!["p90".into()])), "but breach a 5 ms one");
}
