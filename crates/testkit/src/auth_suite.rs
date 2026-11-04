use vc_interfaces::{ActionRunner, AuthProvider, AuthRequest, Browser, Keychain, Secret};

/// The settings whose values are secret: no error message may hold them.
const SECRET_KEYS: [&str; 6] = ["clientSecret", "password", "token", "value", "refreshToken", "code"];

/// The contract every [`AuthProvider`] meets (D112). `served` is a request it can serve;
/// `refused` is one it must refuse.
///
/// # Panics
///
/// When the provider breaks the contract.
pub async fn auth_provider_suite(provider: &dyn AuthProvider, served: &AuthRequest, refused: &AuthRequest) {
    let got = provider.credentials(served).await;
    assert!(got.is_ok(), "serving {served:?} failed: {got:?}");
    let credentials = got.unwrap_or_default();
    assert!(credentials.values().count() > 0, "credentials hold at least one value");
    for value in credentials.values() {
        assert!(!format!("{credentials:?}").contains(value), "Debug never shows a credential");
    }
    let error = provider.credentials(refused).await.err();
    assert!(error.is_some(), "refuses {refused:?}");
    let error = error.map(|error| (error.code().to_string(), error.to_string()));
    let (code, message) = error.unwrap_or_default();
    assert_eq!(code, "V5114", "every auth failure is V5114");
    assert_holds_no_secret(refused, &message);
}

/// Checks `message` holds none of the secret settings of `request`.
fn assert_holds_no_secret(request: &AuthRequest, message: &str) {
    for key in SECRET_KEYS {
        let value = request.text(key).filter(|value| value.len() > 3);
        assert!(value.is_none_or(|value| !message.contains(value)), "the message never holds `{key}`: {message}");
    }
}

/// The contract every [`ActionRunner`] meets (D112): running it again gives the same outcome,
/// and a failure is V5114.
///
/// # Panics
///
/// When the runner breaks the contract.
pub async fn action_runner_suite(runner: &dyn ActionRunner) {
    let first = runner.run().await;
    let second = runner.run().await;
    assert_eq!(first.is_ok(), second.is_ok(), "running again gives the same outcome");
    if let Err(error) = first {
        assert_eq!(error.code().to_string(), "V5114");
    }
}

/// The contract every [`Browser`] meets (D112): it opens an `http` URL and refuses anything
/// else, so a hostile `authorizeUrl` cannot start another program.
///
/// # Panics
///
/// When the browser breaks the contract.
pub async fn browser_suite(browser: &dyn Browser) {
    let opened = browser.open("http://127.0.0.1:9/authorize?state=s").await;
    assert!(opened.is_ok(), "opens an http URL: {opened:?}");
    for refused in ["file:///etc/passwd", "javascript:alert(1)", "--help", ""] {
        assert!(browser.open(refused).await.is_err(), "refuses `{refused}`");
    }
}

/// The contract every [`Keychain`] meets (D112), for a keychain no other test uses.
///
/// # Panics
///
/// When the keychain breaks the contract.
pub fn keychain_suite(keychain: &dyn Keychain) {
    let (service, account) = ("vc-suite", "refresh");
    assert_eq!(keychain.get(service, account).ok(), Some(None), "nothing is stored at first");
    assert!(keychain.set(service, account, &Secret::new("r1")).is_ok());
    assert!(keychain.set(service, account, &Secret::new("r2")).is_ok());
    let stored = keychain.get(service, account).ok().flatten();
    assert_eq!(stored.as_ref().map(Secret::expose), Some("r2"), "a set replaces the earlier one");
    assert_eq!(keychain.get(service, "other").ok(), Some(None), "accounts are separate");
    assert!(keychain.delete(service, account).is_ok());
    assert_eq!(keychain.get(service, account).ok(), Some(None));
    assert!(keychain.delete(service, account).is_ok(), "deleting nothing is fine");
}
