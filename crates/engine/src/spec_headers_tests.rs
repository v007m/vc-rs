//! `vc sync` sends an api file's `source.headers`, rendered under `--profile`, to its spec's
//! origin only, and no rendered value reaches what it writes or says (D313).

use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::Arc;

use serde_json::json;
use tokio::sync::mpsc;
use vc_interfaces::{SourceProvider, TransportError};
use vc_model::RenderedRequest;
use vc_testkit::{FakeCommands, FakeTransport, FakeVcs, MemoryCompileCache, MemoryEnvironment, MemorySources, status};

use super::{SpecHeaders, origin};
use crate::{Engine, EngineBuilder, ImportRequest, SyncRequest};

const TOKEN: &str = "Kd4R-8wNq-Yb6T-2hVz-Jc5M";
const WIDE: &str = "Žetón-ключ-秘密-7731";
const CONFIG: &str = "syntaxVersion: 2\nvars: {usersUrl: 'http://x', team: core}\nprofiles:\n  - id: dev\n    \
                      default: true\n  - id: staging\n    vars: {team: blue}\n";

fn api(headers: &str) -> String {
    format!(
        "baseUrl: ${{var.usersUrl}}\nsource:\n  openapi: https://specs.test/users.json\n  headers:\n{headers}apis:\n  \
         - id: listUsers\n    path: /users\n    operationId: listUsers\n"
    )
}

const SIGNED: &str = "    authorization: Bearer ${secret.SPEC_TOKEN}\n    x-team: ${var.team}\n";

/// The spec, whose response schema is a `$ref` to another host.
fn spec() -> String {
    let user = json!({ "$ref": "https://schemas.test/user.json" });
    let ok = json!({ "200": { "description": "ok", "content": { "application/json": { "schema": user } } } });
    json!({
        "openapi": "3.1.0",
        "info": { "title": "Users", "version": "1" },
        "paths": { "/users": { "get": { "operationId": "listUsers", "responses": ok } } }
    })
    .to_string()
}

/// Each request sent, as `url` and its headers.
type Sent = mpsc::UnboundedReceiver<(String, Vec<(String, String)>)>;

fn synced(api: &str, failure: Option<String>) -> (Engine, Sent) {
    with_config(CONFIG, api, failure)
}

/// The project of `config` and the `users` api file `api`, its spec fetched through a transport
/// that fails with `failure` when given.
fn with_config(config: &str, api: &str, failure: Option<String>) -> (Engine, Sent) {
    let files: [(&str, &[u8]); 3] = [
        ("vibranium.config.yaml", config.as_bytes()),
        (".env.schema", b"# the spec's token\nSPEC_TOKEN=\n# a wide one\nWIDE_TOKEN=\n# never set\nUNSET_TOKEN=\n"),
        ("services/users/users.api.yaml", api.as_bytes()),
    ];
    let sources: Arc<dyn SourceProvider> = Arc::new(MemorySources::new(&files));
    let (sender, sent) = mpsc::unbounded_channel();
    let transport = FakeTransport::new(move |request: &RenderedRequest, _| {
        let headers =
            request.headers.iter().map(|(name, value)| (name.to_string(), value.to_str().unwrap_or("?").to_owned()));
        sender.send((request.uri.to_string(), headers.collect())).ok();
        if let Some(reason) = &failure {
            return Err(TransportError::InvalidRequest { reason: reason.clone() });
        }
        let mut response = status(200);
        response.body =
            if request.uri.host() == Some("specs.test") { spec() } else { json!({"type": "object"}).to_string() }
                .into();
        Ok(response)
    });
    let engine = EngineBuilder::new(PathBuf::from("/project"))
        .sources(sources)
        .compile_cache(Arc::new(MemoryCompileCache::new()))
        .environment(Arc::new(MemoryEnvironment::new(&[("SPEC_TOKEN", TOKEN), ("WIDE_TOKEN", WIDE)])))
        .commands(Arc::new(FakeCommands::new(|_| None)))
        .vcs(Arc::new(FakeVcs::default()))
        .transport(Arc::new(transport))
        .build()
        .unwrap();
    (engine, sent)
}

