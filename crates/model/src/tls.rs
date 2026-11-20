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
    use super::inline_pem;

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
