//! What a run loads before any reporter, history run or watcher starts (D280, D395): its
//! declared secrets and its `http.tls`, with its example-row labels masked by both. `vc run` and
//! `vc load` start here, so a secret or TLS refusal leaves no run started but never ended.

use std::sync::Arc;

use vc_executor::{MaskRules, Redactor, RuleHooks};
use vc_interfaces::{Secret, Secrets};

use crate::client_certs::RunTls;
use crate::plan::Prepared;
use crate::ports::SecretSource;
use crate::{Engine, EngineError, Interrupts, RunReport};

/// The exit code of a run an interrupt ended (D127).
pub const EXIT_CANCELLED: u8 = 130;

impl Engine {
    /// `prepared`'s declared secrets and `http.tls`, with its example-row labels masked by a
    /// redactor like the one its events go through, its secrets' values and passphrases hidden,
    /// so no label reaches a reporter, the history or a watcher unmasked. `None` when an
    /// interrupt comes while `http.tls` loads, which a PKCS#12 bundle's key derivation can make
    /// slow: the load is not waited for.
    ///
    /// # Errors
    ///
    /// The errors of [`Engine::secrets`] and [`Engine::run_tls`].
    pub(crate) async fn secured(
        &self,
        prepared: &mut Prepared,
        interrupts: &mut Interrupts,
    ) -> Result<Option<(Secrets, RunTls)>, EngineError> {
        let secrets = self.secrets(SecretSource::of(prepared), &crate::ports::run_reads(prepared)).await?;
        let tls = tokio::select! {
            biased;
            Some(()) = interrupts.next() => return Ok(None),
            tls = self.run_tls(prepared, &secrets) => tls?,
        };
        if !prepared.plan.row_names.is_empty() {
            self.mask_rows(prepared, &secrets, &tls).await;
        }
        Ok(Some((secrets, tls)))
    }

    /// Masks `prepared`'s example-row labels with its redact rules, `secrets`' values and `tls`'s
    /// passphrases.
    async fn mask_rows(&self, prepared: &mut Prepared, secrets: &Secrets, tls: &RunTls) {
        let program = Arc::clone(&prepared.program);
        let hooks = RuleHooks::new(self.hook_runner(&program), secrets.clone());
        let rules = MaskRules::standard().with_hooks(hooks, &program).for_config(&program.config);
        let redactor = Redactor::for_config(&program.config).with_rules(rules);
        redactor.hide_values(secrets.iter().map(|(_, secret)| secret.expose()));
        redactor.hide_values(tls.passphrases.iter().map(Secret::expose));
        crate::examples::mask_labels(Arc::make_mut(&mut prepared.plan), &redactor).await;
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