fn staging() -> SyncRequest {
    SyncRequest { profiles: vec!["staging".to_owned()], ..SyncRequest::default() }
}

fn header<'h>(headers: &'h [(String, String)], name: &str) -> Option<&'h str> {
    headers.iter().find(|(named, _)| named == name).map(|(_, value)| value.as_str())
}

#[tokio::test]
async fn the_headers_render_under_the_profile_and_go_to_the_spec_s_origin_only() {
    let (engine, mut sent) = synced(&api(SIGNED), None);
    let report = engine.sync(&staging()).await.unwrap();
    let sent: Vec<_> = std::iter::from_fn(|| sent.try_recv().ok()).collect();
    let (spec, others): (Vec<_>, Vec<_>) = sent.iter().partition(|(url, _)| url.starts_with("https://specs.test/"));
    let bearer = format!("Bearer {TOKEN}");
    assert_eq!(spec.len(), 1, "{sent:?}");
    assert_eq!(header(&spec[0].1, "authorization"), Some(bearer.as_str()));
    assert_eq!(header(&spec[0].1, "x-team"), Some("blue"), "the staging profile's var");
    assert!(!others.is_empty() && others.iter().all(|(_, headers)| headers.is_empty()), "{others:?}");
    let written: String = report.changes.files.iter().map(|file| file.after.as_str()).collect();
    let said = report.changes.titles.join("\n");
    assert!(!written.contains(TOKEN) && !said.contains(TOKEN), "{said}\n{written}");
    let (engine, mut sent) = synced(&api(SIGNED), None);
    engine.sync(&SyncRequest::default()).await.unwrap();
    let first = sent.try_recv().unwrap();
    assert_eq!(header(&first.1, "x-team"), Some("core"), "the default profile's: {first:?}");
}

#[tokio::test]
async fn a_failed_fetch_never_shows_a_header_s_value() {
    let (engine, _sent) = synced(&api(SIGNED), Some(format!("refused: Bearer {TOKEN}")));
    let error = engine.sync(&staging()).await.unwrap_err().to_string();
    assert!(error.contains("refused: Bearer ••••") && !error.contains(TOKEN), "{error}");
    let headers = "    authorization: ${secret.SPEC_TOKEN}\n    x-key: ${secret.WIDE_TOKEN}\n    x-retry: '502'\n";
    let (engine, _sent) = synced(&api(headers), Some(format!("refused {TOKEN} and {WIDE} after 502")));
    let error = engine.sync(&staging()).await.unwrap_err().to_string();
    assert!(error.contains("refused •••• and •••• after 502"), "a bare and a UTF-8 secret, not a plain value: {error}");
}

#[tokio::test]
async fn a_header_that_does_not_render_is_refused_by_name_before_any_fetch() {
    let cases = [
        ("    authorization: Bearer ${secret.UNSET_TOKEN}\n", "`secret.UNSET_TOKEN` is not loaded"),
        ("    x-run: ${run.id}\n", "read only `${var.…}`, `${env.…}` and `${secret.…}`"),
        ("    x-team: ${var.nobody}\n", "`var.nobody` is not set under the active profiles"),
    ];
    for (headers, reason) in cases {
        let (engine, mut sent) = synced(&api(headers), None);
        let error = engine.sync(&staging()).await.unwrap_err().to_string();
        assert!(error.contains("`users`'s `source.headers.") && error.contains(reason), "{headers}: {error}");
        assert!(!error.contains(TOKEN) && sent.try_recv().is_err(), "{headers}: {error}");
    }
    let (engine, mut sent) = synced(&api("    authorization: Bearer ${env.SPEC_TOKEN}\n"), None);
    let error = engine.sync(&staging()).await.unwrap_err().to_string();
    assert!(
        error.contains("1 error(s)") && sent.try_recv().is_err(),
        "V2107 refuses a declared secret read as env: {error}"
    );
}

#[tokio::test]
async fn an_unknown_profile_is_refused_even_when_no_file_has_headers() {
    let (engine, _sent) =
        synced(&api("    accept: application/json\n").replace("  headers:\n    accept: application/json\n", ""), None);
    let unknown = SyncRequest { profiles: vec!["prod".to_owned()], ..SyncRequest::default() };
    let error = engine.sync(&unknown).await.unwrap_err().to_string();
    assert!(error.contains("prod"), "{error}");
}

