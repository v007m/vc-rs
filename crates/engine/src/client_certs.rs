//! The client certificates a run presents (#105, D350–D354, D390–D393): `http.tls` rendered
//! once from the config's vars, secrets and environment, its files read on a blocking thread,
//! and the identities the active profiles allow handed to the HTTP transport. Material that
//! cannot be read or parsed stops the run (V5125), and so do parts that do not fit (V5126),
//! before any request. No reason quotes a key, and passphrases go to the redactor.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use vc_adapter_http::{
    ClientIdentity, HttpAdapterError, HttpOptions, HttpTransport, ProxyConfig, TlsOptions, read_material,
};
use vc_diagnostics::Code;
use vc_executor::{ClientCerts, RunInputs, RunValues, config_text};
use vc_interfaces::{CertId, Environment, Transport};
use vc_model::{ClientMaterial, ProfileId, Program, Template, TlsClient};
use zeroize::Zeroizing;

use crate::plan::Prepared;
use crate::{Engine, EngineError};

/// What a run's `http.tls` gave: the transport's options, the `CertId` of each identity it
/// loaded, and the passphrases the redactor hides.
#[derive(Default)]
pub struct RunTls {
    pub(crate) options: TlsOptions,
    pub(crate) certs: ClientCerts,
    pub(crate) passphrases: Vec<Zeroizing<String>>,
}

/// Where one piece of material comes from: inline PEM, or a file.
enum Source {
    Inline(Zeroizing<Vec<u8>>),
    File(PathBuf),
}

/// An identity's material, rendered.
enum Material {
    Pem { cert: Source, key: Source },
    Pkcs12 { file: PathBuf, passphrase: Zeroizing<String> },
}

/// One identity, rendered and ready to read.
struct Rendered {
    id: String,
    hosts: Vec<String>,
    material: Material,
}

/// `http.tls` rendered: the roots and identities to read, the `CertId` of each identity, and
/// the passphrases to hide.
struct Planned {
    ca: Vec<Source>,
    clients: Vec<Rendered>,
    certs: ClientCerts,
    passphrases: Vec<Zeroizing<String>>,
}

impl Engine {
    /// The client certificates and trusted roots of `prepared`'s `http.tls`, with `inputs`'
    /// secrets and environment; nothing to load gives the defaults.
    ///
    /// # Errors
    ///
    /// [`EngineError::Setting`] with V5114, V5125 or V5126 when a value does not render, its
    /// material cannot be read or parsed, or its parts do not fit.
    pub(crate) async fn run_tls(&self, prepared: &Prepared, inputs: &RunInputs) -> Result<RunTls, EngineError> {
        let tls = &prepared.program.config.http_tls;
        if tls.ca.is_empty() && tls.clients.is_empty() {
            return Ok(RunTls::default());
        }
        let Planned { ca, clients, certs, passphrases } = self.planned(prepared, inputs)?;
        let loaded = tokio::task::spawn_blocking(move || load(ca, clients)).await;
        let options =
            loaded.map_err(|error| EngineError::Internal { reason: format!("reading `http.tls`: {error}") })?;
        Ok(RunTls { options: options?, certs, passphrases })
    }

    /// `prepared`'s `http.tls` rendered with `inputs`, keeping the identities its profiles allow.
    fn planned(&self, prepared: &Prepared, inputs: &RunInputs) -> Result<Planned, EngineError> {
        let (program, tls) = (&*prepared.program, &prepared.program.config.http_tls);
        let values = RunValues::new(program, inputs.clone());
        let render = Render { program, values: &values, root: self.root() };
        let ca = tls.ca.iter().map(|template| render.source(template, "a `ca` certificate"));
        let ca = ca.collect::<Result<Vec<_>, _>>()?;
        let active: Vec<_> = tls.clients.values().filter(|client| active(client, &prepared.profiles)).collect();
        let clients = active.iter().map(|client| render.client(client)).collect::<Result<Vec<_>, _>>()?;
        let passphrases = clients.iter().filter_map(Rendered::passphrase).collect();
        let certs = active.iter().zip(0..).map(|(client, index)| (client.id, CertId::new(index))).collect();
        Ok(Planned { ca, clients, certs, passphrases })
    }

    /// As [`Self::transport`], trusting the roots and presenting the client certificates in
    /// `tls`, a run's `http.tls`.
    ///
    /// # Errors
    ///
    /// [`EngineError::Setting`] with V5125 or V5126 when a certificate or identity in `tls` does
    /// not parse or its parts do not fit; [`EngineError::Internal`] when the client cannot be built.
    pub(crate) fn transport_with(
        &self,
        environment: &dyn Environment,
        tls: TlsOptions,
    ) -> Result<Arc<dyn Transport>, EngineError> {
        if let Some(transport) = &self.transport {
            return Ok(Arc::clone(transport));
        }
        let options =
            HttpOptions { proxy: ProxyConfig::from_env(|name| environment.var(name)), tls, ..Default::default() };
        let transport = HttpTransport::new(&options).map_err(|error| transport_error(&error))?;
        Ok(Arc::new(transport))
    }
}

/// Whether `client` goes with a run under `profiles`: it names none of its own, or one of them (D354).
fn active(client: &TlsClient, profiles: &[ProfileId]) -> bool {
    client.profiles.is_empty() || client.profiles.iter().any(|profile| profiles.contains(profile))
}

