//! The client certificates a run presents (#105, D350–D354, D390–D395): `http.tls` rendered
//! once from the config's vars, secrets and environment, before any reporter or history run
//! starts, and the transport built from it by the transport factory on a blocking thread
//! (D399), with the identities the active profiles allow. Material that
//! cannot be read or parsed stops the run (V5125), and so do parts that do not fit (V5126),
//! before any request. No reason quotes a key, and passphrases go to the redactor. What the
//! built material holds that a run only notes (a client certificate's dates, a key file others
//! can read) becomes the run's notices (D412).

use std::path::Path;
use std::sync::Arc;

use vc_diagnostics::Code;
use vc_executor::{ClientCerts, RunValues, config_text};
use vc_interfaces::{
    CertId, ClientSource, ClientSpec, Environment, MaterialSource, Secret, Secrets, TlsError, TlsFinding, TlsPart,
    TlsSpec, TransportPorts,
};
use vc_model::{ClientMaterial, Program, Template, TlsClient, inline_pem};

use crate::check_tls::run_notice;
use crate::plan::Prepared;
use crate::secured::Secured;
use crate::shown::Shown;
use crate::{Engine, EngineError};

impl Engine {
    /// `secrets` and the transport for `prepared`'s `http.tls`, rendered with them and its
    /// environment, which the transport factory builds on a blocking thread, trusting its roots
    /// and presenting its client certificates; with the notices its material gives today (D412).
    ///
    /// # Errors
    ///
    /// [`EngineError::Setting`] with V5114, V5125 or V5126 when a value does not render, its
    /// material cannot be read or parsed, or its parts do not fit; [`EngineError::Internal`] when
    /// the transport cannot be built.
    pub(crate) async fn with_tls(&self, prepared: &Prepared, secrets: Secrets) -> Result<Secured, EngineError> {
        let tls = &prepared.program.config.http_tls;
        let unset = tls.ca.is_empty() && tls.clients.is_empty();
        let (spec, certs) =
            if unset { (TlsSpec::default(), ClientCerts::default()) } else { self.planned(prepared, &secrets)? };
        let hidden = spec.clients.iter().flat_map(hidden).collect();
        let (ports, findings) = self.built(spec, (Arc::clone(&prepared.environment), !unset)).await?;
        let today = self.clock.today();
        let notices = findings.iter().filter_map(|finding| run_notice(finding, today)).collect();
        let TransportPorts { transport, streams } = ports;
        Ok(Secured { secrets, transport, streams, certs, hidden, notices })
    }

    /// The transport factory's ports for `spec` and `environment`, built on a blocking
    /// thread: the factory reads files and the system's roots, and may derive PKCS#12 keys. Then,
    /// on the same thread, what the factory's `inspect` finds in `spec`'s parts (D412); a spec
    /// with none is not inspected.
    ///
    /// # Errors
    ///
    /// When `configured` (the config sets `http.tls`), [`tls_error`]'s; otherwise every refusal
    /// is [`EngineError::Internal`], since no setting is to blame.
    pub(crate) async fn built(
        &self,
        spec: TlsSpec,
        (environment, configured): (Arc<dyn Environment>, bool),
    ) -> Result<(TransportPorts, Vec<TlsFinding>), EngineError> {
        let factory = Arc::clone(&self.transports);
        let built = tokio::task::spawn_blocking(move || {
            let ports = factory.build(&spec, environment.as_ref())?;
            let probes = spec.into_probes();
            Ok::<_, TlsError>((ports, if probes.is_empty() { Vec::new() } else { factory.inspect(&probes) }))
        });
        let built = built.await;
        let built =
            built.map_err(|error| EngineError::Internal { reason: format!("building the transport: {error}") })?;
        built.map_err(
            |error| if configured { tls_error(&error) } else { EngineError::Internal { reason: error.to_string() } },
        )
    }

    /// `prepared`'s `http.tls` rendered with `secrets` and the config's inputs, no other root
    /// (`Scope::RUN_START`), keeping the identities its profiles allow, and the `CertId` of each.
    fn planned(&self, prepared: &Prepared, secrets: &Secrets) -> Result<(TlsSpec, ClientCerts), EngineError> {
        let (program, tls) = (&*prepared.program, &prepared.program.config.http_tls);
        let inputs = crate::run::config_inputs(prepared, Arc::clone(&prepared.draw.random));
        let values = RunValues::of_config(program, inputs.with_secrets(secrets.clone()));
        let render = Render { program, values: &values, root: self.root() };
        let ca = tls.ca.iter().enumerate().map(|(index, template)| render.source(template, &TlsPart::Ca(index)));
        let ca = ca.collect::<Result<Vec<_>, _>>()?;
        let active: Vec<_> = tls.active(Some(&prepared.profiles)).collect();
        let clients = active.iter().map(|client| render.client(client)).collect::<Result<Vec<_>, _>>()?;
        let certs = active.iter().zip(0..).map(|(client, index)| (client.id, CertId::new(index))).collect();
        Ok((TlsSpec { ca, clients }, certs))
    }
}