#[tokio::test]
async fn secrets_load_only_when_a_header_reads_one_through_its_vars() {
    let varlock = CONFIG.replace("syntaxVersion: 2\n", "syntaxVersion: 2\nsecrets: {provider: varlock}\n");
    let (engine, mut sent) = with_config(&varlock, &api("    x-team: ${var.team}\n"), None);
    engine.sync(&staging()).await.unwrap();
    assert_eq!(header(&sent.try_recv().unwrap().1, "x-team"), Some("blue"), "the provider is never asked");
    let through = CONFIG.replace("team: core}", "team: core, signed: 'Bearer ${secret.SPEC_TOKEN}'}");
    let (engine, mut sent) = with_config(&through, &api("    authorization: ${var.signed}\n"), None);
    engine.sync(&staging()).await.unwrap();
    let bearer = format!("Bearer {TOKEN}");
    assert_eq!(header(&sent.try_recv().unwrap().1, "authorization"), Some(bearer.as_str()));
    let (engine, _sent) = with_config(
        &varlock.replace("team: core}", "team: core, signed: '${secret.SPEC_TOKEN}'}"),
        &api("    authorization: ${var.signed}\n"),
        None,
    );
    assert!(engine.sync(&staging()).await.is_err(), "a secret read through a var loads them");
}

#[tokio::test]
async fn a_var_chain_past_eight_levels_or_back_to_itself_is_refused_as_such() {
    let chain = (1..=9).fold(String::new(), |mut chain, level| {
        write!(chain, "v{level}: '${{var.v{}}}', ", level + 1).unwrap();
        chain
    });
    let deep = CONFIG.replace("team: core}", &format!("team: core, {chain}v10: end}}"));
    let (engine, _sent) = with_config(&deep, &api("    x-deep: ${var.v1}\n"), None);
    let error = engine.sync(&staging()).await.unwrap_err().to_string();
    assert!(error.contains("`var.v9` nests deeper than 8 levels") && !error.contains("itself"), "{error}");
    let (engine, mut sent) = with_config(&deep, &api("    x-deep: ${var.v3}\n"), None);
    engine.sync(&staging()).await.unwrap();
    assert_eq!(header(&sent.try_recv().unwrap().1, "x-deep"), Some("end"), "eight levels render");
    let cycle = CONFIG.replace("team: core}", "team: core, a: '${var.b}', b: '${var.a}'}");
    let (engine, _sent) = with_config(&cycle, &api("    x-cycle: ${var.a}\n"), None);
    let error = engine.sync(&staging()).await.unwrap_err().to_string();
    assert!(error.contains("`var.a` refers to itself: `var.a` → `var.b` → `var.a`"), "{error}");
}

#[test]
fn headers_go_to_their_origin_in_any_case_but_not_to_another_port_or_scheme() {
    let mut headers = http::HeaderMap::new();
    headers.insert("authorization", http::HeaderValue::from_static("Bearer t0k3n"));
    let at = SpecHeaders { origin: "https://specs.test".to_owned(), headers, redactor: None };
    let uri = |text: &str| text.parse::<http::Uri>().unwrap();
    let same =
        ["https://SPECS.test/v2/common.json", "HTTPS://specs.test:443/a.json", "https://reader@specs.test/a.json"];
    for at_origin in same {
        assert!(at.for_uri(&uri(at_origin)).is_some(), "{at_origin}");
    }
    for other in ["https://specs.test:8443/a.json", "http://specs.test/a.json", "https://specs.test.evil/a.json"] {
        assert!(at.for_uri(&uri(other)).is_none(), "{other}");
    }
    assert_eq!(origin(&uri("http://specs.test:80/a")).as_deref(), Some("http://specs.test"));
    assert_eq!(origin(&uri("/relative")), None);
    assert_eq!(at.masked("sent Bearer t0k3n"), "sent Bearer t0k3n", "only the secrets read are masked");
}

