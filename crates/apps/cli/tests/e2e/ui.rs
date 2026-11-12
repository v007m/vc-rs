//! `vc ui` (G11, G12, D239–D241). Without the `web-ui` feature it refuses with exit 2. With it,
//! the launch link and the agent token are printed once on stdout. The link signs a browser in
//! once; the agent token lets an MCP client run tests at `/mcp`, under the agent policy and log,
//! and opens nothing else. No secret appears in a log, error, report or history file, even under
//! `RUST_LOG=trace`.

#[cfg(not(feature = "web-ui"))]
#[test]
fn a_build_without_the_web_ui_refuses_with_exit_2() {
    let project = crate::Project::new(&[crate::CONFIG, crate::USERS]);
    let output = project.vc(&["ui", "--no-open"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", crate::said(&output));
    assert!(String::from_utf8_lossy(&output.stderr).contains("built without UI"), "{}", crate::said(&output));
    let install = "help: install vc with the UI from a clone: https://v007m.github.io/vc-rs/start/install/";
    assert!(String::from_utf8_lossy(&output.stderr).contains(install), "{}", crate::said(&output));
}

#[cfg(all(feature = "web-ui", unix))]
mod served {
    use std::io::{BufRead as _, BufReader, Read as _, Write as _};
    use std::net::TcpStream;
    use std::path::Path;
    use std::process::{Child, ChildStdout, Command, Stdio};
    use std::thread::JoinHandle;
    use std::time::Duration;

    use assert_cmd::cargo::cargo_bin;
    use rustix::process::{Pid, Signal, kill_process};
    use vc_mock_server::MockServer;

    use crate::{CONFIG, Project, USERS};

    const CRUD: (&str, &str) = ("services/users/crud.test.yaml", "tests:\n  - id: listsUsers\n    call: users#list\n");
    const ACCEPT: &str = "Accept: application/json, text/event-stream\r\nContent-Type: application/json\r\n";
    const ENGINE: &str = "/api/vibranium.engine.v1.EngineService";
    const HELLO: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"ui-agent","version":"1"}}}"#;
    const RUN: &str = r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"run_tests","arguments":{"ids":["users.crud"]}}}"#;
    const CREATE: &str = r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"call_api","arguments":{"api":"users#create"}}}"#;
    const SEARCH: &str =
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"search","arguments":{"query":"list"}}}"#;

    /// One HTTP/1.1 exchange on loopback `port`; the whole reply. `head` is the method and path,
    /// then any further header lines.
    fn http(port: &str, head: &str, body: &str) -> String {
        let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
        let (line, headers) =
            head.split_once("\r\n").map_or((head, String::new()), |(line, rest)| (line, format!("{rest}\r\n")));
        let request = format!(
            "{line} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{headers}Connection: close\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(request.as_bytes()).unwrap();
        let mut reply = Vec::new();
        let _ = stream.read_to_end(&mut reply);
        String::from_utf8_lossy(&reply).into_owned()
    }

    fn header<'r>(reply: &'r str, name: &str) -> Option<&'r str> {
        let head = reply.split("\r\n\r\n").next()?;
        head.lines()
            .find_map(|line| line.split_once(':').filter(|(key, _)| key.eq_ignore_ascii_case(name)))
            .map(|(_, value)| value.trim())
    }

    /// Initializes an MCP session over `/mcp` with `token` and calls a tool with `call`; its reply.
    fn mcp_tool(port: &str, token: &str, call: &str) -> String {
        mcp_tool_after(port, token, HELLO, call)
    }

    /// As [`mcp_tool`], with `hello` as the `initialize`.
    fn mcp_tool_after(port: &str, token: &str, hello: &str, call: &str) -> String {
        let auth = format!("POST /mcp\r\nAuthorization: Bearer {token}\r\n{ACCEPT}");
        let auth = auth.trim_end();
        let started = http(port, auth, hello);
        assert!(started.starts_with("HTTP/1.1 200"), "{started}");
        let session = header(&started, "mcp-session-id").unwrap().to_owned();
        let auth = format!("{auth}\r\nMcp-Session-Id: {session}\r\nMcp-Protocol-Version: 2025-11-25");
        http(port, &auth, r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
        http(port, &auth, call)
    }

    /// The browser's unary call of `method` with `cookie`; the whole reply.
    fn rpc(port: &str, method: &str, cookie: &str) -> String {
        let head = format!(
            "POST {ENGINE}/{method}\r\nCookie: {cookie}\r\nOrigin: http://127.0.0.1:{port}\r\nContent-Type: application/proto"
        );
        http(port, &head, "")
    }

    /// Every file under `dir` that holds `needle`.
    fn files_holding(dir: &Path, needle: &[u8]) -> Vec<String> {
        let mut found = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap().map(Result::unwrap) {
            let path = entry.path();
            if path.is_dir() {
                found.extend(files_holding(&path, needle));
            } else if std::fs::read(&path).unwrap().windows(needle.len()).any(|window| window == needle) {
                found.push(path.display().to_string());
            }
        }
        found
    }

    /// A project whose default profile lets agents run tests and send only `GET`, against `base`.
    fn project(base: &str) -> Project {
        let config = CONFIG
            .1
            .replace("http://x", base)
            .replace("default: true\n", "default: true\n    agents: {run: true, call: [GET]}\n");
        let project = Project::new(&[USERS, CRUD]);
        project.write(CONFIG.0, &config);
        project
    }

    /// `vc ui` serving a project under `RUST_LOG=trace`, with its two stdout lines read.
    struct Ui {
        child: Child,
        stdout: BufReader<ChildStdout>,
        logs: JoinHandle<String>,
        port: String,
        launch: String,
        agent: String,
    }

    impl Ui {
        fn start(project: &Project) -> Self {
            let mut vc = Command::new(cargo_bin!("vc"));
            vc.current_dir(project.path()).args(["ui", "--no-open", "--port", "0", "--profile", "dev"]);
            vc.env("HOME", project.home()).env("XDG_CONFIG_HOME", project.home()).env("RUST_LOG", "trace");
            let mut child = vc.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
            let mut stdout = BufReader::new(child.stdout.take().unwrap());
            let mut stderr = child.stderr.take().unwrap();
            let logs = std::thread::spawn(move || {
                let mut text = String::new();
                stderr.read_to_string(&mut text).map(|_| text).unwrap()
            });
            let (mut first, mut second) = (String::new(), String::new());
            stdout.read_line(&mut first).unwrap();
            stdout.read_line(&mut second).unwrap();
            let rest = first.trim_end().strip_prefix("vc ui: http://127.0.0.1:").unwrap();
            let (port, launch) = rest.split_once("/?token=").unwrap();
            let agent =
                second.trim_end().strip_prefix(&format!("vc ui: mcp http://127.0.0.1:{port}/mcp token ")).unwrap();
            assert!(launch.len() == 43 && agent.len() == 43 && launch != agent, "{first}{second}");
            let (port, launch, agent) = (port.to_owned(), launch.to_owned(), agent.to_owned());
            Self { child, stdout, logs, port, launch, agent }
        }

        /// Trades the launch link for the cookie; `vc_token_<port>=<value>`.
        fn sign_in(&self) -> String {
            let signed_in = http(&self.port, &format!("GET /?token={}", self.launch), "");
            assert!(signed_in.starts_with("HTTP/1.1 303"), "{signed_in}");
            let cookie = header(&signed_in, "set-cookie").unwrap().split(';').next().unwrap();
            assert!(cookie.starts_with(&format!("vc_token_{}=", self.port)), "{signed_in}");
            cookie.to_owned()
        }

        /// Ends it with Ctrl-C and exit 0; what it printed after its two lines, then its stderr.
        fn stop(mut self) -> (String, String) {
            let pid = Pid::from_raw(i32::try_from(self.child.id()).unwrap()).unwrap();
            kill_process(pid, Signal::INT).unwrap();
            assert_eq!(self.child.wait().unwrap().code(), Some(0));
            let mut rest = String::new();
            self.stdout.read_to_string(&mut rest).unwrap();
            (rest, self.logs.join().unwrap())
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_secrets_sign_in_once_and_serve_mcp_yet_never_reach_a_log_report_or_history() {
        let server = MockServer::start().await.unwrap();
        let project = project(&server.base_url());
        let ui = Ui::start(&project);
        let port = ui.port.clone();
        let cookie = ui.sign_in();
        let again = http(&port, &format!("GET /?token={}", ui.launch), "");
        assert!(again.starts_with("HTTP/1.1 401") && again.contains("restart vc ui"), "{again}");
        let elsewhere = http(&port, &format!("GET /runs?token={}", ui.launch), "");
        assert!(elsewhere.starts_with("HTTP/1.1 400") && !elsewhere.contains(&ui.launch), "{elsewhere}");
        assert!(http(&port, "GET /?token=wrong", "").starts_with("HTTP/1.1 401"));
        let ran = mcp_tool(&port, &ui.agent, RUN);
        assert!(ran.contains(r#"\"exit\":0"#) || ran.contains(r#""exit":0"#), "{ran}");

        let session = cookie.split_once('=').unwrap().1.to_owned();
        let secrets = [ui.launch.clone(), ui.agent.clone(), session];
        let (rest, logs) = ui.stop();
        for secret in &secrets {
            assert!(!rest.contains(secret) && !logs.contains(secret), "stdout: {rest}\nstderr: {logs}");
            let leaked = [project.path(), project.home()].map(|dir| files_holding(dir, secret.as_bytes()));
            assert_eq!(leaked, [Vec::<String>::new(), Vec::new()]);
        }
        assert_eq!(server.stats().await.unwrap().count("GET /users"), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn each_secret_opens_only_its_door_and_mcp_goes_through_the_agent_policy_and_log() {
        let server = MockServer::start().await.unwrap();
        let project = project(&server.base_url());
        let ui = Ui::start(&project);
        let port = ui.port.clone();
        let cookie = ui.sign_in();
        let as_agent = format!(
            "POST {ENGINE}/GetProject\r\nAuthorization: Bearer {}\r\nContent-Type: application/proto",
            ui.agent
        );
        assert!(http(&port, &as_agent, "").starts_with("HTTP/1.1 401"));
        let at_mcp = format!("POST /mcp\r\nCookie: {cookie}\r\nOrigin: http://127.0.0.1:{port}\r\n{ACCEPT}");
        assert!(http(&port, at_mcp.trim_end(), HELLO).starts_with("HTTP/1.1 401"));
        assert!(rpc(&port, "GetProject", &cookie).starts_with("HTTP/1.1 200"));

        let refused = mcp_tool(&port, &ui.agent, CREATE);
        assert!(refused.contains(r#""isError":true"#), "the policy lets agents send only GET: {refused}");
        mcp_tool(&port, &ui.agent, RUN);
        let logged = rpc(&port, "ListAgentActions", &cookie);
        let each = ["ui-agent", "call_api", "refused", "run_tests"].map(|word| logged.contains(word));
        assert_eq!(each, [true; 4], "{logged}");
        ui.stop();
        assert_eq!(server.stats().await.unwrap().count("POST /users"), 0);
    }

    /// rmcp needs `clientInfo`; `/mcp` fills it in as stdio does, so a client that names itself
    /// nowhere, or gives an empty `clientInfo`, is served and logged as `agent`.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_client_without_client_info_is_served_at_mcp_and_logged_as_agent() {
        let server = MockServer::start().await.unwrap();
        let project = project(&server.base_url());
        let ui = Ui::start(&project);
        let unnamed = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{}}}"#;
        let empty = unnamed.replace(r#""capabilities":{}"#, r#""capabilities":{},"clientInfo":{}"#);
        for hello in [unnamed, empty.as_str()] {
            let found = mcp_tool_after(&ui.port, &ui.agent, hello, SEARCH);
            assert!(found.contains(r#""isError":false"#) && found.contains("users.crud#listsUsers"), "{found}");
        }
        ui.stop();
        let db = rusqlite::Connection::open(project.path().join(".vibranium/history.db")).unwrap();
        let mut rows = db.prepare("SELECT agent, tool FROM agent_actions ORDER BY id").unwrap();
        let logged: Vec<(String, String)> =
            rows.query_map([], |row| Ok((row.get(0)?, row.get(1)?))).unwrap().map(Result::unwrap).collect();
        assert_eq!(logged, [("agent".to_owned(), "search".to_owned()), ("agent".to_owned(), "search".to_owned())]);
    }
}
