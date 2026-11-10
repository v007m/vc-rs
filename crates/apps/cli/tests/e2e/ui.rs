//! `vc ui` (G11, G12). Without the `web-ui` feature it refuses with exit 2. With it, the token is
//! printed once on stdout, signs a browser in, lets an MCP client run tests at `/mcp`, and appears
//! in no log, error, report or history file, even under `RUST_LOG=trace`.

#[cfg(not(feature = "web-ui"))]
#[test]
fn a_build_without_the_web_ui_refuses_with_exit_2() {
    let project = crate::Project::new(&[crate::CONFIG, crate::USERS]);
    let output = project.vc(&["ui", "--no-open"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", crate::said(&output));
    assert!(String::from_utf8_lossy(&output.stderr).contains("built without UI"), "{}", crate::said(&output));
}

#[cfg(all(feature = "web-ui", unix))]
mod served {
    use std::io::{BufRead as _, BufReader, Read as _, Write as _};
    use std::net::TcpStream;
    use std::path::Path;
    use std::process::{Command, Stdio};
    use std::time::Duration;

    use assert_cmd::cargo::cargo_bin;
    use rustix::process::{Pid, Signal, kill_process};
    use vc_mock_server::MockServer;

    use crate::{CONFIG, Project, USERS};

    const CRUD: (&str, &str) = ("services/users/crud.test.yaml", "tests:\n  - id: listsUsers\n    call: users#list\n");
    const ACCEPT: &str = "Accept: application/json, text/event-stream\r\nContent-Type: application/json\r\n";

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

    /// Initializes an MCP session over `/mcp` and runs `users.crud`; the run's reply.
    fn run_over_mcp(port: &str, token: &str) -> String {
        let auth = format!("POST /mcp\r\nAuthorization: Bearer {token}\r\n{ACCEPT}");
        let auth = auth.trim_end();
        let hello = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"ui-agent","version":"1"}}}"#;
        let started = http(port, auth, hello);
        assert!(started.starts_with("HTTP/1.1 200"), "{started}");
        let session = header(&started, "mcp-session-id").unwrap().to_owned();
        let auth = format!("{auth}\r\nMcp-Session-Id: {session}\r\nMcp-Protocol-Version: 2025-11-25");
        http(port, &auth, r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
        let run = r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"run_tests","arguments":{"ids":["users.crud"]}}}"#;
        http(port, &auth, run)
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

    #[tokio::test(flavor = "multi_thread")]
    async fn the_token_signs_in_and_serves_mcp_yet_never_reaches_a_log_report_or_history() {
        let server = MockServer::start().await.unwrap();
        let config = CONFIG
            .1
            .replace("http://x", &server.base_url())
            .replace("default: true\n", "default: true\n    agents: {run: true, call: [GET]}\n");
        let project = Project::new(&[USERS, CRUD]);
        project.write(CONFIG.0, &config);
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
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        let (port, token) =
            line.trim_end().strip_prefix("vc ui: http://127.0.0.1:").unwrap().split_once("/?token=").unwrap();
        let (port, token) = (port.to_owned(), token.to_owned());
        assert_eq!(token.len(), 43, "{line}");

        let signed_in = http(&port, &format!("GET /?token={token}"), "");
        assert!(signed_in.starts_with("HTTP/1.1 303") && signed_in.contains("vc_token="), "{signed_in}");
        let elsewhere = http(&port, &format!("GET /runs?token={token}"), "");
        assert!(elsewhere.starts_with("HTTP/1.1 400") && !elsewhere.contains(&token), "{elsewhere}");
        assert!(http(&port, "GET /?token=wrong", "").starts_with("HTTP/1.1 401"));
        let ran = run_over_mcp(&port, &token);
        assert!(ran.contains(r#"\"exit\":0"#) || ran.contains(r#""exit":0"#), "{ran}");

        let pid = Pid::from_raw(i32::try_from(child.id()).unwrap()).unwrap();
        kill_process(pid, Signal::INT).unwrap();
        assert_eq!(child.wait().unwrap().code(), Some(0));
        let mut rest = String::new();
        stdout.read_to_string(&mut rest).unwrap();
        let logs = logs.join().unwrap();
        assert!(!rest.contains(&token) && !logs.contains(&token), "stdout: {rest}\nstderr: {logs}");
        let leaked = [project.path(), project.home()].map(|dir| files_holding(dir, token.as_bytes()));
        assert_eq!(leaked, [Vec::<String>::new(), Vec::new()]);
        assert_eq!(server.stats().await.unwrap().count("GET /users"), 1);
    }
}