/// `vc import openapi https://specs.test/users.json --id people` with `headers`.
fn imported(headers: &[(&str, &str)]) -> ImportRequest {
    ImportRequest {
        kind: "openapi".to_owned(),
        source: "https://specs.test/users.json".to_owned(),
        file_id: Some("people".to_owned()),
        profiles: vec!["staging".to_owned()],
        headers: headers.iter().map(|&(name, value)| (name.to_owned(), value.to_owned())).collect(),
        ..ImportRequest::default()
    }
}

#[tokio::test]
async fn an_import_sends_its_header_s_to_the_spec_s_origin_only_and_writes_the_templates() {
    let (engine, mut sent) = synced(&api(SIGNED), None);
    let given =
        [("Authorization", "Bearer ${secret.SPEC_TOKEN}"), ("Accept", "application/json"), ("x-team", "${var.team}")];
    let changes = engine.import(&imported(&given)).await.unwrap();
    let sent: Vec<_> = std::iter::from_fn(|| sent.try_recv().ok()).collect();
    let (spec, others): (Vec<_>, Vec<_>) = sent.iter().partition(|(url, _)| url.starts_with("https://specs.test/"));
    let bearer = format!("Bearer {TOKEN}");
    assert_eq!(spec.len(), 1, "{sent:?}");
    assert_eq!(header(&spec[0].1, "authorization"), Some(bearer.as_str()));
    assert_eq!(header(&spec[0].1, "accept"), Some("application/json"), "a literal that is no credential");
    assert_eq!(header(&spec[0].1, "x-team"), Some("blue"), "the staging profile's var");
    assert!(!others.is_empty() && others.iter().all(|(_, headers)| headers.is_empty()), "{others:?}");
    let people = changes.files.iter().find(|file| file.path == "services/people/people.api.yaml").unwrap();
    let source = "source:\n  openapi: https://specs.test/users.json\n  headers:\n    Authorization: Bearer \
                  ${secret.SPEC_TOKEN}\n    Accept: application/json\n    x-team: ${var.team}\n";
    assert!(people.after.contains(source), "{}", people.after);
    let written: String = changes.files.iter().map(|file| file.after.as_str()).collect();
    assert!(!written.contains(TOKEN) && !changes.titles.join("\n").contains(TOKEN), "{written}");
}

#[tokio::test]
async fn an_import_refuses_a_credential_given_as_a_literal_with_v4009_and_never_echoes_it() {
    for name in ["Authorization", "x-api-key", "Cookie", "X-Session-Token"] {
        let (engine, mut sent) = synced(&api(SIGNED), None);
        let refused = engine.import(&imported(&[(name, &format!("Bearer {TOKEN}"))])).await.unwrap_err();
        let (error, help) = (refused.to_string(), refused.help().unwrap_or_default());
        assert!(error.contains("V4009") && error.contains(&format!("`--header {name}`")), "{name}: {error}");
        assert!(help.contains("${secret.NAME}") && !format!("{error}{help}").contains(TOKEN), "{name}: {help}");
        assert!(sent.try_recv().is_err(), "{name}: nothing is fetched");
    }
}

#[tokio::test]
async fn an_import_refuses_a_header_it_cannot_send_or_render_before_any_fetch() {
    let pasted = ImportRequest { text: Some(spec()), ..imported(&[("accept", "application/json")]) };
    let cases = [
        (pasted, "is sent only with a spec fetched from a URL"),
        (imported(&[("accept", "a"), ("Accept", "b")]), "`--header Accept` is given twice"),
        (imported(&[("x-team", "${var.team")]), "`--header x-team` is not a valid template"),
        (imported(&[("authorization", "Bearer ${secret.UNSET_TOKEN}")]), "`secret.UNSET_TOKEN` is not loaded"),
        (imported(&[("x-team", "${var.nobody}")]), "`--header x-team` does not render: `var.nobody` is not set"),
    ];
    for (request, reason) in cases {
        let (engine, mut sent) = synced(&api(SIGNED), None);
        let error = engine.import(&request).await.unwrap_err().to_string();
        assert!(error.contains(reason), "{reason}: {error}");
        assert!(sent.try_recv().is_err(), "{reason}: nothing is fetched");
    }
}
