//! The HTTP adapter's [`TransportFactory`] (D399): reads `http.tls`'s files, parses each `ca`
//! entry and client identity once, and builds the [`HttpTransport`] from those parsed parts.

use std::fmt;
use std::sync::Arc;

use rustls::RootCertStore;
use vc_interfaces::{
    ClientSource, ClientSpec, Environment, MaterialSource, TlsError, TlsPart, TlsSpec, Transport, TransportFactory,
};
use zeroize::Zeroizing;

use crate::hosts::HostRules;
use crate::material::read_material;
use crate::tls::{add_ca, with_native};
use crate::{ClientIdentity, HttpAdapterError, HttpOptions, HttpTransport, ProxyConfig};

/// Builds the [`HttpTransport`] for a run's `http.tls`, through the proxy its environment names;
/// or, made [`fixed`](Self::fixed), checks the material the same way and gives the one transport
/// it was made with.
#[derive(Clone, Default)]
pub struct HttpTransports {
    fixed: Option<Arc<dyn Transport>>,
}

impl HttpTransports {
    /// The factory that builds an [`HttpTransport`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A factory that reads and checks `http.tls` as [`HttpTransports::new`]'s does, so material
    /// that cannot be used still stops a run (V5125, V5126), then gives `transport`.
    #[must_use]
    pub fn fixed(transport: Arc<dyn Transport>) -> Self {
        Self { fixed: Some(transport) }
    }
}

impl fmt::Debug for HttpTransports {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpTransports").field("fixed", &self.fixed.is_some()).finish()
    }
}

impl TransportFactory for HttpTransports {
    fn build(&self, tls: &TlsSpec, environment: &dyn Environment) -> Result<Arc<dyn Transport>, TlsError> {
        let (roots, checked) = parsed(tls)?;
        if let Some(transport) = &self.fixed {
            return Ok(Arc::clone(transport));
        }
        let proxy = ProxyConfig::from_env(|name| environment.var(name));
        let options = HttpOptions { proxy, ..HttpOptions::default() };
        let transport = HttpTransport::with_roots(&with_native(roots), &options, checked);
        Ok(Arc::new(transport.map_err(|error| refused(error, None))?))
    }
}

/// `tls`'s `ca` entries, parsed into roots, and its identities, each parsed into its signer and
/// its host rules parsed and checked, once: a bad rule names the client's `hosts`.
fn parsed(tls: &TlsSpec) -> Result<(RootCertStore, HostRules), TlsError> {
    let ca = tls.ca.iter().enumerate();
    let ca = ca.map(|(index, source)| read(source, &TlsPart::Ca(index)).map(|mut bytes| std::mem::take(&mut *bytes)));
    let ca: Vec<_> = ca.collect::<Result<_, _>>()?;
    let mut roots = RootCertStore::empty();
    add_ca(&mut roots, &ca).map_err(|error| refused(error, None))?;
    let clients: Vec<_> = tls.clients.iter().map(identity).collect::<Result<_, _>>()?;
    let checked = HostRules::check(clients).map_err(|error| refused(error, Some("hosts")))?;
    Ok((roots, checked))
}

/// `client`'s identity, its files read, a bundle decrypted and a PEM pair parsed.
fn identity(client: &ClientSpec) -> Result<ClientIdentity, TlsError> {
    let part = |field| TlsPart::Client { id: client.id.clone(), field: Some(field) };
    let identity = match &client.material {
        ClientSource::Pem { cert, key } => {
            let (mut cert, mut key) = (read(cert, &part("cert"))?, read(key, &part("key"))?);
            let pem = ClientIdentity::pem(client.id.clone(), std::mem::take(&mut *cert), std::mem::take(&mut *key));
            pem.parsed().map_err(|error| refused(error, None))?
        }
        ClientSource::Pkcs12 { bundle, passphrase } => {
            let bundle = read(bundle, &part("pkcs12"))?;
            let identity = ClientIdentity::pkcs12(client.id.clone(), &bundle, passphrase.expose());
            identity.map_err(|error| refused(error, Some("pkcs12")))?
        }
    };
    Ok(identity.hosts(client.hosts.clone()))
}

/// The bytes of `source`, or why `part`'s file cannot be read, naming it only as `shown`.
fn read(source: &MaterialSource, part: &TlsPart) -> Result<Zeroizing<Vec<u8>>, TlsError> {
    match source {
        MaterialSource::Inline(text) => Ok(Zeroizing::new(text.expose().as_bytes().to_vec())),
        MaterialSource::File { path, shown } => read_material(path).map_err(|error| {
            let file = shown.as_ref().map_or_else(|| "its file".to_owned(), |shown| format!("`{shown}`"));
            TlsError::Unusable { part: part.clone(), reason: format!("cannot read {file}: {error}") }
        }),
    }
}

/// `error` as the part of `http.tls` it is about, the client's `field` when one is meant: parts
/// that do not fit (V5126), material that cannot be used (V5125), else a transport that cannot
/// be built. The one place an adapter error becomes a TLS error.
fn refused(error: HttpAdapterError, field: Option<&'static str>) -> TlsError {
    match error {
        HttpAdapterError::Mismatch { id, reason } => TlsError::Mismatch { part: TlsPart::Client { id, field }, reason },
        HttpAdapterError::Identity { id, reason } => TlsError::Unusable { part: TlsPart::Client { id, field }, reason },
        HttpAdapterError::Certificate { index, reason } => TlsError::Unusable { part: TlsPart::Ca(index), reason },
        other => TlsError::Transport { reason: other.to_string() },
    }
}

#[cfg(test)]
#[path = "factory_tests.rs"]
mod tests;
