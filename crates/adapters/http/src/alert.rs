//! The TLS alerts a server sends to refuse a client certificate (V5127), and the hint a failed
//! handshake gets when no certificate was sent.

use std::error::Error;
use std::io;

use rustls::AlertDescription;
use vc_interfaces::{SentCert, TransportError};

/// Each alert that refuses a client certificate: its name in the TLS spec and what it means.
const REFUSALS: [(AlertDescription, &str, &str); 7] = [
    (AlertDescription::CertificateRequired, "certificate_required", "it requires a client certificate"),
    (AlertDescription::BadCertificate, "bad_certificate", "the certificate is corrupt or its signature fails"),
    (AlertDescription::UnknownCA, "unknown_ca", "it doesn't trust the issuer"),
    (AlertDescription::CertificateExpired, "certificate_expired", "the certificate has expired or is not valid yet"),
    (AlertDescription::CertificateRevoked, "certificate_revoked", "the issuer revoked the certificate"),
    (AlertDescription::UnsupportedCertificate, "unsupported_certificate", "it doesn't take this kind of certificate"),
    (
        AlertDescription::AccessDenied,
        "access_denied",
        "the server denied access (often: the certificate is valid but not allowed)",
    ),
];

/// The reason of every refusal when no certificate was sent, whatever the alert: a server
/// that gets none answers `bad_certificate` (Go) or another alert as readily as
/// `certificate_required`.
const ASKS: &str = "the server asks for a client certificate";

/// A refused client certificate: the alert's name and what it means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Refusal {
    alert: &'static str,
    reason: &'static str,
}

impl Refusal {
    /// The refusal `error` or one of its causes carries.
    pub fn find(error: &(dyn Error + 'static)) -> Option<Self> {
        let alert = received(error)?;
        let known = REFUSALS.iter().find(|(refusing, ..)| *refusing == alert);
        known.map(|&(_, alert, reason)| Self { alert, reason })
    }

    /// The error for this refusal by `host` of the certificate `sent`.
    pub fn at(self, host: &str, sent: SentCert) -> TransportError {
        let reason = if matches!(sent, SentCert::Named(_)) { self.reason } else { ASKS };
        let (host, alert, reason) = (host.to_owned(), self.alert.to_owned(), reason.to_owned());
        TransportError::ClientCertRejected { host, alert, reason, sent }
    }
}

/// The TLS handshake with `host` that failed with `error` (V5105). When the server answered
/// `handshake_failure` and no certificate matched the host, the reason adds how to send one:
/// OpenSSL answers a missing certificate so on TLS 1.2, though the alert has many other causes.
pub fn failed_handshake(error: &io::Error, host: &str, sent: &SentCert) -> TransportError {
    let mut reason = error.to_string();
    if *sent == SentCert::Missing && received(error) == Some(AlertDescription::HandshakeFailure) {
        reason = format!(
            "{reason}; if the server requires a client certificate, add an `http.tls.clients` entry for `{host}`"
        );
    }
    TransportError::Tls { host: host.to_owned(), reason }
}

/// `error` and its causes, outermost first.
pub fn causes<'a>(error: &'a (dyn Error + 'static)) -> impl Iterator<Item = &'a (dyn Error + 'static)> {
    std::iter::successors(Some(error), |&cause| cause.source())
}

/// The alert the server sent, told by the first rustls error among `error` and its causes. Only
/// when none is a rustls error does it compare their text with rustls's for each refusing alert:
/// h2 keeps just the text of an I/O error, so under HTTP/2 the type is gone.
fn received(error: &(dyn Error + 'static)) -> Option<AlertDescription> {
    if let Some(typed) = causes(error).find_map(rustls_error) {
        return if let rustls::Error::AlertReceived(alert) = typed { Some(*alert) } else { None };
    }
    let texts = REFUSALS.map(|(alert, ..)| (alert, rustls::Error::AlertReceived(alert).to_string()));
    causes(error).find_map(|cause| {
        let text = cause.to_string();
        texts.iter().find(|(_, known)| *known == text).map(|&(alert, _)| alert)
    })
}

/// `error` as a rustls error: itself, or the one an `io::Error` wraps (tokio-rustls wraps it so,
/// and `io::Error::source` skips it).
fn rustls_error<'a>(error: &'a (dyn Error + 'static)) -> Option<&'a rustls::Error> {
    let wrapped = || error.downcast_ref::<io::Error>()?.get_ref()?.downcast_ref::<rustls::Error>();
    error.downcast_ref::<rustls::Error>().or_else(wrapped)
}

#[cfg(test)]
#[path = "alert_tests.rs"]
mod tests;
