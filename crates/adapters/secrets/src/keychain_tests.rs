use std::sync::Arc;

use keyring_core::mock::{Cred, Store};
use keyring_core::{CredentialStore, Error};
use vc_interfaces::{Keychain, Secret};
use vc_testkit::keychain_suite;

use super::OsKeychain;

fn store() -> Arc<CredentialStore> {
    Store::new().unwrap()
}

/// Makes the next use of `service`/`account` in `store` fail with a platform error.
fn fail_next(store: &Arc<CredentialStore>, service: &str, account: &str) {
    let entry = store.build(service, account, None).unwrap();
    let cred: &Cred = entry.as_any().downcast_ref().unwrap();
    cred.set_error(Error::PlatformFailure("locked".into()));
}

#[test]
fn a_keychain_over_a_store_meets_the_keychain_contract() {
    keychain_suite(&OsKeychain::new(store()));
}

#[test]
fn a_store_failure_is_an_error_on_get_set_and_delete() {
    let store = store();
    let keychain = OsKeychain::new(Arc::clone(&store));
    fail_next(&store, "svc", "acct");
    assert!(keychain.get("svc", "acct").is_err_and(|error| error.reason.contains("locked")));
    fail_next(&store, "svc", "acct");
    assert!(keychain.set("svc", "acct", &Secret::new("r1")).is_err());
    fail_next(&store, "svc", "acct");
    assert!(keychain.delete("svc", "acct").is_err());
    assert_eq!(keychain.get("svc", "acct").ok(), Some(None), "a failed set stores nothing");
}

#[test]
fn debug_names_the_store_but_never_a_secret() {
    let keychain = OsKeychain::new(store());
    assert!(keychain.set("svc", "acct", &Secret::new("refresh-s3cret")).is_ok());
    let text = format!("{keychain:?}");
    assert!(text.starts_with("OsKeychain") && !text.contains("refresh-s3cret"), "{text}");
}

#[test]
fn the_platform_keychain_opens_on_the_systems_with_a_native_store() {
    let native = cfg!(any(target_os = "macos", target_os = "windows", target_os = "linux"));
    assert_eq!(OsKeychain::platform().is_ok(), native);
}
