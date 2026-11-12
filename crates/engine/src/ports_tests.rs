use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use vc_testkit::{FakeTransport, FakeVcs, MemoryEnvironment, MemorySources, status};

use crate::sample::builder;
use crate::{CallRequest, Engine, EngineError, PlanRequest, RunRequest, SelectRequest, interrupts};

const CONFIG: (&str, &[u8]) = (
    "vibranium.config.yaml",
    b"syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {baseUrl: 'http://x'}\n",
);
const USERS: (&str, &[u8]) = (
    "services/users/users.api.yaml",
    b"baseUrl: ${var.baseUrl}\napis:\n  - id: get\n    path: /users/1\n    headers: {x-token: '${secret.API_TOKEN}'}\n",
);
const CRUD: (&str, &[u8]) = ("services/users/crud.test.yaml", b"tests:\n  - id: getsUser\n    call: users#get\n");
const SCHEMA: (&str, &[u8]) = (".env.schema", b"API_TOKEN=\nOTHER_KEY=\n");
/// The users api with a second endpoint, `list`, which reads `OTHER_KEY`.
const LISTED: (&str, &[u8]) = (
    "services/users/users.api.yaml",
    b"baseUrl: ${var.baseUrl}
apis:
  - id: get
    path: /users/1
    headers: {x-token: '${secret.API_TOKEN}'}
  - id: list
    path: /users
    headers: {x-key: '${secret.OTHER_KEY}'}
",
);
const BOTH: (&str, &[u8]) = (
    "services/users/crud.test.yaml",
    b"tests:\n  - id: getsUser\n    call: users#get\n  - id: listsUsers\n    call: users#list\n",
);
const OPTIONAL: (&str, &[u8]) = (".env.schema", b"API_TOKEN=\n# The list key, unset in CI.\n# @optional\nOTHER_KEY=\n");
/// A loaded secret's value: test data, in no real provider's key format.
const OTHER: &str = "other-fixture-5e21";
const TOKEN: &str = "tok-fixture-71";

/// The project with `environment`, and how many requests it sent.
fn project(environment: &[(&str, &str)]) -> (Engine, Arc<AtomicUsize>) {
    project_of(&[CONFIG, USERS, CRUD, SCHEMA], environment)
}

/// The project of `files` with `environment`, and how many requests it sent.
fn project_of(files: &[(&str, &[u8])], environment: &[(&str, &str)]) -> (Engine, Arc<AtomicUsize>) {
    let sent = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&sent);
    let engine = builder(FakeVcs::default())
        .sources(Arc::new(MemorySources::new(files)))
        .environment(Arc::new(MemoryEnvironment::new(environment)))
        .transport(Arc::new(FakeTransport::new(move |_, _| {
            counted.fetch_add(1, Ordering::SeqCst);
            Ok(status(200))
        })))
        .build()
        .unwrap();
    (engine, sent)
}

/// The refusal's text and help, when it is a setting's.
fn refusal(error: &EngineError) -> String {
    let EngineError::Setting { origin, reason, help } = error else { panic!("not a setting: {error:?}") };
    format!("{origin}: {reason} / {}", help.as_deref().unwrap_or_default())
}

#[tokio::test]
async fn a_declared_secret_without_a_value_stops_the_run_before_any_request_with_one_v5114() {
    for token in [None, Some(""), Some("  ")] {
        let mut environment = vec![("OTHER_KEY", OTHER)];
        environment.extend(token.map(|token| ("API_TOKEN", token)));
        let (engine, sent) = project(&environment);
        let error = engine.run(&RunRequest::default(), interrupts().1).await.unwrap_err();
        let shown = refusal(&error);
        assert!(shown.starts_with("secrets: V5114: the declared secret `API_TOKEN` has no value"), "{shown}");
        assert!(!shown.contains("OTHER_KEY") && !shown.contains(OTHER), "{shown}");
        let call = engine.call(&CallRequest { api: "users#get".into(), ..CallRequest::default() }).await;
        assert_eq!(call.unwrap_err(), error, "a call is refused the same way");
        assert_eq!(sent.load(Ordering::SeqCst), 0, "nothing was sent");
    }
}

