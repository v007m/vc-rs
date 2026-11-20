use std::path::PathBuf;

use vc_interfaces::{ClientSource, ClientSpec, MaterialSource, TlsError, TlsPart, TlsSpec, TransportFactory};

use crate::MemoryEnvironment;

/// The contract every [`TransportFactory`] meets (D112, D390, D399).
///
/// Nothing to load builds, and a file that cannot be read is refused naming its part, quoting
/// the path only when the config showed it. Call it from within a Tokio runtime.
///
/// # Panics
///
/// When `factory` breaks the contract.
pub fn transport_factory_suite(factory: &dyn TransportFactory) {
    let environment = MemoryEnvironment::new(&[]);
    let build = |spec: TlsSpec| factory.build(&spec, &environment).map(|_| ());
    assert!(build(TlsSpec::default()).is_ok(), "nothing to load builds");
    let gone = |shown: Option<&str>| MaterialSource::File {
        path: PathBuf::from("/vc-contract/missing/gone.pem"),
        shown: shown.map(str::to_owned),
    };
    let ca = build(TlsSpec { ca: vec![gone(Some("certs/gone.pem"))], clients: Vec::new() });
    let reason = match &ca {
        Err(TlsError::Unusable { part: TlsPart::Ca(0), reason }) => reason.as_str(),
        _ => "",
    };
    assert!(reason.contains("`certs/gone.pem`"), "a missing `ca` file is unusable, quoting its shown path: {ca:?}");
    let material = ClientSource::Pem { cert: gone(None), key: gone(None) };
    let client = ClientSpec { id: "partner".to_owned(), hosts: Vec::new(), material };
    let cert = build(TlsSpec { ca: Vec::new(), clients: vec![client] });
    let part = TlsPart::Client { id: "partner".to_owned(), field: Some("cert") };
    let reason = match &cert {
        Err(TlsError::Unusable { part: at, reason }) if *at == part => reason.as_str(),
        _ => "gone",
    };
    assert!(!reason.contains("gone"), "a missing `cert` file is unusable, never quoting a path not shown: {cert:?}");
}
