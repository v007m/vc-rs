//! `vc check`'s look at the `http.tls` material the config writes as text (D408–D411): each value
//! is read through the transport factory's `inspect`, and each finding becomes a diagnostic at the
//! value as written. A value with a `${…}` is left to the run, which renders it first.

use std::path::Path;

use vc_diagnostics::{Code, Diagnostic, Fix, TextEdit};
use vc_interfaces::{MaterialSource, Secret, TlsFinding, TlsPart, TlsProbe};
use vc_model::{ClientMaterial, Date, Program, SourceFiles, Template, TlsClient, inline_pem};
use vc_parser::{Node, ScalarStyle, Syntax};

use crate::Engine;
use crate::check_alerts;
use crate::client_certs::file_at;
use crate::explain::did_you_mean;

/// Seconds in a UTC day: a certificate's dates are judged by the day they fall on (D410).
const SECONDS_PER_DAY: i64 = 86_400;

/// A certificate expiring within this many days is warned about (V3107, D353).
const SOON_DAYS: i64 = 30;

/// Where a path written in `http.tls` is read from.
const ROOT_HELP: &str = "paths are relative to the project root";

/// What `http.tls` material must be.
const LOAD_HELP: &str = "give a PEM certificate, an unencrypted PEM key or a PKCS#12 bundle, as a path from the \
                         project root or inline; an encrypted key goes in a bundle with a `passphrase`";

/// What fits a certificate.
const FIT_HELP: &str = "use the key issued with the certificate, or the bundle's own passphrase";

/// The characters a shell word may hold unquoted.
const SHELL_SAFE: &[char] = &['/', '.', '_', '-', '+', ':', '@', '%', ','];

impl Engine {
    /// The problems with the `http.tls` material `program` writes as text: files that cannot be
    /// read (V2011) or do not load (V2012), client certificates that expire soon (V3107) or are
    /// not valid today (V3108), and key files others can read (V4012).
    pub(crate) fn tls_problems(&self, program: &Program, files: &SourceFiles) -> Vec<Diagnostic> {
        let probes = probes(program, self.root());
        if probes.is_empty() {
            return Vec::new();
        }
        let Some(tls) = tls_node(files) else { return Vec::new() };
        let today = self.clock.today();
        let findings = self.transports.inspect(&probes);
        findings.iter().filter_map(|finding| diagnostic(&tls, finding, today)).collect()
    }
}

/// A probe for each `ca` entry and client whose material is written as text (D408).
fn probes(program: &Program, root: &Path) -> Vec<TlsProbe> {
    let tls = &program.config.http_tls;
    let ca = tls.ca.iter().enumerate();
    let ca = ca.filter_map(|(index, template)| Some(TlsProbe::Ca { index, source: source(template, root)? }));
    let clients = tls.clients.values().filter_map(|client| client_probe(program, client, root));
    ca.chain(clients).collect()
}

/// `client`'s probe: a PEM pair with each half written as text, or a bundle written as a path,
/// with its passphrase when that is text too (empty when it has none).
fn client_probe(program: &Program, client: &TlsClient, root: &Path) -> Option<TlsProbe> {
    let id = program.str(client.id.sym())?.to_owned();
    match &client.material {
        ClientMaterial::Pem { cert, key } => {
            let (cert, key) = (source(cert, root), source(key, root));
            (cert.is_some() || key.is_some()).then_some(TlsProbe::Pem { id, cert, key })
        }
        ClientMaterial::Pkcs12 { file, passphrase } => {
            let literal = file.as_literal()?;
            let bundle = file_at(root, literal, Some(literal));
            let passphrase = passphrase.as_ref().map_or(Some(""), Template::as_literal).map(Secret::new);
            Some(TlsProbe::Pkcs12 { id, bundle, passphrase })
        }
    }
}

