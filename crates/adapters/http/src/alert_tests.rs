use std::{fmt, io};

use rustls::AlertDescription;
use vc_interfaces::{SentCert, TransportError};

use super::{ASKS, REFUSALS, Refusal, failed_handshake};

/// An error that says `text` and whose cause is `source`, as hyper wraps one.
#[derive(Debug)]
struct Wrapper<E> {
    text: &'static str,
    source: E,
}

impl<E> fmt::Display for Wrapper<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.text)
    }
}

impl<E: std::error::Error + 'static> std::error::Error for Wrapper<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

const fn wrapped<E>(source: E) -> Wrapper<E> {
    Wrapper { text: "wrapped", source }
}

fn received(alert: AlertDescription) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, rustls::Error::AlertReceived(alert))
}

fn refused(alert: AlertDescription, sent: SentCert) -> Option<TransportError> {
    Refusal::find(&wrapped(received(alert))).map(|refusal| refusal.at("h", sent))
}

fn rejected(alert: &str, reason: &str, sent: SentCert) -> TransportError {
    TransportError::ClientCertRejected { host: "h".into(), alert: alert.into(), reason: reason.into(), sent }
}

#[test]
fn each_refusing_alert_is_found_under_its_spec_name() {
    let names = REFUSALS.map(|(alert, ..)| Refusal::find(&wrapped(received(alert))).map(|found| found.alert));
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
    assert_eq!(Refusal::find(&wrapped(copied)).map(|found| found.alert), Some("certificate_required"));
}

#[test]
fn a_rustls_error_in_the_chain_wins_over_alert_text() {
    let said = Wrapper { text: "received fatal alert: UnknownCA", source: rustls::Error::NoCertificatesPresented };
    assert_eq!(Refusal::find(&said), None);
}

#[test]
fn other_alerts_and_errors_are_not_refusals() {
    assert_eq!(Refusal::find(&wrapped(received(AlertDescription::HandshakeFailure))), None);
    assert_eq!(Refusal::find(&rustls::Error::NoCertificatesPresented), None);
    assert_eq!(Refusal::find(&io::Error::other("reset")), None);
}

#[test]
fn a_sent_certificate_is_refused_for_what_the_alert_means() {
    let named = || SentCert::Named("good".into());
    let corrupt = "the certificate is corrupt or its signature fails";
    assert_eq!(refused(AlertDescription::BadCertificate, named()), Some(rejected("bad_certificate", corrupt, named())));
    let denied = "the server denied access (often: the certificate is valid but not allowed)";
    assert_eq!(refused(AlertDescription::AccessDenied, named()), Some(rejected("access_denied", denied, named())));
}

#[test]
fn without_a_certificate_every_refusal_says_the_server_asks_for_one() {
    for sent in [SentCert::Missing, SentCert::Off] {
        for (alert, name, _) in REFUSALS {
            assert_eq!(refused(alert, sent.clone()), Some(rejected(name, ASKS, sent.clone())));
        }
    }
}

#[test]
fn a_handshake_failure_without_a_certificate_hints_at_adding_one() {
    let failure = received(AlertDescription::HandshakeFailure);
    let reason = "received fatal alert: HandshakeFailure; if the server requires a client certificate, add an \
                  `http.tls.clients` entry for `h`";
    let expected = TransportError::Tls { host: "h".into(), reason: reason.into() };
    assert_eq!(failed_handshake(&failure, "h", &SentCert::Missing), expected);
}

#[test]
fn other_failed_handshakes_keep_their_reason() {
    let plain = |error: &io::Error| TransportError::Tls { host: "h".into(), reason: error.to_string() };
    let failure = received(AlertDescription::HandshakeFailure);
    for sent in [SentCert::Off, SentCert::Named("good".into())] {
        assert_eq!(failed_handshake(&failure, "h", &sent), plain(&failure));
    }
    let other = received(AlertDescription::ProtocolVersion);
    assert_eq!(failed_handshake(&other, "h", &SentCert::Missing), plain(&other));
}
