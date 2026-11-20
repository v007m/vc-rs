//! What `vc check` and a run's start find in `http.tls` material without stopping (D408–D412):
//! files that cannot be read (V2011), material that cannot be used (V2012), each leaf
//! certificate's validity (V3107, V3108) and key files others may read (V4012). The checks are
//! the transport factory's, so a check refuses what a run would.

use std::io;
use std::path::Path;

use rustls::RootCertStore;
use vc_interfaces::{MaterialSource, NEAR_NAMES, Secret, TlsFinding, TlsPart, TlsProbe};
use zeroize::Zeroizing;

use crate::identity::pem_chain;
use crate::material::read_material;
use crate::tls::add_ca;
use crate::{ClientIdentity, HttpAdapterError};

/// What `probes` hold, in their order.
pub fn inspect(probes: &[TlsProbe]) -> Vec<TlsFinding> {
    let mut findings = Findings::default();
    for probe in probes {
        match probe {
            TlsProbe::Ca { index, source } => findings.ca(*index, source),
            TlsProbe::Pem { id, cert, key } => findings.pem(id, (cert.as_ref(), key.as_ref())),
            TlsProbe::Pkcs12 { id, bundle, passphrase } => findings.pkcs12(id, (bundle, passphrase.as_ref())),
        }
    }
    findings.0
}

/// The findings so far.
#[derive(Default)]
struct Findings(Vec<TlsFinding>);

impl Findings {
    /// A `ca` entry, read and parsed as the factory adds it to the roots.
    fn ca(&mut self, index: usize, source: &MaterialSource) {
        let part = TlsPart::Ca(index);
        let Some(bytes) = self.read(source, &part) else { return };
        if let Err(HttpAdapterError::Certificate { reason, .. }) =
            add_ca(&mut RootCertStore::empty(), &[bytes.to_vec()])
        {
            self.0.push(TlsFinding::Unusable { part, reason });
        }
    }

    /// A PEM identity: the chain's leaf dated, and the key checked against it when both are known.
    fn pem(&mut self, id: &str, (cert, key): (Option<&MaterialSource>, Option<&MaterialSource>)) {
        let (cert_part, key_part) = (client(id, "cert"), client(id, "key"));
        let cert = cert.and_then(|source| self.read(source, &cert_part));
        let key = key.and_then(|source| self.key(source, &key_part));
        let Some(mut cert) = cert else { return };
        match pem_chain(&cert) {
            Ok(chain) => chain.first().into_iter().for_each(|leaf| self.validity(&cert_part, leaf)),
            Err(reason) => {
                self.0.push(TlsFinding::Unusable { part: cert_part, reason });
                return;
            }
        }
        if let Some(mut key) = key {
            let identity = ClientIdentity::pem(id, std::mem::take(&mut *cert), std::mem::take(&mut *key));
            if let Err(error) = identity.parsed() {
                self.refused(key_part, error);
            }
        }
    }

    /// A PKCS#12 identity: the bundle read, then decoded and its leaf dated when its passphrase
    /// is known.
    fn pkcs12(&mut self, id: &str, (bundle, passphrase): (&MaterialSource, Option<&Secret>)) {
        let part = client(id, "pkcs12");
        let Some(bytes) = self.key(bundle, &part) else { return };
        let Some(passphrase) = passphrase else { return };
        match ClientIdentity::pkcs12(id, &bytes, passphrase.expose()) {
            Ok(identity) => {
                if let Some(leaf) = identity.leaf() {
                    self.validity(&part, leaf);
                }
            }
            Err(error) => self.refused(part, error),
        }
    }

    /// The bytes of `source`, or `None` with why `part`'s file cannot be read.
    fn read(&mut self, source: &MaterialSource, part: &TlsPart) -> Option<Zeroizing<Vec<u8>>> {
        let path = match source {
            MaterialSource::Inline(text) => return Some(Zeroizing::new(text.expose().as_bytes().to_vec())),
            MaterialSource::File { path, .. } => path,
        };
        let error = match read_material(path) {
            Ok(bytes) => return Some(bytes),
            Err(error) => error,
        };
        let near = if error.kind() == io::ErrorKind::NotFound { near(path) } else { Vec::new() };
        self.0.push(TlsFinding::Unreadable { part: part.clone(), reason: error.to_string(), near });
        None
    }

    /// [`Findings::read`] for a key or bundle, with its file's mode checked once it is read.
    fn key(&mut self, source: &MaterialSource, part: &TlsPart) -> Option<Zeroizing<Vec<u8>>> {
        let bytes = self.read(source, part)?;
        if let MaterialSource::File { path, .. } = source
            && let Some(mode) = exposed(path)
        {
            self.0.push(TlsFinding::Exposed { part: part.clone(), mode });
        }
        Some(bytes)
    }

    /// `leaf`'s validity, or why it is not an X.509 certificate.
    fn validity(&mut self, part: &TlsPart, leaf: &[u8]) {
        let finding = match x509_parser::parse_x509_certificate(leaf) {
            Ok((_, cert)) => {
                let validity = cert.validity();
                let (not_before, not_after) = (validity.not_before.timestamp(), validity.not_after.timestamp());
                TlsFinding::Validity { part: part.clone(), not_before, not_after }
            }
            Err(_) => TlsFinding::Unusable { part: part.clone(), reason: "the certificate is not X.509".to_owned() },
        };
        self.0.push(finding);
    }

    /// `error` about `part`: parts that do not fit, or material that cannot be used.
    fn refused(&mut self, part: TlsPart, error: HttpAdapterError) {
        self.0.push(match error {
            HttpAdapterError::Mismatch { reason, .. } => TlsFinding::Mismatch { part, reason },
            other => TlsFinding::Unusable { part, reason: reason(other) },
        });
    }
}

/// An identity's `field`.
fn client(id: &str, field: &'static str) -> TlsPart {
    TlsPart::Client { id: id.to_owned(), field: Some(field) }
}

/// What is wrong with an identity, without its ID, which the part names.
fn reason(error: HttpAdapterError) -> String {
    match error {
        HttpAdapterError::Identity { reason, .. } => reason,
        other => other.to_string(),
    }
}

/// The names in the folder of `path`, a file that does not exist, sorted; at most
/// [`NEAR_NAMES`], and none when the folder cannot be listed.
fn near(path: &Path) -> Vec<String> {
    let Some(entries) = path.parent().and_then(|folder| std::fs::read_dir(folder).ok()) else { return Vec::new() };
    let files = entries.filter_map(Result::ok).filter(|entry| entry.file_type().is_ok_and(|kind| !kind.is_dir()));
    let names = files.map(|entry| entry.file_name().to_string_lossy().into_owned());
    let mut names: Vec<_> = names.take(NEAR_NAMES).collect();
    names.sort_unstable();
    names
}

/// The permission bits of the file at `path` when its group or others have any (Unix only).
#[cfg(unix)]
fn exposed(path: &Path) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt as _;
    let mode = std::fs::metadata(path).ok()?.permissions().mode() & 0o777;
    (mode & 0o077 != 0).then_some(mode)
}

/// Windows has no mode bits to check.
#[cfg(not(unix))]
fn exposed(_path: &Path) -> Option<u32> {
    None
}

#[cfg(test)]
#[path = "inspect_tests.rs"]
mod tests;
