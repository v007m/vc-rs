//! `vc mcp` serves the MCP tools over stdio (D197): a scripted agent lists the tools, reads,
//! calls and runs against the mock, and needs a person's approval exactly where the profile's
//! `agents` rules say (D195). A client that names itself nowhere is `agent`; one that breaks
//! the handshake ends the server with exit 2, never as a bug.

use std::io::{BufRead as _, BufReader, Lines, Write as _};
use std::process::{Child, ChildStdin, ChildStdout, Stdio};
use std::time::{Duration, Instant};

use assert_cmd::cargo::cargo_bin;
use serde_json::{Value, json};
use vc_mock_server::MockServer;

use crate::{CONFIG, Project, USERS, said};

/// `dev` lets an agent run tests and send GET, asks a person for POST and DELETE, and lets
/// it write nothing.
const AGENTS: &str = "    agents: {run: true, call: [GET], approve: [POST, DELETE]}\n";

const CRUD: (&str, &str) = ("services/users/crud.test.yaml", "tests:\n  - id: listsUsers\n    call: users#list\n");

/// An MCP client over a running `vc mcp`.
struct Agent {
    child: Child,
    input: ChildStdin,
    output: Lines<BufReader<ChildStdout>>,
    next: u64,
}

impl Agent {
    fn start(project: &Project) -> Self {
        let client = json!({"name": "e2e-agent", "version": "1.0.0"});
        Self::hello(project, json!({"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": client}))
    }

    /// Starts `vc mcp` and initializes it with `hello`.
    fn hello(project: &Project, hello: Value) -> Self {
        let mut vc = std::process::Command::new(cargo_bin!("vc"));
        vc.current_dir(project.path()).arg("mcp").env("HOME", project.home()).env("NO_COLOR", "1");
        let mut child = vc.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().unwrap();
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap()).lines();
        let mut agent = Self { child, input, output, next: 0 };
        let answer = agent.ask("initialize", hello);
        assert_eq!(answer["result"]["serverInfo"]["name"], "vibranium", "{answer}");
        agent.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
        agent
    }

    fn send(&mut self, message: &Value) {
        writeln!(self.input, "{message}").unwrap();
        self.input.flush().unwrap();
    }

    /// Sends a request and waits for its answer.
    fn ask(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        let id = self.next;
        let mut request = json!({"jsonrpc": "2.0", "id": id, "method": method});
        request["params"] = params;
        self.send(&request);
        loop {
            let line = self.output.next().unwrap().unwrap();
            let message: Value = serde_json::from_str(&line).unwrap();
            if message["id"] == id {
                return message;
            }
        }
    }

    /// Calls a tool; its result.
    fn call(&mut self, tool: &str, arguments: Value) -> Value {
        let mut params = json!({ "name": tool });
        params["arguments"] = arguments;
        let answer = self.ask("tools/call", params);
        answer["result"].clone()
    }

    /// Closes stdin, which ends the session; the exit code.
    fn stop(mut self) -> Option<i32> {
        drop(self.input);
        self.child.wait().unwrap().code()
    }
}

