//! What every entry point loads before anything is recorded (D280, D395, D400): its declared
//! secrets, then its `http.tls`, then the transport built from it, in one step. `vc run` and
//! `vc load` race the whole step against their interrupts and mask their example-row labels
//! with what it hides; `vc call`, `vc test auth` and a cache clear take it before they record or
//! send anything. So a secret or TLS refusal leaves no run started but never ended.

use std::collections::BTreeSet;
use std::sync::Arc;

use vc_executor::{ClientCerts, MaskRules, Redactor, RuleHooks};
use vc_interfaces::{Secret, Secrets, Transport};

use crate::plan::Prepared;
use crate::ports::SecretSource;
use crate::{Engine, EngineError, Interrupts, RunReport};

/// The exit code of a run an interrupt ended (D127).
pub const EXIT_CANCELLED: u8 = 130;

/// What a run is secured with: its declared secrets, the transport built from its `http.tls`,
/// the `CertId` of each identity that transport presents, and the values the redactor hides
/// besides the secrets.
pub struct Secured {
    pub(crate) secrets: Secrets,
    pub(crate) transport: Arc<dyn Transport>,
    pub(crate) certs: ClientCerts,
    pub(crate) hidden: Vec<Secret>,
}

impl Engine {
    /// The secrets in `reads` that `prepared` declares, then its `http.tls` rendered with them,
    /// then the transport built from it.
    ///
    /// # Errors
    ///
    /// The errors of [`Engine::secrets`] and [`Engine::with_tls`].
    pub(crate) async fn secure(&self, prepared: &Prepared, reads: &BTreeSet<String>) -> Result<Secured, EngineError> {
        let secrets = self.secrets(SecretSource::of(prepared), reads).await?;
        self.with_tls(prepared, secrets).await
    }

    /// [`Engine::secure`] for a run of `prepared`, with its example-row labels masked by a
    /// redactor like the one its events go through, so no label reaches a reporter, the history
    /// or a watcher unmasked. `None` when an interrupt comes first: the step, which a PKCS#12
    /// bundle's key derivation can make slow, is not waited for.
    ///
    /// # Errors
    ///
    /// The errors of [`Engine::secure`].
    pub(crate) async fn secured(
        &self,
        prepared: &mut Prepared,
        interrupts: &mut Interrupts,
    ) -> Result<Option<Secured>, EngineError> {
        let reads = crate::ports::run_reads(prepared);
        let secured = tokio::select! {
            biased;
            Some(()) = interrupts.next() => return Ok(None),
            secured = self.secure(prepared, &reads) => secured?,
        };
        if !prepared.plan.row_names.is_empty() {
            self.mask_rows(prepared, &secured).await;
        }
        Ok(Some(secured))
    }

    /// Masks `prepared`'s example-row labels with its redact rules and what `secured` hides.
    async fn mask_rows(&self, prepared: &mut Prepared, secured: &Secured) {
        let program = Arc::clone(&prepared.program);
        let hooks = RuleHooks::new(self.hook_runner(&program), secured.secrets.clone());
        let rules = MaskRules::standard().with_hooks(hooks, &program).for_config(&program.config);
        let redactor = Redactor::for_config(&program.config).with_rules(rules);
        redactor.hide_values(secured.secrets.iter().map(|(_, secret)| secret.expose()));
        redactor.hide_values(secured.hidden.iter().map(Secret::expose));
        crate::examples::mask_labels(Arc::make_mut(&mut prepared.plan), &redactor).await;
    }
}

impl Secured {
    /// The secrets, moved out for the run's inputs; the step keeps none.
    pub(crate) fn take_secrets(&mut self) -> Secrets {
        std::mem::take(&mut self.secrets)
    }
}

/// The report of a run interrupted before it started: nothing ran, nothing was recorded.
pub fn cancelled(prepared: Prepared) -> RunReport {
    let seed = prepared.draw.seed;
    RunReport { exit: EXIT_CANCELLED, seed, generated: false, notices: prepared.notices, run: None, html_report: None }
}

#[cfg(test)]
#[path = "secured_tests.rs"]
mod tests;
