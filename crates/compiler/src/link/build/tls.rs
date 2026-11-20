//! The config's `http.tls` and an auth's `clientCert` (D350), as written: the engine
//! renders and reads them when the run starts.

use indexmap::IndexMap;
use vc_model::{ClientCert, ClientMaterial, Config, ProfileId, Template, TlsClient, TlsClientId, TlsConfig};
use vc_parser::Node;

use super::{Build, Built, read};
use crate::link::project;
use crate::link::project::Source;

/// The `clientCert` that sends no certificate.
pub const NO_CLIENT_CERT: &str = "none";

impl<'a> Build<'_, 'a> {
    /// `config` with `http.tls`.
    pub fn tls(&mut self, source: &Source<'a>, config: Config) -> Built<Config> {
        let Some(tls) = project::tls(source.root) else { return Ok(config) };
        let mut clients = IndexMap::new();
        for node in project::tls_clients(source.root) {
            let Some(client) = self.tls_client(source, node)? else { continue };
            clients.entry(client.id).or_insert(client);
        }
        Ok(Config { http_tls: TlsConfig { ca: ca(source, tls), clients }, ..config })
    }

    /// An auth's `clientCert`.
    pub fn client_cert(&mut self, node: &Node) -> Built<Option<ClientCert>> {
        match project::text(node, "clientCert") {
            None => Ok(None),
            Some(NO_CLIENT_CERT) => Ok(Some(ClientCert::Off)),
            Some(id) => Ok(Some(ClientCert::Named(TlsClientId::new(self.sym(id)?)))),
        }
    }

    fn tls_client(&mut self, source: &Source<'a>, node: &Node) -> Built<Option<TlsClient>> {
        let (Some(id), Some(material)) = (project::text(node, "id"), material(source, node)) else {
            return Ok(None);
        };
        let mut profiles = Vec::new();
        for profile in read::texts(node, "profiles") {
            profiles.push(ProfileId::new(self.sym(&profile)?));
        }
        Ok(Some(TlsClient {
            id: TlsClientId::new(self.sym(id)?),
            name: read::owned(node, "name"),
            hosts: read::texts(node, "hosts"),
            profiles,
            material,
        }))
    }
}

/// `ca`: one template, or a list of them.
fn ca(source: &Source<'_>, tls: &Node) -> Vec<Template> {
    let Some(ca) = tls.get("ca") else { return Vec::new() };
    if ca.as_scalar().is_some() {
        return read::template(source, ca).into_iter().collect();
    }
    ca.items().iter().filter_map(|item| read::template(source, item)).collect()
}

/// `pkcs12` with its `passphrase`, or `cert` and `key`; `None` when neither is whole.
fn material(source: &Source<'_>, node: &Node) -> Option<ClientMaterial> {
    let template = |key: &str| node.get(key).and_then(|value| read::template(source, value));
    if let Some(file) = template("pkcs12") {
        return Some(ClientMaterial::Pkcs12 { file, passphrase: template("passphrase") });
    }
    Some(ClientMaterial::Pem { cert: template("cert")?, key: template("key")? })
}
