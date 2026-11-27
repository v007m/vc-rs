//! The TLS material a run's `http.tls` names, read and checked once before the first request
//! (D350, D390, D399): the extra trusted roots and the client identities, and the factory that
//! builds the transport presenting them.

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use crate::{Environment, Secret, StreamTransport, Transport};

/// Builds the transport a run sends through, trusting the roots and presenting the identities
/// of its rendered `http.tls` (D399).
pub trait TransportFactory: Send + Sync {
    /// The ports for `tls`, through the proxy `environment` names; `tls`'s material is read,
    /// parsed and checked once, here. It blocks on files and on a PKCS#12 bundle's key
    /// derivation, so call it off the async runtime (`spawn_blocking`), from within a Tokio
    /// runtime.
    ///
    /// # Errors
    ///
    /// [`TlsError`] naming the part that cannot be used; no reason quotes a key or a passphrase.
    fn build(&self, tls: &TlsSpec, environment: &dyn Environment) -> Result<TransportPorts, TlsError>;

    /// What `probes`' material holds that a check or a run's start reports without stopping
    /// (D408–D412): a file that cannot be read, material that cannot be used or whose parts do
    /// not fit, each leaf certificate's validity, and a key or bundle file that others may read.
    /// It blocks on files as [`TransportFactory::build`] does. A factory that reads no files
    /// finds nothing, the default.
    fn inspect(&self, probes: &[TlsProbe]) -> Vec<TlsFinding> {
        let _ = probes;
        Vec::new()
    }

    /// How many connections the transports it built hold open now, for a load run's Hosts
    /// panel (D675); `None`, the default, when it does not count them.
    fn open_connections(&self) -> Option<u64> {
        None
    }
}

/// A rendered `http.tls`: the `ca` entries, and the client identities the run's profiles allow.
/// What a [`TransportFactory`] builds for a run (D428).
///
/// The transport, and the stream port presenting the same roots and identities when the
/// factory has one.
#[derive(Clone)]
#[expect(clippy::exhaustive_structs, reason = "built by the adapter, read by the engine; both are in this repo (D160)")]
pub struct TransportPorts {
    /// Sends each request and reads its answer.
    pub transport: Arc<dyn Transport>,
    /// Opens `sse` and `websocket` streams; `None` when the factory opens none.
    pub streams: Option<Arc<dyn StreamTransport>>,
}

impl From<Arc<dyn Transport>> for TransportPorts {
    /// `transport` alone, with no stream port.
    fn from(transport: Arc<dyn Transport>) -> Self {
        Self { transport, streams: None }
    }
}

impl fmt::Debug for TransportPorts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TransportPorts").field("streams", &self.streams.is_some()).finish_non_exhaustive()
    }
}

#[derive(Debug, Default)]
#[expect(clippy::exhaustive_structs, reason = "built by the engine, read by the adapter; both are in this repo (D160)")]
pub struct TlsSpec {
    /// The `ca` entries, in order.
    pub ca: Vec<MaterialSource>,
    /// The identities; entry `i` becomes `CertId::new(i)`.
    pub clients: Vec<ClientSpec>,
}

impl TlsSpec {
    /// Every part of this spec as a probe, for [`TransportFactory::inspect`] once it is built.
    #[must_use]
    pub fn into_probes(self) -> Vec<TlsProbe> {
        let ca = self.ca.into_iter().enumerate().map(|(index, source)| TlsProbe::Ca { index, source });
        let clients = self.clients.into_iter().map(|ClientSpec { id, material, .. }| match material {
            ClientSource::Pem { cert, key } => TlsProbe::Pem { id, cert: Some(cert), key: Some(key) },
            ClientSource::Pkcs12 { bundle, passphrase } => {
                TlsProbe::Pkcs12 { id, bundle, passphrase: Some(passphrase) }
            }
        });
        ca.chain(clients).collect()
    }
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

/// One `ca` entry or client identity for [`TransportFactory::inspect`], with only the values known
/// before a run renders it: `vc check` leaves out a value written with `${…}` (D408).
#[derive(Debug)]
#[expect(clippy::exhaustive_enums, reason = "a `ca` entry or one of the two forms of a client (D350)")]
pub enum TlsProbe {
    /// The `ca` entry at `index`.
    Ca { index: usize, source: MaterialSource },
    /// A PEM identity's `cert` and `key`, each `None` when it is known only once rendered.
    Pem { id: String, cert: Option<MaterialSource>, key: Option<MaterialSource> },
    /// A PKCS#12 identity's bundle and passphrase; with no passphrase known, the bundle is only
    /// read.
    Pkcs12 { id: String, bundle: MaterialSource, passphrase: Option<Secret> },
}

/// What [`TransportFactory::inspect`] found in one part of `http.tls`. No reason quotes a key, a
/// passphrase or a rendered path.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_enums,
    reason = "matched by the engine, built by the adapter; both are in this repo (D160)"
)]
pub enum TlsFinding {
    /// `part`'s file cannot be read (V2011). `near` holds the names in its folder when it does
    /// not exist, at most [`NEAR_NAMES`] of them.
    Unreadable { part: TlsPart, reason: String, near: Vec<String> },
    /// `part`'s material cannot be used (V2012).
    Unusable { part: TlsPart, reason: String },
    /// `part`'s parts do not fit: a key that is not the certificate's, a wrong passphrase (V2012).
    Mismatch { part: TlsPart, reason: String },
    /// `part`'s leaf certificate is valid from `not_before` until `not_after`, in seconds since
    /// the Unix epoch (V3107, V3108).
    Validity { part: TlsPart, not_before: i64, not_after: i64 },
    /// `part`'s file has `mode`, which lets its group or others read or write it (V4012).
    Exposed { part: TlsPart, mode: u32 },
}

/// The most names of a missing file's folder a [`TlsFinding::Unreadable`] holds (D409).
pub const NEAR_NAMES: usize = 1024;

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
