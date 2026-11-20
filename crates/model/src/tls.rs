//! `http.tls` (D350): extra trusted roots and the client identities, as written. The engine
//! renders and reads them once, when the run starts; the compiler only checks their shape.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::{ProfileId, Template, TlsClientId};

/// `http.tls`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct TlsConfig {
    /// `ca`: each a path or inline PEM, trusted for every host on top of the OS store.
    pub ca: Vec<Template>,
    /// `clients`, in written order: the first whose `hosts` match is used.
    pub clients: IndexMap<TlsClientId, TlsClient>,
}

impl TlsConfig {
    /// The `clients` a run under `profiles` loads, in written order: each that names no profile,
    /// or one of `profiles`; `None` follows every profile (D354, D392).
    pub fn active(&self, profiles: Option<&[ProfileId]>) -> impl Iterator<Item = &TlsClient> {
        let follows = move |profile: &ProfileId| profiles.is_none_or(|profiles| profiles.contains(profile));
        self.clients.values().filter(move |client| client.profiles.is_empty() || client.profiles.iter().any(follows))
    }
}

/// One entry of `http.tls.clients`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct TlsClient {
    /// `id`, named by an auth's `clientCert`.
    pub id: TlsClientId,
    /// `name`.
    pub name: Option<String>,
    /// `hosts`: `host`, `host:port` or `*.domain`; none means only through `clientCert`.
    pub hosts: Vec<String>,
    /// `profiles`: only under these; none means every profile.
    pub profiles: Vec<ProfileId>,
    /// `cert` and `key`, or `pkcs12` and `passphrase`.
    pub material: ClientMaterial,
}

/// Where an identity's certificate and key come from. Each value is a template, rendered once
/// per run; a rendered value [`inline_pem`] finds to be PEM is inline, anything else a path.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::exhaustive_enums,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub enum ClientMaterial {
    /// `cert` (the chain, leaf first) and `key`.
    Pem {
        /// `cert`.
        cert: Template,
        /// `key`.
        key: Template,
    },
    /// `pkcs12`, a path, with its `passphrase`; `None` is the empty passphrase.
    Pkcs12 {
        /// `pkcs12`.
        file: Template,
        /// `passphrase`.
        passphrase: Option<Template>,
    },
}

/// An auth's `clientCert`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::exhaustive_enums,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub enum ClientCert {
    /// `clientCert: <id>`: that identity, whatever the host.
    Named(TlsClientId),
    /// `clientCert: none`: no certificate, for negative tests.
    Off,
}

/// Whether a rendered `http.tls` value is inline PEM rather than a path (D374).
///
/// Once trimmed, it starts `-----BEGIN`, says `PRIVATE KEY` or runs over more than one line.
/// The compiler masks such a literal `key`, and the engine never takes one for a path.
#[must_use]
pub fn inline_pem(text: &str) -> bool {
    let text = text.trim();
    text.starts_with("-----BEGIN") || text.contains("PRIVATE KEY") || text.contains('\n')
}

#[cfg(test)]
mod tests {
    use super::{ClientMaterial, TlsClient, TlsConfig, inline_pem};
    use crate::{ProfileId, StringTable, Template, TlsClientId};

    #[test]
    fn a_client_is_active_when_it_names_no_profile_or_one_that_is_followed() {
        let mut strings = StringTable::new();
        let mut sym = |text: &str| strings.intern(text).unwrap();
        let (dev, staging) = (ProfileId::new(sym("dev")), ProfileId::new(sym("staging")));
        let empty = Template { parts: Vec::new() };
        let material = ClientMaterial::Pkcs12 { file: empty, passphrase: None };
        let mut tls = TlsConfig::default();
        for (id, profiles) in [("any", vec![]), ("stage", vec![staging]), ("both", vec![dev, staging])] {
            let id = TlsClientId::new(sym(id));
            let client = TlsClient { id, name: None, hosts: Vec::new(), profiles, material: material.clone() };
            tls.clients.insert(id, client);
        }
        let ids = |profiles: Option<&[ProfileId]>| tls.active(profiles).map(|client| client.id).collect::<Vec<_>>();
        let all: Vec<_> = tls.clients.keys().copied().collect();
        assert_eq!(ids(Some(&[dev])), [all[0], all[2]]);
        assert_eq!(ids(Some(&[staging])), all);
        assert_eq!(ids(Some(&[])), [all[0]]);
        assert_eq!(ids(None), all, "no profiles given follows every one");
    }

    #[test]
    fn a_value_is_inline_pem_when_it_begins_says_private_key_or_runs_over_lines() {
        assert!(inline_pem("-----BEGIN EC PARAMETERS-----"), "a one-line PEM header");
        assert!(inline_pem("\n-----BEGIN PRIVATE KEY-----"), "a header after a newline");
        assert!(inline_pem("MIIB PRIVATE KEY"), "`PRIVATE KEY` without `-----BEGIN`");
        assert!(inline_pem("MIIBVgIBADANBgkqhkiG9w0BAQEFAASC\nAUAwggE8AgEAAkEA"), "base64 over two lines");
        assert!(!inline_pem("MIIBVgIBADANBgkqhkiG9w0BAQEFAASC\n"), "one line of bare base64 is a path");
        assert!(!inline_pem(" certs/partner.key "), "a path");
    }
}