/// Runs `vc <approve>` from another terminal once a `tool` call is waiting; what it printed.
fn approve_when_asked(project: &Project, tool: &str, approve: &[&str]) -> String {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        let listed = project.vc(&["approvals"]).output().unwrap();
        if String::from_utf8_lossy(&listed.stdout).contains(tool) {
            let approved = project.vc(approve).output().unwrap();
            assert_eq!(approved.status.code(), Some(0), "{}", said(&approved));
            return String::from_utf8_lossy(&approved.stdout).into_owned();
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("no {tool} call ever waited");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_needs_approval_exactly_where_the_profile_says() {
    let server = MockServer::start().await.unwrap();
    let config = CONFIG
        .1
        .replace("http://x", &server.base_url())
        .replace("    default: true\n", &format!("    default: true\n{AGENTS}"));
    let project = Project::new(&[USERS, CRUD]);
    project.write(CONFIG.0, &config);
    let mut agent = Agent::start(&project);

    let tools = agent.ask("tools/list", json!({}));
    assert_eq!(tools["result"]["tools"].as_array().unwrap().len(), 9, "{tools}");
    let found = agent.call("search", json!({"query": "create"}));
    assert_eq!(found["structuredContent"]["items"][0]["id"], "users#create", "{found}");

    let listed = agent.call("call_api", json!({"api": "users#list"}));
    assert_eq!(listed["isError"], false, "{listed}");
    let ran = agent.call("run_tests", json!({"ids": ["users.crud"]}));
    assert_eq!(ran["structuredContent"]["exit"], 0, "{ran}");

    let waiting = std::thread::scope(|scope| {
        scope.spawn(|| approve_when_asked(&project, "call_api", &["approve", "1"]));
        agent.call("call_api", json!({"api": "users#create"}))
    });
    assert_eq!(waiting["isError"], false, "{waiting}");
    let refused = agent.call("write_definition", json!({"path": CRUD.0, "text": CRUD.1}));
    assert_eq!(refused["isError"], true, "{refused}");
    assert_eq!(refused["structuredContent"]["kind"], "refused", "{refused}");
    assert_eq!(agent.stop(), Some(0));

    let stats = server.stats().await.unwrap();
    assert_eq!((stats.count("GET /users"), stats.count("POST /users"), stats.count("DELETE /users/{id}")), (2, 1, 1));
    let decided = project.vc(&["approvals", "--all"]).output().unwrap();
    assert!(String::from_utf8_lossy(&decided.stdout).contains("e2e-agent"), "{}", said(&decided));
}

#[test]
fn an_agent_that_closes_stdin_before_it_begins_ends_the_server_quietly() {
    let project = Project::new(&[CONFIG, USERS, CRUD]);
    let closed = project.vc(&["mcp"]).write_stdin("").output().unwrap();
    assert_eq!(closed.status.code(), Some(0), "{}", said(&closed));
    assert!(closed.stdout.is_empty(), "{}", said(&closed));
    assert!(closed.stderr.is_empty(), "{}", said(&closed));
}

#[test]
fn an_unknown_profile_is_refused_before_the_server_starts() {
    let project = Project::new(&[CONFIG, USERS, CRUD]);
    let refused = project.vc(&["mcp", "--profile", "nope"]).write_stdin("").output().unwrap();
    assert_eq!(refused.status.code(), Some(2), "{}", said(&refused));
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(stderr.contains("error: --profile: no profile `nope`"), "{stderr}");
    assert!(stderr.contains("  = help: the profiles are dev\n"), "{stderr}");
    assert!(refused.stdout.is_empty(), "stdout carries only the protocol: {}", said(&refused));
}

/// A profile from any layer is checked, not only `--profile`; the user config's counts only
/// when the project has no default profile (D129).
#[test]
fn an_unknown_profile_from_the_environment_or_the_user_config_is_refused() {
    let project = Project::new(&[CONFIG, USERS, CRUD]);
    let from_env = project.vc(&["mcp"]).env("VIB_PROFILE", "nope").write_stdin("").output().unwrap();
    assert_eq!(from_env.status.code(), Some(2), "{}", said(&from_env));
    assert!(said(&from_env).contains("error: VIB_PROFILE: no profile `nope`"), "{}", said(&from_env));
    assert!(from_env.stdout.is_empty(), "stdout carries only the protocol: {}", said(&from_env));
    project.write(CONFIG.0, &CONFIG.1.replace("    default: true\n", ""));
    let user = project.home().join("vibranium");
    std::fs::create_dir_all(&user).unwrap();
    std::fs::write(user.join("config.yaml"), "profile: nope\n").unwrap();
    let from_user = project.vc(&["mcp"]).write_stdin("").output().unwrap();
    assert_eq!(from_user.status.code(), Some(2), "{}", said(&from_user));
    assert!(said(&from_user).contains("no profile `nope`"), "{}", said(&from_user));
    assert!(said(&from_user).contains("  = help: the profiles are dev\n"), "{}", said(&from_user));
    assert!(from_user.stdout.is_empty(), "stdout carries only the protocol: {}", said(&from_user));
}

#[test]
fn a_bad_set_is_refused_before_the_server_starts() {
    let project = Project::new(&[CONFIG, USERS, CRUD]);
    let refused = project.vc(&["mcp", "--set", "colour=red"]).write_stdin("").output().unwrap();
    assert_eq!(refused.status.code(), Some(2), "{}", said(&refused));
    assert!(said(&refused).contains("error: --set: `colour`: not a setting"), "{}", said(&refused));
    assert!(refused.stdout.is_empty(), "stdout carries only the protocol: {}", said(&refused));
}

#[test]
fn a_known_profile_serves_as_before() {
    let project = Project::new(&[CONFIG, USERS, CRUD]);
    let served = project.vc(&["mcp", "--profile", "dev"]).write_stdin("").output().unwrap();
    assert_eq!(served.status.code(), Some(0), "{}", said(&served));
}

#[tokio::test(flavor = "multi_thread")]
async fn approve_always_says_whether_it_kept_a_rule() {
    let server = MockServer::start().await.unwrap();
    let asks = AGENTS.replace("DELETE]}", "DELETE], writeDefinitions: ask}");
    let config = CONFIG
        .1
        .replace("http://x", &server.base_url())
        .replace("    default: true\n", &format!("    default: true\n{asks}"));
    let project = Project::new(&[USERS, CRUD]);
    project.write(CONFIG.0, &config);
    let mut agent = Agent::start(&project);

    let (wrote, said) = std::thread::scope(|scope| {
        let said = scope.spawn(|| approve_when_asked(&project, "write_definition", &["approve", "1", "--always"]));
        (agent.call("write_definition", json!({"path": CRUD.0, "text": CRUD.1})), said.join().unwrap())
    });
    assert_eq!(wrote["isError"], false, "{wrote}");
    assert_eq!(
        said.trim(),
        "✓ approved request #1 once; no rule kept: a write or a run that reaches code asks every time"
    );

    let (created, said) = std::thread::scope(|scope| {
        let said = scope.spawn(|| approve_when_asked(&project, "call_api", &["approve", "2", "--always"]));
        (agent.call("call_api", json!({"api": "users#create"})), said.join().unwrap())
    });
    assert_eq!(created["isError"], false, "{created}");
    assert!(said.starts_with("✓ approved request #2, and POST"), "{said}");
    assert!(said.trim_end().ends_with(" calls like it from now on; a write or code still asks"), "{said}");
    assert_eq!(agent.stop(), Some(0));
}

/// The `agent` column of the action log.
fn logged_agents(project: &Project) -> Vec<String> {
    let db = rusqlite::Connection::open(project.path().join(".vibranium/history.db")).unwrap();
    let mut rows = db.prepare("SELECT agent FROM agent_actions ORDER BY id").unwrap();
    rows.query_map([], |row| row.get(0)).unwrap().map(Result::unwrap).collect()
}

/// rmcp needs `clientInfo`; the server fills it in, so a client that names itself nowhere is
/// served and logged as `agent`, as is one that sends an empty name.
#[test]
fn a_client_without_client_info_is_served_and_logged_as_agent() {
    let project = Project::new(&[CONFIG, USERS, CRUD]);
    let mut unnamed = Agent::hello(&project, json!({"protocolVersion": "2025-11-25", "capabilities": {}}));
    let found = unnamed.call("search", json!({"query": "list"}));
    assert_eq!(found["isError"], false, "{found}");
    assert_eq!(unnamed.stop(), Some(0));
    let empty = json!({"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": ""}});
    let mut nameless = Agent::hello(&project, empty);
    nameless.call("search", json!({"query": "list"}));
    assert_eq!(nameless.stop(), Some(0));
    assert_eq!(logged_agents(&project), ["agent", "agent"]);
}

/// An `initialize` rmcp cannot read gets a JSON-RPC error and the server keeps serving; the
/// `notifications/initialized` sent after it breaks the handshake: exit 2 naming what came.
#[test]
fn a_client_that_breaks_the_handshake_is_a_user_error_not_a_bug() {
    let project = Project::new(&[CONFIG, USERS, CRUD]);
    let bad = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"clientInfo": {"name": "x"}}});
    let initialized = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
    let broken = project.vc(&["mcp"]).write_stdin(format!("{bad}\n{initialized}\n")).output().unwrap();
    assert_eq!(broken.status.code(), Some(2), "{}", said(&broken));
    let answer: Value = serde_json::from_slice(&broken.stdout).unwrap();
    assert_eq!((answer["id"].clone(), answer["error"]["code"].clone()), (json!(1), json!(-32602)), "{answer}");
    let stderr = String::from_utf8_lossy(&broken.stderr);
    let error = "error: the MCP client broke the handshake: it sent `notifications/initialized` before an \
        `initialize` was answered\n  = help: send `initialize` first, and `notifications/initialized` once it is answered\n";
    assert_eq!(stderr, error);

    let first = project.vc(&["mcp"]).write_stdin(format!("{initialized}\n")).output().unwrap();
    assert_eq!(first.status.code(), Some(2), "{}", said(&first));
    assert!(said(&first).contains("it sent `notifications/initialized` before"), "{}", said(&first));
    assert!(!said(&first).contains("this is a bug"), "{}", said(&first));
}
