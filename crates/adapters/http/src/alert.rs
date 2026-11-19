//! The TLS alerts a server sends to refuse a client certificate (V5127).

use std::error::Error;

use rustls::AlertDescription;

use crate::HttpAdapterError;

/// Each alert that refuses a client certificate: its name in the TLS spec and what it means.
const REFUSALS: [(AlertDescription, &str, &str); 7] = [
    (AlertDescription::CertificateRequired, "certificate_required", "it requires a client certificate"),
    (AlertDescription::BadCertificate, "bad_certificate", "the certificate is corrupt or its signature fails"),
    (AlertDescription::UnknownCA, "unknown_ca", "it doesn't trust the issuer"),
    (AlertDescription::CertificateExpired, "certificate_expired", "the certificate has expired or is not valid yet"),
    (AlertDescription::CertificateRevoked, "certificate_revoked", "the issuer revoked the certificate"),
    (AlertDescription::UnsupportedCertificate, "unsupported_certificate", "it doesn't take this kind of certificate"),
    (AlertDescription::AccessDenied, "access_denied", "the certificate is valid but not let in"),
];

/// A refused client certificate: the alert's name and what it means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Refusal {
    alert: &'static str,
    reason: &'static str,
}

impl Refusal {
    /// The refusal `error` or one of its causes carries.
    pub fn find(error: &(dyn Error + 'static)) -> Option<Self> {
        let mut cause = Some(error);
        while let Some(error) = cause {
            if let Some(found) = Self::of(error) {
                return Some(found);
            }
            cause = error.source();
        }
        None
    }

    /// The refusal `error` itself reports, told by rustls's own text for the alert: an
    /// `io::Error` shows the rustls error it wraps, though `source` skips it, and h2 keeps only
    /// that text, so under HTTP/2 the rustls error's type is gone.
    fn of(error: &dyn Error) -> Option<Self> {
        let text = error.to_string();
        let known = REFUSALS.iter().find(|(alert, ..)| rustls::Error::AlertReceived(*alert).to_string() == text);
        known.map(|&(_, alert, reason)| Self { alert, reason })
    }

    /// The error for this refusal by `host`.
    pub fn at(self, host: &str) -> HttpAdapterError {
        let (alert, reason) = (self.alert.to_owned(), self.reason.to_owned());
        HttpAdapterError::ClientCertRejected { host: host.to_owned(), alert, reason }
    }
}

#[cfg(test)]
mod tests {
    use std::{fmt, io};

    use rustls::AlertDescription;

    use super::{REFUSALS, Refusal};

    /// An error whose cause is an `io::Error`, as hyper wraps one.
    #[derive(Debug)]
    struct Wrapper(io::Error);

    impl fmt::Display for Wrapper {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("wrapped")
        }
    }

    impl std::error::Error for Wrapper {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.0)
        }
    }

    fn received(alert: AlertDescription) -> io::Error {
        io::Error::new(io::ErrorKind::InvalidData, rustls::Error::AlertReceived(alert))
    }

    #[test]
    fn each_refusing_alert_is_found_under_its_spec_name() {
        let names = REFUSALS.map(|(alert, ..)| Refusal::find(&Wrapper(received(alert))).map(|found| found.alert));
        let expected = [
            "certificate_required",
            "bad_certificate",
            "unknown_ca",
            "certificate_expired",
            "certificate_revoked",
            "unsupported_certificate",
            "access_denied",
        ];
        assert_eq!(names, expected.map(Some));
    }

    #[test]
    fn finds_a_bare_rustls_alert_and_its_meaning() {
        let found = Refusal::find(&rustls::Error::AlertReceived(AlertDescription::UnknownCA));
        assert_eq!(found, Some(Refusal { alert: "unknown_ca", reason: "it doesn't trust the issuer" }));
    }

    #[test]
    fn finds_the_text_h2_keeps_of_an_alert() {
        let copied = io::Error::new(io::ErrorKind::InvalidData, "received fatal alert: CertificateRequired".to_owned());
        assert_eq!(Refusal::find(&Wrapper(copied)).map(|found| found.alert), Some("certificate_required"));
    }

    #[test]
    fn other_alerts_and_errors_are_not_refusals() {
        assert_eq!(Refusal::find(&Wrapper(received(AlertDescription::HandshakeFailure))), None);
        assert_eq!(Refusal::find(&rustls::Error::NoCertificatesPresented), None);
        assert_eq!(Refusal::find(&io::Error::other("reset")), None);
    }
}