#[tokio::test]
async fn a_run_with_every_declared_secret_set_sends_its_requests() {
    let (engine, sent) = project(&[("API_TOKEN", TOKEN), ("OTHER_KEY", OTHER)]);
    let report = engine.run(&RunRequest::default(), interrupts().1).await.unwrap();
    assert_eq!((report.exit, sent.load(Ordering::SeqCst)), (0, 1));
}

/// A run of the tests `ids` only.
fn selected(ids: &[&str]) -> RunRequest {
    let select = SelectRequest { ids: ids.iter().map(|id| (*id).to_owned()).collect(), ..SelectRequest::default() };
    RunRequest { plan: PlanRequest { select, ..PlanRequest::default() }, ..RunRequest::default() }
}

#[tokio::test]
async fn a_declared_secret_nothing_selected_reads_may_be_unset() {
    let (engine, sent) = project_of(&[CONFIG, LISTED, BOTH, SCHEMA], &[("API_TOKEN", TOKEN)]);
    let report = engine.run(&selected(&["users.crud#getsUser"]), interrupts().1).await.unwrap();
    assert_eq!((report.exit, sent.load(Ordering::SeqCst)), (0, 1), "only listsUsers reads OTHER_KEY");
    let error = engine.run(&RunRequest::default(), interrupts().1).await.unwrap_err();
    let shown = refusal(&error);
    assert!(shown.starts_with("secrets: V5114: the declared secret `OTHER_KEY` has no value"), "{shown}");
    assert!(!shown.contains("API_TOKEN") && !shown.contains(TOKEN), "{shown}");
    let call = |api: &str| CallRequest { api: api.into(), ..CallRequest::default() };
    assert!(engine.call(&call("users#get")).await.is_ok(), "a call is checked against the secrets its api reads");
    assert_eq!(engine.call(&call("users#list")).await.unwrap_err(), error);
    assert_eq!(sent.load(Ordering::SeqCst), 2, "the refused run and call sent nothing");
}

#[tokio::test]
async fn an_optional_secret_may_be_unset_and_only_a_request_reading_it_fails() {
    let (engine, sent) = project_of(&[CONFIG, LISTED, BOTH, OPTIONAL], &[("API_TOKEN", TOKEN)]);
    let report = engine.run(&RunRequest::default(), interrupts().1).await.unwrap();
    assert_eq!((report.exit, sent.load(Ordering::SeqCst)), (1, 1), "listsUsers fails before it sends");
    let (engine, sent) = project_of(&[CONFIG, LISTED, BOTH, OPTIONAL], &[("API_TOKEN", TOKEN), ("OTHER_KEY", OTHER)]);
    let report = engine.run(&RunRequest::default(), interrupts().1).await.unwrap();
    assert_eq!((report.exit, sent.load(Ordering::SeqCst)), (0, 2), "set, an optional secret is sent");
}

#[test]
fn a_test_draws_its_rows_from_the_seed_and_its_id_alone() {
    let draws = |seed, id| {
        let random = super::random_for(seed, id);
        [random.next_u64(), random.next_u64()]
    };
    assert_eq!(draws(7, "users.crud#listsPages"), draws(7, "users.crud#listsPages"));
    assert_ne!(draws(7, "users.crud#listsPages"), draws(7, "users.crud#listsUsers"));
    assert_ne!(draws(7, "users.crud#listsPages"), draws(8, "users.crud#listsPages"));
}

#[test]
fn a_fresh_seed_fits_a_javascript_number_and_a_given_one_is_kept() {
    for _ in 0..64 {
        assert!(super::random(None).0 <= super::MAX_FRESH_SEED, "2^53 - 1 at most");
    }
    assert_eq!(super::random(Some(u64::MAX)).0, u64::MAX, "`--seed` is used as given");
}

