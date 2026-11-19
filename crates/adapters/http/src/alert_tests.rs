use std::{fmt, io};

use rustls::AlertDescription;
use vc_interfaces::{SentCert, TransportError};

use super::{Failure, REFUSALS, Refusal};

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

fn found(error: &(dyn std::error::Error + 'static)) -> Option<&'static str> {
    Refusal::of(Failure::of(error, "h", SentCert::Missing).alert?).map(|refusal| refusal.alert)
}

fn refused(alert: AlertDescription, sent: SentCert) -> Option<TransportError> {
    Failure::of(&wrapped(received(alert)), "h", sent).refused()
}

fn rejected(alert: &str, reason: &str, sent: SentCert) -> TransportError {
    TransportError::ClientCertRejected { host: "h".into(), alert: alert.into(), reason: reason.into(), sent }
}

fn tls(reason: &str) -> TransportError {
    TransportError::Tls { host: "h".into(), reason: reason.into() }
}

const HINT: &str =
    "no client certificate was sent: if the server requires one, add an `http.tls.clients` entry for `h`";

#[test]
fn each_refusing_alert_is_found_under_its_spec_name() {
    let names = REFUSALS.map(|(alert, ..)| found(&wrapped(received(alert))));
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
    let failure = Failure::of(&rustls::Error::AlertReceived(AlertDescription::UnknownCA), "h", SentCert::Missing);
    let found = failure.alert.and_then(Refusal::of);
    assert_eq!(found, Some(Refusal { alert: "unknown_ca", reason: "it doesn't trust the issuer" }));
}

#[test]
fn finds_the_text_h2_keeps_of_an_alert() {
    let copied = io::Error::new(io::ErrorKind::InvalidData, "received fatal alert: CertificateRequired".to_owned());
    assert_eq!(found(&wrapped(copied)), Some("certificate_required"));
}

#[test]
fn a_rustls_error_in_the_chain_wins_over_alert_text() {
    let said = Wrapper { text: "received fatal alert: UnknownCA", source: rustls::Error::NoCertificatesPresented };
    assert_eq!(found(&said), None);
}

#[test]
fn other_alerts_and_errors_are_not_refusals() {
    assert_eq!(refused(AlertDescription::HandshakeFailure, SentCert::Missing), None);
    assert_eq!(found(&rustls::Error::NoCertificatesPresented), None);
    assert_eq!(found(&io::Error::other("reset")), None);
}

#[test]
fn a_refusal_keeps_what_the_alert_means_whatever_was_sent() {
    let corrupt = "the certificate is corrupt or its signature fails";
    let denied = "the server denied access (often: the certificate is valid but not allowed)";
    for sent in [SentCert::Named("good".into()), SentCert::Missing, SentCert::Off] {
        let bad = refused(AlertDescription::BadCertificate, sent.clone());
        assert_eq!(bad, Some(rejected("bad_certificate", corrupt, sent.clone())));
        let access = refused(AlertDescription::AccessDenied, sent.clone());
        assert_eq!(access, Some(rejected("access_denied", denied, sent)));
    }
}

#[test]
fn a_refusal_without_a_certificate_adds_how_to_send_one() {
    let text = refused(AlertDescription::AccessDenied, SentCert::Missing).map(|error| error.to_string());
    let denied = "the server denied access (often: the certificate is valid but not allowed)";
    assert_eq!(text, Some(format!("TLS handshake with h failed with access_denied ({denied}); {HINT}")));
}

#[test]
fn a_handshake_failure_without_a_certificate_hints_at_adding_one() {
    let failure = received(AlertDescription::HandshakeFailure);
    let failed = Failure::of(&wrapped(failure), "h", SentCert::Missing).failed("why".into());
    assert_eq!(failed, tls(&format!("why; {HINT}")));
}

#[test]
fn other_failed_handshakes_keep_their_reason() {
    let failure = || wrapped(received(AlertDescription::HandshakeFailure));
    for sent in [SentCert::Off, SentCert::Named("good".into())] {
        assert_eq!(Failure::of(&failure(), "h", sent).failed("why".into()), tls("why"));
    }
    let other = wrapped(received(AlertDescription::ProtocolVersion));
    assert_eq!(Failure::of(&other, "h", SentCert::Missing).failed("why".into()), tls("why"));
    assert_eq!(Failure::of(&io::Error::other("reset"), "h", SentCert::Missing).failed("why".into()), tls("why"));
}

#[test]
fn an_alert_on_the_first_read_fails_the_handshake_with_the_same_hint() {
    let alerted = |alert, sent| Failure::of(&wrapped(received(alert)), "h", sent).alerted();
    let failure = "received fatal alert: HandshakeFailure";
    assert_eq!(
        alerted(AlertDescription::HandshakeFailure, SentCert::Missing),
        Some(tls(&format!("{failure}; {HINT}")))
    );
    assert_eq!(alerted(AlertDescription::HandshakeFailure, SentCert::Off), Some(tls(failure)));
    let version = alerted(AlertDescription::ProtocolVersion, SentCert::Missing);
    assert_eq!(version, Some(tls("received fatal alert: ProtocolVersion")));
    assert_eq!(Failure::of(&io::Error::other("reset"), "h", SentCert::Missing).alerted(), None);
}
