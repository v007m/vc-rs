use std::sync::Arc;

use vc_testkit::{FakeCommands, FakeVcs, MemoryEnvironment, MemorySources};

use super::{SecretCheck, SecretsChecked};
use crate::sample::builder;
use crate::{Engine, EngineError, ExplainRequest};

const SECRET: &str = "tok-check-5be1d0";
const SCHEMA: (&str, &[u8]) = (".env.schema", b"# Shop secrets\nSHOP_TOKEN=\nSHOP_CLIENT_ID=\nSHOP_BLANK=\n");
const CONFIG: (&str, &[u8]) = (
    "vibranium.config.yaml",
    b"syntaxVersion: 2\nvars:\n  token: '${secret.SHOP_TOKEN}'\nprofiles:\n  - id: dev\n    default: true\n  - id: staging\n",
);
const VARLOCK: (&str, &[u8]) = (
    "vibranium.config.yaml",
    b"syntaxVersion: 2\nsecrets: {provider: varlock}\nprofiles:\n  - id: dev\n    default: true\n",
);

fn project(config: (&str, &'static [u8])) -> Engine {
    let environment = [("SHOP_TOKEN", SECRET), ("SHOP_BLANK", "  ")];
    builder(FakeVcs::default())
        .sources(Arc::new(MemorySources::new(&[config, SCHEMA])))
        .environment(Arc::new(MemoryEnvironment::new(&environment)))
        .commands(Arc::new(FakeCommands::new(|_| None)))
        .build()
        .unwrap()
}

fn profile(name: &str) -> ExplainRequest {
    ExplainRequest { profiles: vec![name.to_owned()], ..ExplainRequest::default() }
}

fn states(checked: &SecretsChecked) -> Vec<(&str, bool)> {
    checked.secrets.iter().map(|SecretCheck { name, resolved, .. }| (name.as_str(), *resolved)).collect()
}

#[tokio::test]
async fn gives_each_declared_secret_in_schema_order_and_whether_it_resolves_never_its_value() {
    let checked = project(CONFIG).check_secrets(&profile("staging")).await.unwrap();
    assert_eq!(checked.provider, "env");
    assert_eq!(states(&checked), [("SHOP_TOKEN", true), ("SHOP_CLIENT_ID", false), ("SHOP_BLANK", false)]);
    assert_eq!(checked.error, None);
    assert!(!format!("{checked:?}").contains(SECRET), "{checked:?}");
}

#[tokio::test]
async fn a_provider_that_cannot_load_resolves_none_and_says_why_without_a_value() {
    let checked = project(VARLOCK).check_secrets(&ExplainRequest::default()).await.unwrap();
    assert_eq!(checked.provider, "varlock");
    assert_eq!(states(&checked), [("SHOP_TOKEN", false), ("SHOP_CLIENT_ID", false), ("SHOP_BLANK", false)]);
    let error = checked.error.as_deref().unwrap();
    assert!(error.contains("varlock"), "{error}");
    assert!(!format!("{checked:?}").contains(SECRET), "{checked:?}");
}

#[tokio::test]
async fn an_unknown_profile_is_refused() {
    let refused = project(CONFIG).check_secrets(&profile("prod")).await.unwrap_err();
    assert!(matches!(refused, EngineError::Setting { .. }), "{refused:?}");
}
