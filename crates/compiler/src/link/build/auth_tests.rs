use vc_diagnostics::Code;
use vc_model::{
    ActionKind, ContentHash, Method, Owner, Program, RedactRule, SourceFile, SourcePath, Template, TemplateValue,
};

use crate::{CompileInput, Compiled};

const CONFIG: &str = "syntaxVersion: 2\nvars: {idUrl: https://id.test, ttl: 60}\nprofiles:\n  - id: dev\n    default: true\nsecrets: {provider: varlock}\nredact:\n  rules:\n    - regex: '\\d{3}-\\d{4}'\n    - cel: item.name == 'ssn'\nauth:\n  - id: svc\n    type: oauth2ClientCredentials\n    tokenUrl: ${var.idUrl}/token\n    clientId: shop\n    clientSecret: ${secret.CLIENT_SECRET}\n    scopes: [read, write]\n    refreshBefore: 30s\n    refreshOn: [401, 419]\n  - id: login\n    type: flow\n    name: Partner login\n    vars: {user: ada}\n    expiresIn: ${var.ttl}s\n    cache: true\n    steps:\n      - id: token\n        method: POST\n        url: ${var.idUrl}/login\n        query: {as: '${var.user}'}\n        headers: {content-type: application/json}\n        body: {user: '${var.user}'}\n        save: {accessToken: '${response.body.token}'}\n    apply:\n      headers: {authorization: 'Bearer ${var.accessToken}'}\n  - id: key\n    type: apiKey\n    in: header\n    name: x-api-key\n    value: k\n    pkce: true\n    redirectPort: 8123\n";

const SCHEMA: &str = "# @sensitive\nCLIENT_SECRET=\nAPI_TOKEN=\n";

fn file(path: &str, text: &str) -> SourceFile {
    SourceFile { path: SourcePath::new(path).unwrap(), text: text.to_owned(), hash: ContentHash::new([0; 32]) }
}

fn compiled(api: &str, schema: Option<&str>) -> Compiled {
    let files = vec![file("vibranium.config.yaml", CONFIG), file("services/users/users.api.yaml", api)];
    let secret_schema = schema.map(|text| file(".env.schema", text));
    crate::compile(CompileInput { files, secret_schema, ..CompileInput::default() })
}

fn program() -> std::sync::Arc<Program> {
    let compiled = compiled("apis:\n  - id: list\n    path: /users\n", Some(SCHEMA));
    assert!(!compiled.diagnostics.has_errors(), "{:?}", compiled.diagnostics);
    compiled.program.unwrap()
}

fn def<'p>(program: &'p Program, id: &str) -> &'p vc_model::AuthDef {
    program.auth.values().find(|def| program.str(def.id.sym()) == Some(id)).unwrap()
}

fn literal(text: &str) -> TemplateValue {
    TemplateValue::String(Template { parts: vec![vc_model::Part::Literal(text.to_owned())] })
}

#[test]
fn oauth_settings_keep_their_types_and_the_refresh_rules() {
    let program = program();
    let svc = def(&program, "svc");
    assert_eq!(svc.kind, "oauth2ClientCredentials");
    assert_eq!(svc.refresh_on, [401, 419]);
    assert_eq!(svc.refresh_before.get(), std::time::Duration::from_secs(30));
    assert_eq!(svc.settings["clientId"], literal("shop"));
    assert_eq!(svc.settings["scopes"], TemplateValue::List(vec![literal("read"), literal("write")]));
    assert!(matches!(&svc.settings["tokenUrl"], TemplateValue::String(template) if template.parts.len() == 2));
}

#[test]
fn oauth_settings_drop_the_keys_the_def_holds_itself() {
    let program = program();
    let svc = def(&program, "svc");
    assert!(!svc.settings.contains_key("refreshOn") && !svc.settings.contains_key("type"));
    assert!(svc.steps.is_empty() && svc.expires_in.is_none() && !svc.cache);
}

#[test]
fn an_api_key_keeps_flags_numbers_and_its_name() {
    let program = program();
    let key = def(&program, "key");
    assert_eq!(key.settings["pkce"], TemplateValue::Bool(true));
    assert_eq!(key.settings["redirectPort"], TemplateValue::Number(8123.into()));
    assert_eq!(key.settings["name"], literal("x-api-key"));
    assert_eq!(key.refresh_on, [401]);
    assert_eq!(key.refresh_before.get(), std::time::Duration::from_secs(60));
}