#[tokio::test]
async fn a_blank_secret_is_unset_whether_required_or_optional() {
    let (engine, sent) = project(&[("API_TOKEN", "  "), ("OTHER_KEY", OTHER)]);
    let shown = refusal(&engine.run(&RunRequest::default(), interrupts().1).await.unwrap_err());
    assert!(shown.starts_with("secrets: V5114: the declared secret `API_TOKEN` has no value"), "{shown}");
    assert_eq!(sent.load(Ordering::SeqCst), 0, "a blank required secret refuses the run");
    let blank = [("API_TOKEN", TOKEN), ("OTHER_KEY", " ")];
    let (engine, sent) = project_of(&[CONFIG, LISTED, BOTH, OPTIONAL], &blank);
    let report = engine.run(&RunRequest::default(), interrupts().1).await.unwrap();
    assert_eq!((report.exit, sent.load(Ordering::SeqCst)), (1, 1), "listsUsers reads it and fails (V5114)");
    let report = engine.run(&selected(&["users.crud#getsUser"]), interrupts().1).await.unwrap();
    assert_eq!((report.exit, sent.load(Ordering::SeqCst)), (0, 2), "a run that does not read it passes");
}

/// A config whose `beforeRun` sends a request, so a refusal is seen to come before it.
const WARMED: (&str, &[u8]) = (
    "vibranium.config.yaml",
    b"syntaxVersion: 2
profiles:
  - id: dev
    default: true
vars: {baseUrl: 'http://x'}
beforeRun:
  - id: warm
    call: users#get
",
);
/// The users api with `signed`, whose `beforeRequest` hook may read any secret.
const SIGNED: (&str, &[u8]) = (
    "services/users/users.api.yaml",
    b"baseUrl: ${var.baseUrl}
apis:
  - id: get
    path: /users/1
  - id: signed
    path: /signed
    beforeRequest:
      - function: ../../hooks/sign.ts#sign
",
);
const SIGNS: (&str, &[u8]) = (
    "services/users/crud.test.yaml",
    b"tests:\n  - id: getsUser\n    call: users#get\n  - id: signsRequest\n    call: users#signed\n",
);
const HOOK: (&str, &[u8]) = (
    "hooks/sign.ts",
    b"export function sign(request, { secret }) { return { ...request, sig: secret('SIGNING_KEY') }; }\n",
);
const SIGNING: (&str, &[u8]) = (".env.schema", b"SIGNING_KEY=\n");

#[tokio::test]
async fn a_hook_in_the_selection_reads_every_declared_secret() {
    let (engine, sent) = project_of(&[WARMED, SIGNED, SIGNS, HOOK, SIGNING], &[]);
    let error = engine.run(&selected(&["users.crud#signsRequest"]), interrupts().1).await.unwrap_err();
    let shown = refusal(&error);
    assert!(shown.starts_with("secrets: V5114: the declared secret `SIGNING_KEY` has no value"), "{shown}");
    assert_eq!(sent.load(Ordering::SeqCst), 0, "refused before beforeRun sends anything");
    let report = engine.run(&selected(&["users.crud#getsUser"]), interrupts().1).await.unwrap();
    assert_eq!((report.exit, sent.load(Ordering::SeqCst)), (0, 2), "pure YAML reads only what it names");
}

/// Profiles `dev` and `prod`, only `prod`'s vars reading `PROD_TOKEN`.
const PROFILED: (&str, &[u8]) = (
    "vibranium.config.yaml",
    b"syntaxVersion: 2
profiles:
  - id: dev
    default: true
  - id: prod
    vars: {token: '${secret.PROD_TOKEN}'}
vars: {baseUrl: 'http://x', token: none}
",
);
const PROD_SCHEMA: (&str, &[u8]) = (".env.schema", b"API_TOKEN=\nPROD_TOKEN=\n");

#[tokio::test]
async fn only_the_active_profiles_vars_are_read() {
    let (engine, sent) = project_of(&[PROFILED, USERS, CRUD, PROD_SCHEMA], &[("API_TOKEN", TOKEN)]);
    let report = engine.run(&RunRequest::default(), interrupts().1).await.unwrap();
    assert_eq!((report.exit, sent.load(Ordering::SeqCst)), (0, 1), "dev does not read PROD_TOKEN");
    let mut prod = RunRequest::default();
    prod.plan.profiles = vec!["prod".to_owned()];
    let shown = refusal(&engine.run(&prod, interrupts().1).await.unwrap_err());
    assert!(shown.starts_with("secrets: V5114: the declared secret `PROD_TOKEN` has no value"), "{shown}");
}
