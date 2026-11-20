//! The client certificates a run presents (#105, D350–D354, D390–D393): `http.tls` rendered
//! once from the config's vars, secrets and environment, read by the TLS loader on a blocking
//! thread, and the identities the active profiles allow handed to the transport. Material that
//! cannot be read or parsed stops the run (V5125), and so do parts that do not fit (V5126),
//! before any request. No reason quotes a key, and passphrases go to the redactor.

use std::path::Path;

use vc_diagnostics::Code;
use vc_executor::{ClientCerts, RunInputs, RunValues, config_text};
use vc_interfaces::{
    CertId, ClientSource, ClientSpec, LoadedTls, MaterialSource, Passphrase, TlsError, TlsPart, TlsSpec,
};
use vc_model::{ClientMaterial, ProfileId, Program, Template, TlsClient};
use zeroize::Zeroizing;

use crate::plan::Prepared;
use crate::{Engine, EngineError};

/// What a run's `http.tls` gave: the material the transport presents, the `CertId` of each
/// identity it loaded, and the passphrases the redactor hides.
#[derive(Default)]
pub struct RunTls {
    pub(crate) loaded: Option<LoadedTls>,
    pub(crate) certs: ClientCerts,
    pub(crate) passphrases: Vec<Zeroizing<String>>,
}

impl Engine {
    /// The client certificates and trusted roots of `prepared`'s `http.tls`, with `inputs`'
    /// secrets and environment, read by the TLS loader on a blocking thread; nothing to load
    /// gives the defaults.
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
        let (spec, certs) = self.planned(prepared, inputs)?;
        let passphrases = spec.clients.iter().filter_map(passphrase).collect();
        let reader = std::sync::Arc::clone(&self.tls_loader);
        let read = tokio::task::spawn_blocking(move || reader.load(spec)).await;
        let read = read.map_err(|error| EngineError::Internal { reason: format!("reading `http.tls`: {error}") })?;
        Ok(RunTls { loaded: Some(read.map_err(|error| tls_error(&error))?), certs, passphrases })
    }

    /// `prepared`'s `http.tls` rendered with `inputs`, keeping the identities its profiles allow,
    /// and the `CertId` of each.
    fn planned(&self, prepared: &Prepared, inputs: &RunInputs) -> Result<(TlsSpec, ClientCerts), EngineError> {
        let (program, tls) = (&*prepared.program, &prepared.program.config.http_tls);
        let values = RunValues::new(program, inputs.clone());
        let render = Render { program, values: &values, root: self.root() };
        let ca = tls.ca.iter().enumerate().map(|(index, template)| render.source(template, &TlsPart::Ca(index)));
        let ca = ca.collect::<Result<Vec<_>, _>>()?;
        let active: Vec<_> = tls.clients.values().filter(|client| active(client, &prepared.profiles)).collect();
        let clients = active.iter().map(|client| render.client(client)).collect::<Result<Vec<_>, _>>()?;
        let certs = active.iter().zip(0..).map(|(client, index)| (client.id, CertId::new(index))).collect();
        Ok((TlsSpec { ca, clients }, certs))
    }
}

/// Whether `client` goes with a run under `profiles`: it names none of its own, or one of them (D354).
fn active(client: &TlsClient, profiles: &[ProfileId]) -> bool {
    client.profiles.is_empty() || client.profiles.iter().any(|profile| profiles.contains(profile))
}

/// A PKCS#12 identity's passphrase, when it has one to hide.
fn passphrase(client: &ClientSpec) -> Option<Zeroizing<String>> {
    let ClientSource::Pkcs12 { passphrase: Passphrase(passphrase), .. } = &client.material else { return None };
    Some(passphrase.clone()).filter(|passphrase| !passphrase.is_empty())
}

/// Renders `http.tls` values with the config's scope; relative paths are under `root`.
struct Render<'a> {
    program: &'a Program,
    values: &'a RunValues,
    root: &'a Path,
}

impl Render<'_> {
    /// `template` as text, or the run-stopping error for `part`.
    fn text(&self, template: &Template, part: &TlsPart) -> Result<Zeroizing<String>, EngineError> {
        let rendered = config_text(self.program, self.values, template);
        rendered.map(Zeroizing::new).map_err(|error| {
            refused(error.code.unwrap_or(Code::V5125), &part.to_string(), &error.message)
        })
    }

    /// `template` as inline PEM when it starts `-----BEGIN`, else as a path: absolute as it is,
    /// else under the project root.
    fn source(&self, template: &Template, part: &TlsPart) -> Result<MaterialSource, EngineError> {
        let text = self.text(template, part)?;
        if text.starts_with("-----BEGIN") {
            return Ok(MaterialSource::Inline(Zeroizing::new(text.as_bytes().to_vec())));
        }
        Ok(MaterialSource::File { path: self.root.join(text.as_str()), shown: Some(text.to_string()) })
    }

    /// `client`'s ID, hosts and material, rendered.
    fn client(&self, client: &TlsClient) -> Result<ClientSpec, EngineError> {
        let id = self.program.str(client.id.sym()).unwrap_or_default().to_owned();
        let part = |field| TlsPart::Client { id: id.clone(), field: Some(field) };
        let material = match &client.material {
            ClientMaterial::Pem { cert, key } => {
                ClientSource::Pem { cert: self.source(cert, &part("cert"))?, key: self.source(key, &part("key"))? }
            }
            ClientMaterial::Pkcs12 { file, passphrase } => {
                let text = |template| self.text(template, &part("passphrase"));
                let passphrase = passphrase.as_ref().map(text).transpose()?.unwrap_or_default();
                ClientSource::Pkcs12 { bundle: self.source(file, &part("pkcs12"))?, passphrase: Passphrase(passphrase) }
            }
        };
        Ok(ClientSpec { id, hosts: client.hosts.clone(), material })
    }
}

/// The run-stopping error for `http.tls` material the loader refused: V5126 for parts that do
/// not fit, V5125 for material that cannot be read or parsed.
fn tls_error(error: &TlsError) -> EngineError {
    match error {
        TlsError::Unusable { part, reason } => refused(Code::V5125, &part.to_string(), reason),
        TlsError::Mismatch { part, reason } => refused(Code::V5126, &part.to_string(), reason),
    }
}

/// The `http.tls` setting error `code` for `what`, with `reason`.
pub fn refused(code: Code, what: &str, reason: &str) -> EngineError {
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
