//! `vc new api|test|auth|file` append to the file the ID derives from (D139), keep every byte
//! they do not add, and refuse an ID that breaks the conventions with a suggested one (D87).

use std::io::Write as _;
use std::process::Stdio;
use std::time::Duration;

use assert_cmd::cargo::cargo_bin;
use vc_mock_server::MockServer;

use crate::{CONFIG, CRUD, Project, USERS, checks_clean, said, schema_line};

const DEACTIVATE: &str =
    "  - id: deactivate\n    name: Deactivate\n    method: POST\n    path: /users/${var.id}/deactivate\n";

#[test]
fn new_api_appends_with_path_parameters_as_vars() {
    let project = Project::new(&[CONFIG, USERS]);
    let args = ["new", "api", "users#deactivate", "--method", "POST", "--path", "/users/{id}/deactivate"];
    let output = project.vc(&args).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    let line = USERS.1.lines().count() + 2;
    let added = format!("✓ added users#deactivate to services/users/users.api.yaml:{line}\n");
    assert_eq!(String::from_utf8_lossy(&output.stdout), added, "the line of its `- id:`");
    let text = project.read(USERS.0);
    assert!(text.starts_with(USERS.1), "every existing byte is kept:\n{text}");
    let tail = &text[USERS.1.len()..];
    assert!(tail.starts_with("  # users#deactivate · POST /users/{id}/deactivate"), "a brief comment:\n{tail}");
    assert!(tail.ends_with(DEACTIVATE), "{tail}");
    checks_clean(&project);
}

