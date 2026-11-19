//! The TLS alerts a server sends to refuse a client certificate (V5127), and the TLS errors
//! (V5105) the other alerts make, with a hint when no certificate was sent.

use std::error::Error;
use std::io;

use rustls::AlertDescription;
use vc_interfaces::{SentCert, TransportError};

use crate::error::causes;

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

/// A refused client certificate: the alert's name and what it means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Refusal {
    alert: &'static str,
    reason: &'static str,
}

impl Refusal {
    /// The refusal `alert` means, if it refuses a client certificate.
    fn of(alert: AlertDescription) -> Option<Self> {
        let known = REFUSALS.iter().find(|(refusing, ..)| *refusing == alert);
        known.map(|&(_, alert, reason)| Self { alert, reason })
    }
}

/// A failed request to `host` that sent the certificate `sent`, and the alert its error carries.
#[derive(Debug)]
pub struct Failure<'a> {
    alert: Option<AlertDescription>,
    host: &'a str,
    sent: SentCert,
}

impl<'a> Failure<'a> {
    /// The failure `error` tells of: the alert it or one of its causes carries.
    pub fn of(error: &(dyn Error + 'static), host: &'a str, sent: SentCert) -> Self {
        Self { alert: received(error), host, sent }
    }

    /// The refused client certificate (V5127), when the alert refuses one.
    pub fn refused(&self) -> Option<TransportError> {
        let Refusal { alert, reason } = Refusal::of(self.alert?)?;
        let (host, alert, reason) = (self.host.to_owned(), alert.to_owned(), reason.to_owned());
        Some(TransportError::ClientCertRejected { host, alert, reason, sent: self.sent.clone() })
    }

    /// The handshake that failed for `reason` (V5105). When the server answered
    /// `handshake_failure` and no certificate matched the host, the reason adds how to send one:
    /// OpenSSL answers a missing certificate so, though the alert has many other causes.
    pub fn failed(&self, reason: String) -> TransportError {
        let hint = self.sent == SentCert::Missing && self.alert == Some(AlertDescription::HandshakeFailure);
        let reason = if hint { format!("{reason}; {}", self.sent.advice(self.host)) } else { reason };
        TransportError::Tls { host: self.host.to_owned(), reason }
    }

    /// The handshake that failed for the alert alone, when it came after the handshake seemed
    /// done: TLS 1.3 sends the server's verdict on the client on the first read.
    pub fn alerted(&self) -> Option<TransportError> {
        self.alert.map(|alert| self.failed(rustls::Error::AlertReceived(alert).to_string()))
    }
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