/// The shortest body line of a held key the redactor hides on its own (D405): a PEM base64
/// line is 64 characters, its last one shorter, and a shorter piece would mask ordinary text.
const LINE_HIDDEN_FROM: usize = 16;

/// What the redactor hides of `client`: a PKCS#12 bundle's passphrase, or a PEM key given
/// inline, rendered from a secret, an env var or a var or written literally (D401). Each is
/// hidden as written, trimmed and with LF line ends, and each body line of at least
/// [`LINE_HIDDEN_FROM`] characters on its own (D405); each form once.
fn hidden(client: &ClientSpec) -> Vec<Secret> {
    let held = match &client.material {
        ClientSource::Pkcs12 { passphrase, .. } => passphrase.expose(),
        ClientSource::Pem { key: MaterialSource::Inline(key), .. } => key.expose(),
        ClientSource::Pem { .. } => return Vec::new(),
    };
    let (trimmed, lf) = (held.trim(), Secret::new(held.replace("\r\n", "\n")));
    let lines = trimmed.lines().map(|line| line.trim_end_matches('\r'));
    let body = lines.filter(|line| line.len() >= LINE_HIDDEN_FROM && !line.starts_with("-----"));
    let whole = [held, trimmed, lf.expose(), lf.expose().trim()].into_iter();
    let mut forms: Vec<&str> = Vec::new();
    for form in whole.chain(body).filter(|form| !form.is_empty()) {
        if !forms.contains(&form) {
            forms.push(form);
        }
    }
    forms.into_iter().map(Secret::new).collect()
}

/// Renders `http.tls` values with the config's scope; relative paths are under `root`.
struct Render<'a> {
    program: &'a Program,
    values: &'a RunValues,
    root: &'a Path,
}

impl Render<'_> {
    /// `template` as text, or the run-stopping error for `part`, which names `template`'s
    /// expressions as the config wrote them and never what they read (D397).
    fn text(&self, template: &Template, part: &TlsPart) -> Result<Secret, EngineError> {
        let rendered = config_text(self.program, self.values, template);
        rendered.map(Secret::new).map_err(|error| {
            let reason = format!("{} does not render", self.expressions(template));
            refused(error.code.unwrap_or(Code::V5125), &part.to_string(), &reason)
        })
    }

    /// `template`'s `${…}` expressions as written, each in backticks, and nothing around them: the
    /// text between may be key material written inline.
    fn expressions(&self, template: &Template) -> String {
        let shown = Shown { program: self.program, vars: &[] };
        let written: Vec<_> =
            template.references().map(|reference| format!("`{}`", shown.written(reference))).collect();
        if written.is_empty() { "the value".to_owned() } else { written.join(", ") }
    }

    /// `template` as inline PEM when [`inline_pem`] says so, else as a [`Render::path`].
    fn source(&self, template: &Template, part: &TlsPart) -> Result<MaterialSource, EngineError> {
        let text = self.text(template, part)?;
        if inline_pem(text.expose()) {
            return Ok(MaterialSource::Inline(text));
        }
        Ok(self.file(template, &text))
    }

    /// `template` as a path, whatever it renders to (D396).
    fn path(&self, template: &Template, part: &TlsPart) -> Result<MaterialSource, EngineError> {
        Ok(self.file(template, &self.text(template, part)?))
    }

    /// `text`, `template` rendered, as a path once trimmed: absolute as it is, else under the
    /// project root, which an error quotes only when the config wrote it literally (D396).
    fn file(&self, template: &Template, text: &Secret) -> MaterialSource {
        file_at(self.root, text.expose(), template.as_literal())
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
                let passphrase = passphrase.as_ref().map(text).transpose()?.unwrap_or_else(|| Secret::new(""));
                ClientSource::Pkcs12 { bundle: self.path(file, &part("pkcs12"))?, passphrase }
            }
        };
        Ok(ClientSpec { id, hosts: client.hosts.clone(), material })
    }
}

/// The run-stopping error for `http.tls` material the transport factory refused: V5126 for
/// parts that do not fit, V5125 for material that cannot be read or parsed; a transport that
/// cannot be built for another reason is an internal error.
pub fn tls_error(error: &TlsError) -> EngineError {
    match error {
        TlsError::Unusable { part, reason } => refused(Code::V5125, &part.to_string(), reason),
        TlsError::Mismatch { part, reason } => refused(Code::V5126, &part.to_string(), reason),
        TlsError::Transport { reason } => EngineError::Internal { reason: reason.clone() },
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

/// The file at `text`, trimmed: absolute as it is, else under `root`; `literal`, the value as
/// the config wrote it when it has no `${…}`, is the only form an error may quote (D396, D408).
pub fn file_at(root: &Path, text: &str, literal: Option<&str>) -> MaterialSource {
    MaterialSource::File { path: root.join(text.trim()), shown: literal.map(|literal| literal.trim().to_owned()) }
}

#[cfg(test)]
#[path = "client_certs_tests.rs"]
mod tests;
