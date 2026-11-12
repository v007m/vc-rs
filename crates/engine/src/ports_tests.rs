use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use vc_interfaces::CommandOutput;
use vc_testkit::{FakeCommands, FakeTransport, FakeVcs, MemoryEnvironment, MemorySources, status};

use crate::sample::builder;
use crate::{CallRequest, Engine, EngineError, RunRequest, interrupts};

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
/// A loaded secret's value: test data, in no real provider's key format.
const OTHER: &str = "other-fixture-5e21";

/// The project with `environment`, and how many requests it sent.
fn project(environment: &[(&str, &str)]) -> (Engine, Arc<AtomicUsize>) {
    let sent = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&sent);
    let engine = builder(FakeVcs::default())
        .sources(Arc::new(MemorySources::new(&[CONFIG, USERS, CRUD, SCHEMA])))
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
    let (engine, sent) = project(&[("API_TOKEN", "tok-fixture-71"), ("OTHER_KEY", OTHER)]);
    let report = engine.run(&RunRequest::default(), interrupts().1).await.unwrap();
    assert_eq!((report.exit, sent.load(Ordering::SeqCst)), (0, 1));
}

#[tokio::test]
async fn a_run_and_a_call_give_varlock_the_last_profile_as_vc_profile_and_no_run_value() {
    const VARLOCK: (&str, &[u8]) = (
        "vibranium.config.yaml",
        b"syntaxVersion: 2\nsecrets: {provider: varlock}\nprofiles:\n  - id: dev\n    default: true\n  - id: qa\nvars: {baseUrl: 'http://x'}\n",
    );
    let commands = FakeCommands::new(|spec| {
        let only_profile = spec.env == [("VC_PROFILE".to_owned(), "qa".to_owned())];
        let stdout = r#"{"API_TOKEN":"tok-varlock-3a","OTHER_KEY":"o"}"#;
        (spec.program == "varlock" && only_profile).then(|| Ok(CommandOutput::new(Some(0), stdout)))
    });
    let sent = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&sent);
    let engine = builder(FakeVcs::default())
        .sources(Arc::new(MemorySources::new(&[VARLOCK, USERS, CRUD, SCHEMA])))
        .commands(Arc::new(commands))
        .transport(Arc::new(FakeTransport::new(move |request, _| {
            assert_eq!(request.headers.get("x-token").map(|value| value.as_bytes()), Some(&b"tok-varlock-3a"[..]));
            counted.fetch_add(1, Ordering::SeqCst);
            Ok(status(200))
        })))
        .build()
        .unwrap();
    let profiles = vec!["dev".to_owned(), "qa".to_owned()];
    let run = engine.run(&RunRequest { profiles: profiles.clone(), ..RunRequest::default() }, interrupts().1).await;
    assert_eq!(run.unwrap().exit, 0);
    let call = CallRequest { api: "users#get".into(), profiles, ..CallRequest::default() };
    assert_eq!(engine.call(&call).await.unwrap().exit, 0);
    assert_eq!(sent.load(Ordering::SeqCst), 2);
    let unnamed = engine.run(&RunRequest::default(), interrupts().1).await;
    assert!(unnamed.is_err(), "with `dev`, varlock is not given `qa`: {unnamed:?}");
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
