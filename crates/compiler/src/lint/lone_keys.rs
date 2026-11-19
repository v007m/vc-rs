//! V2211: a key that does something only beside another one that is missing: `parallel`
//! without `repeat` or `forEach`, an `abortOn` metric no threshold names, and in an
//! `http.tls.clients` entry `cert` without `key`, `key` without `cert`, or `passphrase` without
//! `pkcs12` (D350).

use vc_diagnostics::{Code, Diagnostic};
use vc_model::FileKind;
use vc_parser::{Entry, Node};

use super::{Lint, LintCx};
use crate::link;
use crate::link::Source;

/// Each `http.tls.clients` key and the partner it needs.
const TLS_PARTNERS: [(&str, &str); 3] = [("cert", "key"), ("key", "cert"), ("passphrase", "pkcs12")];

/// Reports every key whose partner is missing.
pub struct LoneKeys;

impl Lint for LoneKeys {
    fn code(&self) -> Code {
        Code::V2211
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        let mut found = Vec::new();
        for source in &cx.project.sources {
            found.extend(link::actions(source).into_iter().filter_map(|action| self.parallel(action)));
            if source.kind == FileKind::Load {
                found.extend(self.abort_on(source));
            }
            if source.kind == FileKind::Config {
                let tls = source.root.get("http").and_then(|http| http.get("tls"));
                let clients = tls.map(|tls| link::list(tls, "clients")).unwrap_or_default();
                found.extend(clients.iter().flat_map(|client| self.tls_client(client)));
            }
        }
        found
    }
}

impl LoneKeys {
    /// `parallel` on an action that does not loop.
    fn parallel(&self, action: &Node) -> Option<Diagnostic> {
        let entry = action.entry("parallel")?;
        if action.get("repeat").is_some() || action.get("forEach").is_some() {
            return None;
        }
        let diagnostic = Diagnostic::new(self.code(), entry.key_span, "`parallel` on an action that does not loop");
        let diagnostic = diagnostic.label("needs `repeat` or `forEach`");
        Some(diagnostic.help(
            "`parallel` sets how many iterations run at a time: add `repeat: 10` or `forEach`, or remove `parallel`",
        ))
    }

    /// The `abortOn` metrics that neither the scenario's nor a mix test's `thresholds` name.
    fn abort_on(&self, source: &Source<'_>) -> Vec<Diagnostic> {
        let root = source.root;
        let holders = std::iter::once(root).chain(link::list(root, "mix"));
        let named: Vec<&str> = holders
            .filter_map(|holder| holder.get("thresholds"))
            .flat_map(Node::entries)
            .map(|entry| entry.key.as_str())
            .collect();
        let metrics = link::list(root, "abortOn").iter().filter_map(|node| Some((node, node.as_str()?)));
        metrics
            .filter(|(_, metric)| !named.contains(metric))
            .map(|(node, metric)| self.unthresholded(node, metric))
            .collect()
    }

    /// `cert` or `key` without the other, and `passphrase` without `pkcs12`, in a client entry. A
    /// `cert` or `key` beside `pkcs12` is V1016, a link error, so these lints do not run then.
    fn tls_client(&self, client: &Node) -> Vec<Diagnostic> {
        let lone = TLS_PARTNERS.into_iter().filter(|(_, partner)| client.get(partner).is_none());
        lone.filter_map(|(key, partner)| Some(self.unpaired(client.entry(key)?, partner))).collect()
    }

    fn unpaired(&self, entry: &Entry, partner: &str) -> Diagnostic {
        let key = &entry.key;
        let message = format!("`{key}` without `{partner}`");
        let help = if partner == "pkcs12" {
            "`passphrase` unlocks a `pkcs12` bundle: add `pkcs12`, or remove `passphrase`".to_owned()
        } else {
            format!("a PEM client certificate needs `cert` and `key`: add `{partner}`, or give `pkcs12` instead")
        };
        Diagnostic::new(self.code(), entry.key_span, message).label(format!("needs `{partner}`")).help(help)
    }

    fn unthresholded(&self, node: &Node, metric: &str) -> Diagnostic {
        let message = format!("`abortOn` names `{metric}`, which has no threshold");
        Diagnostic::new(self.code(), node.span, message)
            .label("never breached")
            .note("a run stops early only when a threshold on the metric is breached")
            .help(format!(
                "add a threshold such as `thresholds: {{{metric}: {{lt: …}}}}`, or remove `{metric}` from `abortOn`"
            ))
    }
}
