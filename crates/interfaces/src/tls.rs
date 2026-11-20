//! The TLS material a run's `http.tls` names, read and checked once before the first request
//! (D350, D390, D399): the extra trusted roots and the client identities, and the factory that
//! builds the transport presenting them.

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use crate::{Environment, Secret, Transport};

/// Builds the transport a run sends through, trusting the roots and presenting the identities
/// of its rendered `http.tls` (D399).
pub trait TransportFactory: Send + Sync {
    /// The transport for `tls`, through the proxy `environment` names; `tls`'s material is read,
    /// parsed and checked once, here. It blocks on files and on a PKCS#12 bundle's key
    /// derivation, so call it off the async runtime (`spawn_blocking`), from within a Tokio
    /// runtime.
    ///
    /// # Errors
    ///
    /// [`TlsError`] naming the part that cannot be used; no reason quotes a key or a passphrase.
    fn build(&self, tls: &TlsSpec, environment: &dyn Environment) -> Result<Arc<dyn Transport>, TlsError>;
}

/// A rendered `http.tls`: the `ca` entries, and the client identities the run's profiles allow.
#[derive(Debug, Default)]
#[expect(clippy::exhaustive_structs, reason = "built by the engine, read by the adapter; both are in this repo (D160)")]
pub struct TlsSpec {
    /// The `ca` entries, in order.
    pub ca: Vec<MaterialSource>,
    /// The identities; entry `i` becomes `CertId::new(i)`.
    pub clients: Vec<ClientSpec>,
}

/// One client identity: its ID, the hosts it is sent to and its material.
#[derive(Debug)]
#[expect(clippy::exhaustive_structs, reason = "built by the engine, read by the adapter; both are in this repo (D160)")]
pub struct ClientSpec {
    /// Its `id`, as errors name it.
    pub id: String,
    /// Its `hosts` patterns.
    pub hosts: Vec<String>,
    /// Its certificate and key.
    pub material: ClientSource,
}

/// A client identity's material: PEM, or a PKCS#12 bundle and its passphrase.
#[derive(Debug)]
#[expect(clippy::exhaustive_enums, reason = "the two forms `http.tls.clients` takes (D350)")]
pub enum ClientSource {
    /// A PEM certificate chain, leaf first, and its private key.
    Pem { cert: MaterialSource, key: MaterialSource },
    /// A PKCS#12 bundle and its passphrase, which `Debug` never shows; an empty one is none.
    Pkcs12 { bundle: MaterialSource, passphrase: Secret },
}

/// Where one piece of material comes from.
#[expect(clippy::exhaustive_enums, reason = "inline or a file, the two forms a value takes (D374)")]
pub enum MaterialSource {
    /// PEM text the config gives inline, or renders from a secret, env var or var; in memory
    /// wiped on drop, never shown by `Debug`.
    Inline(Secret),
    /// A file. `shown` is the path as the config wrote it, when it was written literally: the
    /// only form an error may quote, since a rendered one may hold anything.
    File { path: PathBuf, shown: Option<String> },
}

impl fmt::Debug for MaterialSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Inline(_) => f.write_str("Inline"),
            Self::File { shown, .. } => f.debug_struct("File").field("shown", shown).finish_non_exhaustive(),
        }
    }
}

/// The part of `http.tls` an error is about.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_enums,
    reason = "matched by the engine, built by the adapter; both are in this repo (D160)"
)]
pub enum TlsPart {
    /// The `ca` entry at this index.
    Ca(usize),
    /// The client identity with `id`, and the key of it (`cert`, `key`, `pkcs12`) when one is meant.
    Client { id: String, field: Option<&'static str> },
}

impl fmt::Display for TlsPart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ca(index) => write!(f, "`ca` certificate {}", index + 1),
            Self::Client { id, field: None } => write!(f, "client certificate `{id}`"),
            Self::Client { id, field: Some(field) } => write!(f, "client certificate `{id}`, its `{field}`"),
        }
    }
}

/// Why no transport can be built for a [`TlsSpec`]. No reason quotes a key, a passphrase or a
/// rendered path.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[expect(clippy::exhaustive_enums, reason = "one variant per way a run stops (D391, D399)")]
pub enum TlsError {
    /// Material that cannot be read or parsed (V5125).
    #[error("{part}: {reason}")]
    Unusable { part: TlsPart, reason: String },
    /// Parts that do not fit: a key that is not the certificate's, a wrong passphrase (V5126).
    #[error("{part}: {reason}")]
    Mismatch { part: TlsPart, reason: String },
    /// A transport that cannot be built for a reason no part of `http.tls` causes.
    #[error("{reason}")]
    Transport { reason: String },
}
