//! The HTTP transport's [`TlsLoader`]: reads `http.tls`'s files and builds its client identities
//! into the [`TlsOptions`] the transport is made from.

use rustls::RootCertStore;
use vc_interfaces::{ClientSource, ClientSpec, LoadedTls, MaterialSource, TlsError, TlsLoader, TlsPart, TlsSpec};
use zeroize::Zeroizing;

use crate::material::read_material;
use crate::tls::add_ca;
use crate::{ClientIdentity, HttpAdapterError, TlsOptions};

/// Loads `http.tls` for [`crate::HttpTransport`]: what it loads is a [`TlsOptions`].
#[derive(Clone, Copy, Debug, Default)]
#[expect(
    clippy::exhaustive_structs,
    reason = "unit adapter built by the engine builder, shared across the workspace; every user is in this repo (D160)"
)]
pub struct HttpTlsLoader;

impl TlsLoader for HttpTlsLoader {
    fn load(&self, spec: TlsSpec) -> Result<LoadedTls, TlsError> {
        let ca = spec.ca.into_iter().enumerate();
        let ca =
            ca.map(|(index, source)| read(source, &TlsPart::Ca(index)).map(|mut bytes| std::mem::take(&mut *bytes)));
        let ca: Vec<_> = ca.collect::<Result<_, _>>()?;
        add_ca(&mut RootCertStore::empty(), &ca).map_err(|error| refused(error, TlsPart::Ca(0)))?;
        let clients = spec.clients.into_iter().map(identity).collect::<Result<_, _>>()?;
        Ok(LoadedTls::new(TlsOptions { ca, clients }))
    }
}

/// `identity`, its PEM certificate and key parsed and checked to belong together.
fn checked(identity: ClientIdentity) -> Result<ClientIdentity, TlsError> {
    let part = || TlsPart::Client { id: identity.id().to_owned(), field: None };
    identity.certified().map_err(|error| refused(error, part()))?;
    Ok(identity)
}

/// `client`'s identity, its files read and a bundle decrypted.
fn identity(client: ClientSpec) -> Result<ClientIdentity, TlsError> {
    let part = |field| TlsPart::Client { id: client.id.clone(), field: Some(field) };
    let identity = match client.material {
        ClientSource::Pem { cert, key } => {
            let (mut cert, mut key) = (read(cert, &part("cert"))?, read(key, &part("key"))?);
            checked(ClientIdentity::pem(client.id.clone(), std::mem::take(&mut *cert), std::mem::take(&mut *key)))?
        }
        ClientSource::Pkcs12 { bundle, passphrase } => {
            let bundle = read(bundle, &part("pkcs12"))?;
            let identity = ClientIdentity::pkcs12(client.id.clone(), &bundle, passphrase.expose());
            identity.map_err(|error| refused(error, part("pkcs12")))?
        }
    };
    Ok(identity.hosts(client.hosts))
}

/// The bytes of `source`, or why `part`'s file cannot be read, naming it only as `shown`.
fn read(source: MaterialSource, part: &TlsPart) -> Result<Zeroizing<Vec<u8>>, TlsError> {
    match source {
        MaterialSource::Inline(text) => Ok(Zeroizing::new(text.expose().as_bytes().to_vec())),
        MaterialSource::File { path, shown } => read_material(&path).map_err(|error| {
            let file = shown.map_or_else(|| "its file".to_owned(), |shown| format!("`{shown}`"));
            TlsError::Unusable { part: part.clone(), reason: format!("cannot read {file}: {error}") }
        }),
    }
}

/// `error` about `part`, or the `ca` entry it names: V5126 for parts that do not fit, else V5125.
fn refused(error: HttpAdapterError, part: TlsPart) -> TlsError {
    match error {
        HttpAdapterError::Mismatch { reason, .. } => TlsError::Mismatch { part, reason },
        HttpAdapterError::Identity { reason, .. } => TlsError::Unusable { part, reason },
        HttpAdapterError::Certificate { index, reason } => TlsError::Unusable { part: TlsPart::Ca(index), reason },
        other => TlsError::Unusable { part, reason: other.to_string() },
    }
}

#[cfg(test)]
#[path = "loader_tests.rs"]
mod tests;
