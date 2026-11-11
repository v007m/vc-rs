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
const OPTIONAL: (&str, &[u8]) = (".env.schema", b"API_TOKEN=\n# @optional  the list key, unset in CI\nOTHER_KEY=\n");
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
    assert_eq!(super::fnv1a(""), 0xcbf2_9ce4_8422_2325, "the FNV-1a offset basis");
    assert_eq!(super::fnv1a("a"), 0xaf63_dc4c_8601_ec8c, "FNV-1a of `a`");
}

#[test]
fn a_fresh_seed_fits_a_javascript_number_and_a_given_one_is_kept() {
    for _ in 0..64 {
        assert!(super::random(None).0 <= super::MAX_FRESH_SEED, "2^53 - 1 at most");
    }
    assert_eq!(super::random(Some(u64::MAX)).0, u64::MAX, "`--seed` is used as given");
}