/// Renders `http.tls` values with the config's scope; relative paths are under `root`.
struct Render<'a> {
    program: &'a Program,
    values: &'a RunValues,
    root: &'a Path,
}

impl Render<'_> {
    /// `template` as text, or the run-stopping error for `what`.
    fn text(&self, template: &Template, what: &str) -> Result<Zeroizing<String>, EngineError> {
        let rendered = config_text(self.program, self.values, template);
        rendered.map(Zeroizing::new).map_err(|error| refused(error.code.unwrap_or(Code::V5125), what, &error.message))
    }

    /// `template` as a path: absolute as it is, else under the project root.
    fn path(&self, template: &Template, what: &str) -> Result<PathBuf, EngineError> {
        Ok(self.root.join(self.text(template, what)?.as_str()))
    }

    /// `template` as inline PEM when it starts `-----BEGIN`, else as a path.
    fn source(&self, template: &Template, what: &str) -> Result<Source, EngineError> {
        let text = self.text(template, what)?;
        if text.starts_with("-----BEGIN") {
            return Ok(Source::Inline(Zeroizing::new(text.as_bytes().to_vec())));
        }
        Ok(Source::File(self.root.join(text.as_str())))
    }

    /// `client`'s ID, hosts and material, rendered.
    fn client(&self, client: &TlsClient) -> Result<Rendered, EngineError> {
        let id = self.program.str(client.id.sym()).unwrap_or_default().to_owned();
        let what = format!("client certificate `{id}`");
        let material = match &client.material {
            ClientMaterial::Pem { cert, key } => {
                Material::Pem { cert: self.source(cert, &what)?, key: self.source(key, &what)? }
            }
            ClientMaterial::Pkcs12 { file, passphrase } => {
                let passphrase = passphrase.as_ref().map(|template| self.text(template, &what)).transpose()?;
                Material::Pkcs12 { file: self.path(file, &what)?, passphrase: passphrase.unwrap_or_default() }
            }
        };
        Ok(Rendered { id, hosts: client.hosts.clone(), material })
    }
}

impl Rendered {
    /// The passphrase, when it has one to hide.
    fn passphrase(&self) -> Option<Zeroizing<String>> {
        let Material::Pkcs12 { passphrase, .. } = &self.material else { return None };
        Some(passphrase.clone()).filter(|passphrase| !passphrase.is_empty())
    }

    /// The identity, its files read and a bundle decrypted.
    fn identity(self) -> Result<ClientIdentity, EngineError> {
        let what = format!("client certificate `{}`", self.id);
        let identity = match self.material {
            Material::Pem { cert, key } => {
                let (mut cert, mut key) = (read(cert, &what)?, read(key, &what)?);
                ClientIdentity::pem(self.id, std::mem::take(&mut *cert), std::mem::take(&mut *key))
            }
            Material::Pkcs12 { file, passphrase } => {
                let bundle = read(Source::File(file), &what)?;
                ClientIdentity::pkcs12(self.id, &bundle, &passphrase).map_err(|error| transport_error(&error))?
            }
        };
        Ok(identity.hosts(self.hosts))
    }
}

/// The transport's TLS options: `ca` and `clients` read, on a blocking thread.
fn load(ca: Vec<Source>, clients: Vec<Rendered>) -> Result<TlsOptions, EngineError> {
    let ca =
        ca.into_iter().map(|source| read(source, "a `ca` certificate").map(|mut bytes| std::mem::take(&mut *bytes)));
    let ca = ca.collect::<Result<_, _>>()?;
    let clients = clients.into_iter().map(Rendered::identity).collect::<Result<_, _>>()?;
    Ok(TlsOptions { ca, clients })
}

/// The bytes of `source`, or V5125 for `what` naming the file that cannot be read.
fn read(source: Source, what: &str) -> Result<Zeroizing<Vec<u8>>, EngineError> {
    match source {
        Source::Inline(bytes) => Ok(bytes),
        Source::File(path) => read_material(&path)
            .map_err(|error| refused(Code::V5125, what, &format!("cannot read `{}`: {error}", path.display()))),
    }
}

/// The run-stopping error for a transport that cannot be built from `http.tls`: V5126 for an
/// identity whose parts do not fit, V5125 for material that does not parse.
fn transport_error(error: &HttpAdapterError) -> EngineError {
    match error {
        HttpAdapterError::Identity { id, reason } => {
            refused(Code::V5125, &format!("client certificate `{id}`"), reason)
        }
        HttpAdapterError::Mismatch { id, reason } => {
            refused(Code::V5126, &format!("client certificate `{id}`"), reason)
        }
        HttpAdapterError::Certificate { index, reason } => {
            refused(Code::V5125, &format!("`ca` certificate {}", index + 1), reason)
        }
        other => EngineError::Internal { reason: other.to_string() },
    }
}

/// The `http.tls` setting error `code` for `what`, with `reason`.
fn refused(code: Code, what: &str, reason: &str) -> EngineError {
    let help = match code {
        Code::V5126 => "use the key issued with the certificate, or the bundle's own passphrase",
        Code::V5114 => "set the secret in the environment or the project's `.env`",
        _ => "check the path (relative to the project root) or the inline PEM, and that it holds PEM or PKCS#12",
    };
    EngineError::Setting {
        origin: "http.tls".to_owned(),
        reason: format!("{code}: {what}: {reason}"),
        help: Some(format!("{help}; `vc explain {code}` says more")),
    }
}

#[cfg(test)]
#[path = "client_certs_tests.rs"]
mod tests;