/// A new file from `--path` takes its base URL from the config var `<last word>Url`, added as
/// `http://localhost` with a note to set it, or reused when the config has it; each api is named.
#[test]
fn new_api_creates_the_file_its_id_derives_from_with_a_base_url_and_a_name() {
    let project = Project::new(&[CONFIG]);
    let args = ["new", "api", "payments.refunds#create", "--method", "POST", "--path", "/refunds"];
    let output = project.vc(&args).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    let note = "  set `vars.refundsUrl` in the config to the service's URL; it is http://localhost\n";
    assert!(String::from_utf8_lossy(&output.stdout).ends_with(note), "{}", said(&output));
    let text = project.read("services/payments/refunds.api.yaml");
    assert_eq!(text.lines().next(), Some(schema_line("api").as_str()));
    assert!(text.contains("\nbaseUrl: ${var.refundsUrl}\napis:\n"), "{text}");
    assert!(text.contains("  - id: create\n    name: Create\n    method: POST\n    path: /refunds\n"), "{text}");
    let config = project.read(CONFIG.0);
    assert!(config.contains("refundsUrl: http://localhost"), "{config}");
    checks_clean(&project);

    project.write(CONFIG.0, &CONFIG.1.replace("vars: {", "vars: {ordersUrl: 'http://orders.test', "));
    let output = project.vc(&["new", "api", "orders#listOrders", "--path", "/orders"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    assert!(!said(&output).contains("set `vars."), "the config already sets it: {}", said(&output));
    let text = project.read("services/orders/orders.api.yaml");
    assert!(text.contains("\nbaseUrl: ${var.ordersUrl}\napis:\n"), "{text}");
    assert!(text.contains("  - id: listOrders\n    name: List orders\n    path: /orders\n"), "{text}");
    assert!(!project.read(CONFIG.0).contains("refundsUrl"), "the config is left as it was");
    checks_clean(&project);
}

/// An existing file with no `baseUrl` cannot take an api from `--url`, and says how to fix it.
#[test]
fn new_api_from_a_url_into_a_file_with_no_base_url_says_to_add_one() {
    let people = ("services/people/people.api.yaml", "apis:\n  - id: list\n    path: https://people.test/people\n");
    let project = Project::new(&[CONFIG, people]);
    let args = ["new", "api", "people#find", "--url", "https://people.test/people/{id}"];
    let output = project.vc(&args).output().unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", said(&output));
    let text = said(&output);
    assert!(text.contains("`people` has no `baseUrl`\n"), "{text}");
    let help = "help: add `baseUrl: ${var.peopleUrl}` to it, or use `--path` to add to \
                services/people/people.api.yaml, or another file ID for https://people.test";
    assert!(text.contains(help), "{text}");
    assert!(!text.contains("from none"), "{text}");
    assert_eq!(project.read(people.0), people.1, "nothing is written");
}

/// D177: the URL's origin becomes a var ending in `Url`, the rest the path and query; GET.
#[test]
fn new_api_from_a_full_url_splits_out_a_base_url_var() {
    let project = Project::new(&[CONFIG]);
    project.vc(&["new", "api", "search#find", "--url", "https://api.example.com/users?q=ann"]).assert().code(0);
    let text = project.read("services/search/search.api.yaml");
    let base = text.lines().find(|line| line.starts_with("baseUrl: ${var.")).unwrap_or_else(|| panic!("{text}"));
    let var = base.trim_start_matches("baseUrl: ${var.").trim_end_matches('}');
    assert!(var.ends_with("Url"), "{var}");
    assert!(
        text.contains("  - id: find\n    name: Find\n    path: /users\n") && text.contains("      q: ann\n"),
        "{text}"
    );
    assert!(!text.contains("method:") || text.contains("method: GET"), "{text}");
    let config = project.read(CONFIG.0);
    assert!(config.contains(var) && config.contains("https://api.example.com"), "{config}");
    checks_clean(&project);
}

/// D176: a copied curl command gives the method, URL, query and body; a credential is named in a
/// note and never written.
#[test]
fn new_api_from_curl_writes_what_the_command_sends_but_no_credential() {
    let project = Project::new(&[CONFIG]);
    let curl = "curl -X PATCH https://api.shop.test/users/u_7f3k -H 'authorization: Bearer eyJhbGciOi' \\\n  \
                -H 'content-type: application/json' -d '{\"name\":\"Ada\"}'";
    let output = project.vc(&["new", "api", "shop#renamesUser", "--from-curl", curl]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("✓ added shop#renamesUser to services/shop/shop.api.yaml:"), "{stdout}");
    assert!(stdout.contains("  PATCH /users/${var.userId}: header authorization: "), "{stdout}");
    let text = project.read("services/shop/shop.api.yaml");
    assert!(text.contains("    method: PATCH\n    path: /users/${var.userId}\n"), "{text}");
    let written = format!("{text}{}", project.read(CONFIG.0));
    assert!(!written.contains("eyJhbGciOi") && !stdout.contains("eyJhbGciOi"), "{written}");
    checks_clean(&project);
}

#[test]
fn new_api_refuses_an_id_that_breaks_the_convention_and_suggests_one() {
    let project = Project::new(&[CONFIG, USERS]);
    let output = project.vc(&["new", "api", "users#Deactivate_User", "--path", "/x"]).output().unwrap();
    assert!(!output.status.success());
    assert!(said(&output).contains("deactivateUser"), "{}", said(&output));
    assert_eq!(project.read(USERS.0), USERS.1);
}

#[test]
fn new_api_refuses_an_id_the_file_already_has() {
    let project = Project::new(&[CONFIG, USERS]);
    let output = project.vc(&["new", "api", "users#list", "--path", "/x"]).output().unwrap();
    assert!(!output.status.success());
    assert!(said(&output).contains("users#list"), "{}", said(&output));
    assert_eq!(project.read(USERS.0), USERS.1);
}

#[tokio::test(flavor = "multi_thread")]
async fn new_test_from_an_api_writes_a_test_that_passes() {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[CONFIG, USERS]);
    project.serve_from(&server.base_url());
    project.vc(&["new", "test", "users.crud#listsUsers", "--from", "users#list"]).assert().code(0);
    let text = project.read(CRUD.0);
    assert_eq!(text.lines().next(), Some(schema_line("test").as_str()));
    assert!(text.contains("  - id: listsUsers\n") && text.contains("users#list"), "{text}");
    let output = project.vc(&["run", "users.crud"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
}

#[test]
fn new_test_auth_adds_a_test_calling_with_no_auth() {
    let project = Project::new(&[CONFIG, USERS]);
    project.vc(&["new", "test", "users.crud#listsUsers", "--from", "users#list", "--auth"]).assert().code(0);
    let text = project.read(CRUD.0);
    let anonymous = "  - id: listRejectsAnonymous\n    name: List needs a signed-in caller\n    call: users#list\n";
    assert!(text.contains("  - id: listsUsers\n") && text.contains(anonymous), "{text}");
    assert!(text.ends_with("    auth: none\n    expect:\n      status: 401\n"), "{text}");
    checks_clean(&project);
}

/// D262: each path var with no value of its own gets a placeholder marked `# replace`, in the
/// `--auth` test too, so the new file passes `vc check`.
#[test]
fn new_test_sets_a_placeholder_for_each_path_var_so_the_file_checks() {
    let project = Project::new(&[CONFIG, USERS]);
    let args = ["new", "api", "users#deactivate", "--method", "POST", "--path", "/users/{id}/deactivate"];
    project.vc(&args).assert().code(0);
    let args = ["new", "test", "users.crud#deactivatesUser", "--from", "users#deactivate", "--auth"];
    let output = project.vc(&args).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    let text = project.read(CRUD.0);
    let vars = "    vars:\n      id: replace-me  # replace with a real id\n";
    let happy = format!("  - id: deactivatesUser\n    name: Deactivates user\n    call: users#deactivate\n{vars}");
    assert!(text.contains(&happy), "{text}");
    let anonymous = format!("    call: users#deactivate\n    auth: none\n{vars}    expect:\n      status: 401\n");
    assert!(text.ends_with(&anonymous), "{text}");
    checks_clean(&project);
}

/// D262: a path var the api gives a value (a sample from the spec, for an imported api) gets that
/// value; a var the config sets is left to the config.
#[test]
fn new_test_sets_the_apis_sample_and_leaves_a_config_var_alone() {
    let config = (
        CONFIG.0,
        "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n    vars: {orgId: o_1}\nvars: {baseUrl: 'http://x'}\n",
    );
    let orgs = (
        "services/orgs/orgs.api.yaml",
        "baseUrl: ${var.baseUrl}\napis:\n  - id: member\n    path: /orgs/${var.orgId}/members/${var.memberId}\n    vars:\n      memberId: 'm_42'\n",
    );
    let project = Project::new(&[config, orgs]);
    project.vc(&["new", "test", "orgs.members#getsMember", "--from", "orgs#member"]).assert().code(0);
    let text = project.read("services/orgs/members.test.yaml");
    assert!(
        text.ends_with("    call: orgs#member\n    vars:\n      memberId: 'm_42'  # replace: the api's sample value\n"),
        "{text}"
    );
    assert!(!text.contains("orgId"), "the profile sets it:\n{text}");
    checks_clean(&project);
    let users = Project::new(&[CONFIG, USERS]);
    users.vc(&["new", "test", "users.crud#removesUser", "--from", "users#remove"]).assert().code(0);
    let text = users.read(CRUD.0);
    assert!(
        text.ends_with("    vars:\n      userId: 0  # replace: the api's sample value\n"),
        "from a flow map:\n{text}"
    );
    checks_clean(&users);
}

/// D176: negative cases are built from the body schema, so without one there are none; the
/// message says what to do, with no decision number.
#[test]
fn new_test_negative_needs_a_body_schema() {
    let project = Project::new(&[CONFIG, USERS]);
    let args = ["new", "test", "users.crud#rejectsBadUser", "--from", "users#create", "--negative"];
    let output = project.vc(&args).output().unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", said(&output));
    let text = said(&output);
    let reason = "`--negative` cannot build bad-body cases for `users#create`: it has no request body schema\n";
    assert!(text.contains(reason), "{text}");
    let help = "help: drop `--negative` to add the happy-path test, then add each invalid body as an `examples` row";
    assert!(text.contains(help), "{text}");
    assert!(!text.contains("D176") && !text.contains("D1"), "no decision numbers for users: {text}");
    assert!(!project.exists(CRUD.0), "nothing written");
}

#[test]
fn new_test_refuses_an_unknown_api() {
    let project = Project::new(&[CONFIG, USERS]);
    let output = project.vc(&["new", "test", "users.crud#listsUsers", "--from", "users#lst"]).output().unwrap();
    assert!(!output.status.success());
    assert!(said(&output).contains("users#list"), "suggests the close match: {}", said(&output));
    assert!(!project.exists(CRUD.0));
}

/// Asserts `vc new auth` succeeded and appended `partnerToken` to the config, keeping its bytes.
fn added_partner_token(project: &Project, output: &std::process::Output) {
    assert_eq!(output.status.code(), Some(0), "{}", said(output));
    let config = project.read(CONFIG.0);
    assert!(config.starts_with(CONFIG.1), "the config's bytes are kept:\n{config}");
    assert!(config.contains("id: partnerToken") && config.contains("type: bearer"), "{config}");
}

#[test]
fn new_auth_without_a_terminal_takes_flags_and_never_prompts() {
    let project = Project::new(&[CONFIG]);
    let mut vc = project.vc(&["new", "auth", "partnerToken", "--type", "bearer"]);
    let output = vc.write_stdin("").timeout(Duration::from_secs(20)).output().unwrap();
    added_partner_token(&project, &output);
    assert!(!said(&output).contains('?'), "no prompt when stdin is not a terminal:\n{}", said(&output));
}

#[test]
fn new_auth_without_a_terminal_leaves_a_key_it_cannot_write_to_vc_check() {
    let project = Project::new(&[CONFIG]);
    for (id, kind) in [("partnerFlow", "flow"), ("partnerClient", "oauth2ClientCredentials")] {
        let mut vc = project.vc(&["new", "auth", id, "--type", kind]);
        let output = vc.write_stdin("").timeout(Duration::from_secs(20)).output().unwrap();
        assert_eq!(output.status.code(), Some(0), "{kind}: {}", said(&output));
        let config = project.read(CONFIG.0);
        assert!(config.contains(&format!("id: {id}")) && config.contains(&format!("type: {kind}")), "{config}");
    }
    let check = said(&project.vc(&["check"]).output().unwrap());
    for missing in ["a `flow` auth needs `steps`", "a `oauth2ClientCredentials` auth needs `tokenUrl`"] {
        assert!(check.contains("V1005") && check.contains(missing), "{missing}:\n{check}");
    }
}

#[test]
fn new_auth_in_a_new_project_declares_the_secret_it_writes() {
    let project = Project::empty();
    project.vc(&["init"]).assert().code(0);
    let schema = project.read(".env.schema");
    let mut vc = project.vc(&["new", "auth", "partnerToken", "--type", "bearer"]);
    let output = vc.write_stdin("").timeout(Duration::from_secs(20)).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    assert!(project.read(CONFIG.0).contains("token: ${secret.PARTNER_TOKEN_TOKEN}"));
    let declared = project.read(".env.schema");
    assert!(declared.starts_with(&schema), "the schema's bytes are kept:\n{declared}");
    assert!(declared.lines().any(|line| line == "PARTNER_TOKEN_TOKEN="), "{declared}");
    let note = "set `PARTNER_TOKEN_TOKEN` in `.env` or the environment; runs stop with V5114 until it has a value";
    assert!(String::from_utf8_lossy(&output.stdout).contains(note), "{}", said(&output));
    checks_clean(&project);
}

#[test]
fn new_auth_without_a_schema_says_where_to_declare_the_secret_and_that_it_needs_a_value() {
    let project = Project::new(&[CONFIG]);
    let mut vc = project.vc(&["new", "auth", "partnerToken", "--type", "basic"]);
    let output = vc.write_stdin("").timeout(Duration::from_secs(20)).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    let text = said(&output);
    assert!(text.contains("add PARTNER_TOKEN_PASSWORD= to .env.schema"), "{text}");
    assert!(text.contains("set `PARTNER_TOKEN_PASSWORD` in `.env` or the environment"), "{text}");
    assert!(!project.exists(".env.schema"), "no schema is made: {text}");
    let mut vc = project.vc(&["new", "auth", "partnerClient", "--type", "oauth2ClientCredentials"]);
    let output = vc.write_stdin("").timeout(Duration::from_secs(20)).output().unwrap();
    assert!(!said(&output).contains("secret"), "no secret is left to set:\n{}", said(&output));
}

#[test]
fn new_auth_with_json_never_prompts() {
    let project = Project::new(&[CONFIG]);
    let mut vc = project.vc(&["new", "auth", "partnerToken", "--type", "bearer", "--json"]);
    let output = vc.write_stdin("").timeout(Duration::from_secs(20)).output().unwrap();
    added_partner_token(&project, &output);
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).unwrap_or_else(|e| panic!("{e}: {}", said(&output)));
    assert!(json.is_object(), "{json}");
}

/// `vc <args>` on a pseudo-terminal through `script(1)`, with `answers` typed in.
#[cfg(unix)]
fn on_a_terminal(project: &Project, args: &[&str], answers: &str) -> std::process::Output {
    let vc = cargo_bin!("vc").display().to_string();
    let command = std::iter::once(vc).chain(args.iter().map(|arg| (*arg).to_owned())).collect::<Vec<_>>();
    let mut script = std::process::Command::new("script");
    if cfg!(target_os = "macos") {
        script.args(["-q", "/dev/null"]).args(&command);
    } else {
        script.args(["-qec", &command.join(" "), "/dev/null"]);
    }
    let base = project.process(&[]);
    script.current_dir(project.path()).envs(base.get_envs().filter_map(|(k, v)| v.map(|v| (k, v))));
    script.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = script.spawn().unwrap();
    // macOS `script` sends the end of input to the terminal at once when its input closes, ahead
    // of the answers still unread; so the answers' pipe stays open until `vc` is done.
    let mut input = child.stdin.take().unwrap();
    input.write_all(answers.as_bytes()).unwrap();
    let output = child.wait_with_output().unwrap();
    drop(input);
    output
}

#[cfg(unix)]
#[test]
fn new_auth_on_a_terminal_prompts_for_what_the_flags_leave_out() {
    let project = Project::new(&[CONFIG]);
    let output = on_a_terminal(&project, &["new", "auth", "partnerToken"], "bearer\n\n\n\n");
    added_partner_token(&project, &output);
    assert!(said(&output).contains('?'), "asks on a terminal:\n{}", said(&output));
}

#[test]
fn new_file_writes_an_empty_file_with_its_header() {
    let project = Project::new(&[CONFIG]);
    project.vc(&["new", "file", "test", "users.crud"]).assert().code(0);
    project.vc(&["new", "file", "api", "users"]).assert().code(0);
    assert_eq!(project.read(CRUD.0).lines().next(), Some(schema_line("test").as_str()));
    assert_eq!(project.read(USERS.0).lines().next(), Some(schema_line("api").as_str()));
    checks_clean(&project);
    let again = project.vc(&["new", "file", "test", "users.crud"]).output().unwrap();
    assert!(!again.status.success(), "an existing file is never overwritten");
}

#[test]
fn new_file_load_writes_a_mix_of_the_first_test_and_one_stage_that_checks() {
    let project = Project::new(&[CONFIG, USERS, CRUD]);
    let output = project.vc(&["new", "file", "load", "checkout.peak"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    assert!(said(&output).contains("added checkout.peak to load/checkout/peak.load.yaml:1"), "{}", said(&output));
    let written = project.read("load/checkout/peak.load.yaml");
    assert_eq!(written.lines().next(), Some(schema_line("load").as_str()));
    let body = concat!(
        "mix:\n",
        "  # replace with the tests to run, each with its share of the traffic\n",
        "  - test: users.crud#createsUser\n",
        "    weight: 1\n",
        "stages:\n",
        "  - duration: 30s\n",
        "    users: 1\n",
    );
    assert!(written.ends_with(body), "the first test by ID, one user for 30s:\n{written}");
    checks_clean(&project);
}

#[test]
fn new_file_load_in_a_project_with_no_test_says_to_add_one_first() {
    let project = Project::new(&[CONFIG, USERS]);
    let output = project.vc(&["new", "file", "load", "checkout.peak"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", said(&output));
    let text = said(&output);
    assert!(text.contains("a load file runs tests, and this project has none yet"), "{text}");
    assert!(text.contains("help: add one first with `vc new test <fileId>#<testId> --from <api>`"), "{text}");
    assert!(!project.exists("load"), "nothing is written");
}