/// `template` when it is text: inline PEM as it is, else a path from `root`.
fn source(template: &Template, root: &Path) -> Option<MaterialSource> {
    let text = template.as_literal()?;
    Some(if inline_pem(text) { MaterialSource::Inline(Secret::new(text)) } else { file_at(root, text, Some(text)) })
}

/// The config's `http.tls` node.
fn tls_node(files: &SourceFiles) -> Option<Node> {
    let (id, file) = check_alerts::config(files)?;
    let (root, _) = vc_parser::parse(id, Syntax::of(&file.path), &file.text);
    root?.get("http")?.get("tls").cloned()
}

/// The value `part` is about in `tls`: a `ca` entry, or a client's key (the entry without one);
/// else the nearest node written out that holds it (`ca`, the client, or `tls`), so no finding
/// is lost to a shape the parse does not spell out.
fn node<'n>(tls: &'n Node, part: &TlsPart) -> &'n Node {
    match part {
        TlsPart::Ca(index) => {
            let Some(ca) = tls.get("ca") else { return tls };
            if ca.as_scalar().is_some() { ca } else { ca.items().get(*index).unwrap_or(ca) }
        }
        TlsPart::Client { id, field } => {
            let clients = tls.get("clients").map_or(&[][..], Node::items);
            let Some(client) = clients.iter().find(|client| client.get("id").and_then(Node::as_str) == Some(id)) else {
                return tls;
            };
            field.and_then(|field| client.get(field)).unwrap_or(client)
        }
    }
}

/// The diagnostic `finding` gives on `tls`, at the value it is about, judged on `today`.
fn diagnostic(tls: &Node, finding: &TlsFinding, today: Date) -> Option<Diagnostic> {
    let (part, made) = match finding {
        TlsFinding::Unreadable { part, reason, near } => (part, Made::Unread { reason, near }),
        TlsFinding::Unusable { part, reason } => (part, Made::Refused("does not load", reason, LOAD_HELP)),
        TlsFinding::Mismatch { part, reason } => (part, Made::Refused("does not fit", reason, FIT_HELP)),
        TlsFinding::Validity { part, not_before, not_after } => {
            (part, Made::Dated(dated((*not_before, *not_after), today)?))
        }
        TlsFinding::Exposed { part, mode } => (part, Made::Exposed(*mode)),
    };
    Some(made.at(node(tls, part), part))
}

/// What a finding says, before it is placed.
enum Made<'f> {
    Unread { reason: &'f str, near: &'f [String] },
    Refused(&'static str, &'f str, &'static str),
    Dated(Dated),
    Exposed(u32),
}

impl Made<'_> {
    /// The diagnostic at `node`, the value `part` names.
    fn at(self, node: &Node, part: &TlsPart) -> Diagnostic {
        let written = written(node);
        match self {
            Self::Unread { reason, near } => {
                let message = format!("{part}: `{written}` cannot be read: {reason}");
                suggested(Diagnostic::new(Code::V2011, node.span, message).label("cannot be read"), node, near)
            }
            Self::Refused(label, reason, help) => {
                Diagnostic::new(Code::V2012, node.span, format!("{part} {label}: {reason}")).label(label).help(help)
            }
            Self::Dated(dated) => dated.diagnostic(node, part),
            Self::Exposed(mode) => {
                let message = format!("{part}: `{written}` is open to other users (mode {mode:03o})");
                let help = format!(
                    "make it readable by you alone, from the project root: `chmod 600 {}`",
                    shell_word(written)
                );
                Diagnostic::new(Code::V4012, node.span, message).label("open to other users").help(help)
            }
        }
    }
}

/// How a certificate's dates stand on a day (D410).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dated {
    /// It expired: the day its validity ends is before today.
    Expired(Date),
    /// It is not valid yet: the day its validity starts is after today.
    NotYet(Date),
    /// It expires on this day, within [`SOON_DAYS`] days.
    Soon(Date),
    /// Today is its last day: it may have expired already, at a time of day the check does not see.
    LastDay(Date),
}