#[test]
fn a_flow_builds_its_steps_as_inline_run_actions() {
    let program = program();
    let login = def(&program, "login");
    assert_eq!((login.name.as_deref(), login.cache), (Some("Partner login"), true));
    assert_eq!(login.vars.values.len(), 1);
    assert!(login.expires_in.as_ref().is_some_and(|template| template.parts.len() == 2));
    assert!(login.settings.contains_key("apply"));
    let [step] = login.steps[..] else { panic!("one step: {:?}", login.steps) };
    let action = &program.actions[usize::try_from(step.index()).unwrap()];
    let ActionKind::Inline(request) = &action.kind else { panic!("inline: {:?}", action.kind) };
    assert_eq!((request.method, action.owner), (Method::Post, Owner::Run));
    assert!(request.headers.contains_key("content-type"));
    assert!(matches!(request.body, vc_model::BodySpec::Json(_)));
    assert_eq!(action.save.values.len(), 1);
}

#[test]
fn a_flow_step_keeps_its_query() {
    let program = program();
    let step = def(&program, "login").steps[0];
    let action = &program.actions[usize::try_from(step.index()).unwrap()];
    let ActionKind::Inline(request) = &action.kind else { panic!("inline: {:?}", action.kind) };
    assert_eq!(request.query.keys().collect::<Vec<_>>(), ["as"]);
}

#[test]
fn secrets_and_redact_rules_reach_the_config() {
    let program = program();
    let config = &program.config;
    assert_eq!(config.secrets.provider, "varlock");
    assert_eq!(config.secrets.schema, ".env.schema");
    assert_eq!(config.secrets.declared, ["CLIENT_SECRET", "API_TOKEN"]);
    assert!(config.secrets.optional.is_empty(), "`@sensitive` does not make a secret optional");
    assert!(
        matches!(&config.redact_rules[..], [RedactRule::Regex(regex), RedactRule::Cel(_)] if regex == "\\d{3}-\\d{4}")
    );
}

#[test]
fn an_undeclared_secret_is_v2106_with_a_fix_to_the_close_name() {
    let compiled =
        compiled("apis:\n  - id: list\n    path: /users\n    headers: {x-a: '${secret.API_TOKN}'}\n", Some(SCHEMA));
    let found: Vec<_> = compiled.diagnostics.iter().filter(|found| found.code == Code::V2106).collect();
    let [problem] = found[..] else { panic!("{:?}", compiled.diagnostics) };
    assert!(problem.message.contains("API_TOKN"));
    let fix = &problem.fixes[0];
    assert!(fix.certain);
    assert_eq!(fix.edits[0].new_text, "API_TOKEN");
}

#[test]
fn the_profile_variable_in_the_schema_is_no_secret_so_reading_it_is_v2106() {
    let schema = "# @currentEnv=$VC_PROFILE\nVC_PROFILE=dev\nCLIENT_SECRET=\n";
    let compiled =
        compiled("apis:\n  - id: list\n    path: /users\n    headers: {x-a: '${secret.VC_PROFILE}'}\n", Some(schema));
    let found: Vec<_> = compiled.diagnostics.iter().filter(|found| found.code == Code::V2106).collect();
    let [problem] = found[..] else { panic!("{:?}", compiled.diagnostics) };
    assert!(problem.message.contains("VC_PROFILE"), "{}", problem.message);
}

#[test]
fn an_env_read_of_a_secret_is_v2107_with_a_certain_rewrite() {
    let api = "apis:\n  - id: list\n    path: /users\n    headers: {x-a: '${env.API_TOKEN}', x-b: '${env.HOME}'}\n";
    let compiled = compiled(api, Some(SCHEMA));
    let found: Vec<_> = compiled.diagnostics.iter().filter(|found| found.code == Code::V2107).collect();
    let [problem] = found[..] else { panic!("{:?}", compiled.diagnostics) };
    let edit = &problem.fixes[0].edits[0];
    assert_eq!(edit.new_text, "secret");
    let text = api;
    assert_eq!(&text[edit.range.clone()], "env");
}

#[test]
fn without_a_schema_secret_names_are_not_checked() {
    let api = "apis:\n  - id: list\n    path: /users\n    headers: {x-a: '${env.API_TOKEN}', x-b: '${secret.NOPE}'}\n";
    let compiled = compiled(api, None);
    assert!(!compiled.diagnostics.iter().any(|found| matches!(found.code, Code::V2106 | Code::V2107)));
    assert_eq!(compiled.program.unwrap().config.secrets.declared.len(), 0);
}

#[test]
fn an_optional_secret_is_declared_and_kept_apart_in_the_config() {
    let schema = "CLIENT_SECRET=\n# @optional\nAPI_TOKEN=\n";
    let api = "apis:\n  - id: list\n    path: /users\n    headers: {x-a: '${secret.API_TOKEN}'}\n";
    let compiled = compiled(api, Some(schema));
    assert!(!compiled.diagnostics.has_errors(), "an optional secret is still declared: {:?}", compiled.diagnostics);
    let program = compiled.program.unwrap();
    assert_eq!(program.config.secrets.declared, ["CLIENT_SECRET", "API_TOKEN"]);
    assert_eq!(program.config.secrets.optional, ["API_TOKEN"]);
}