impl Dated {
    fn diagnostic(self, node: &Node, part: &TlsPart) -> Diagnostic {
        let refused = "servers refuse it at the handshake";
        let (code, message, label, help) = match self {
            Self::Expired(on) => (
                Code::V3108,
                format!("{part} expired on {on}"),
                "expired",
                format!("{refused}; renew it, or keep it only for a test that expects the refusal"),
            ),
            Self::NotYet(from) => (
                Code::V3108,
                format!("{part} is not valid until {from}"),
                "not valid yet",
                format!(
                    "{refused} until then; check the clock it was issued with, or keep it only for a test that expects the refusal"
                ),
            ),
            Self::Soon(on) => (
                Code::V3107,
                format!("{part} expires on {on}, within {SOON_DAYS} days"),
                "expires soon",
                format!("renew it before then and point the config at the new one; after that {refused}"),
            ),
            Self::LastDay(on) => (
                Code::V3107,
                format!("{part} expires today, {on}"),
                "expires today",
                format!("renew it now and point the config at the new one; once it ends {refused}"),
            ),
        };
        Diagnostic::new(code, node.span, message).label(label).help(help)
    }
}

/// How a certificate valid from `not_before` until `not_after` (Unix seconds) stands `today`,
/// by UTC day; `None` when it is valid for more than [`SOON_DAYS`] days yet.
fn dated((not_before, not_after): (i64, i64), today: Date) -> Option<Dated> {
    let day = |seconds: i64| seconds.div_euclid(SECONDS_PER_DAY);
    let until = Date::from_unix_days(day(not_after));
    if let Some(until) = until.filter(|until| *until < today) {
        return Some(Dated::Expired(until));
    }
    if let Some(from) = Date::from_unix_days(day(not_before)).filter(|from| *from > today) {
        return Some(Dated::NotYet(from));
    }
    let until = until?;
    if until == today {
        return Some(Dated::LastDay(until));
    }
    let warned_from = Date::from_unix_days(day(not_after).saturating_sub(SOON_DAYS))?;
    (warned_from <= today).then_some(Dated::Soon(until))
}

/// `node`'s text as written, trimmed.
fn written(node: &Node) -> &str {
    node.as_scalar().map_or("", |scalar| scalar.text.trim())
}

/// `diagnostic` with the names in the missing file's folder that are close to its name, `near`:
/// one written plainly is a certain fix.
fn suggested(diagnostic: Diagnostic, node: &Node, near: &[String]) -> Diagnostic {
    let written = written(node);
    let (folder, name) = written.split_at(written.rfind(['/', '\\']).map_or(0, |at| at + 1));
    let close = vc_planner::closest(name, near.iter().map(String::as_str));
    let close: Vec<String> = close.into_iter().map(|close| format!("{folder}{close}")).collect();
    let Some(help) = did_you_mean(&close) else { return diagnostic.help(ROOT_HELP) };
    match close.as_slice() {
        [one] if plain(node) => {
            let edit = TextEdit::replace(node.span, vc_parser::write_text(one, true));
            diagnostic.help(help).fix(Fix::certain(format!("Replace with {one}"), vec![edit]))
        }
        _ => diagnostic.help(help),
    }
}

/// Whether `node` is written plainly, without quotes or a `$`, so its span is its text.
fn plain(node: &Node) -> bool {
    node.as_scalar().is_some_and(|scalar| scalar.style == ScalarStyle::Plain && !scalar.text.contains('$'))
}

/// `path` as one shell word: as it is when it is safe, else in single quotes.
fn shell_word(path: &str) -> String {
    let safe = !path.is_empty() && path.chars().all(|c| c.is_ascii_alphanumeric() || SHELL_SAFE.contains(&c));
    if safe { path.to_owned() } else { format!("'{}'", path.replace('\'', r"'\''")) }
}

#[cfg(test)]
#[path = "check_tls_tests.rs"]
mod tests;
