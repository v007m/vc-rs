use std::sync::Arc;
use std::sync::mpsc::{self, Sender};

use async_trait::async_trait;
use vc_interfaces::{AuthError, AuthProvider, AuthRequest, CertChoice, CertId, Credentials};
use vc_model::{
    AuthDef, AuthId, AuthRef, ClientCert, ClientMaterial, OwnerKey, ProfileId, TlsClient, TlsClientId, VarDefs,
};
use vc_testkit::{FakeTransport, RecordingTransport, status};

use super::{ClientCerts, cert_choice};
use crate::action::called;
use crate::fixture::{self, Fixture};
use crate::{ActionCx, ActionExecutors, ActionRequest, Frame, HostLimits, PipelineParts};

/// What an auth's `clientCert` is, given the identity `good`; `None` for no auth.
type Pick = fn(TlsClientId) -> Option<ClientCert>;

/// A call whose api uses an auth whose `clientCert` is what `cert` gives for the identity `good`
/// (no auth when `None`); `good` is only for the profiles `staging` and `qa`.
fn call_with(cert: Pick) -> (Fixture, vc_model::ActionId, TlsClientId) {
    let mut fixture = Fixture::new();
    let good = TlsClientId::new(fixture.sym("good"));
    let profiles = ["staging", "qa"].map(|word| ProfileId::new(fixture.sym(word))).to_vec();
    let material = ClientMaterial::Pem { cert: fixture::literal("a.pem"), key: fixture::literal("a.key") };
    let client = TlsClient { id: good, name: None, hosts: vec![], profiles, material };
    fixture.program.config.http_tls.clients.insert(good, client);
    let call = fixture.call("/whoami");
    if let Some(cert) = cert(good) {
        let id = AuthId::new(fixture.sym("svc"));
        let mut def = AuthDef::new(id, "mtls");
        def.client_cert = Some(cert);
        fixture.program.auth.insert(id, def);
        fixture.api_mut(call).auth = AuthRef::Use { id, vars: VarDefs::default() };
    }
    (fixture, call, good)
}

fn choice(fixture: &Fixture, call: vc_model::ActionId, loaded: &ClientCerts) -> Result<CertChoice, vc_model::Failure> {
    let action = &fixture.program.actions[usize::try_from(call.index()).unwrap_or_default()];
    let api = called(&fixture.program, action).unwrap();
    cert_choice(&fixture.program, loaded, (api, action))
}

#[test]
fn an_auth_names_its_certificate_or_none_and_otherwise_the_host_decides() {
    let cases: [(Pick, CertChoice); 3] = [
        (|_| None, CertChoice::Host),
        (|_| Some(ClientCert::Off), CertChoice::Off),
        (|good| Some(ClientCert::Named(good)), CertChoice::Named(CertId::new(3))),
    ];
    for (cert, wanted) in cases {
        let (fixture, call, good) = call_with(cert);
        let loaded = ClientCerts::from([(good, CertId::new(3))]);
        assert_eq!(choice(&fixture, call, &loaded), Ok(wanted));
    }
}

#[test]
fn a_named_certificate_the_profiles_did_not_load_is_v5114_naming_theirs() {
    let (fixture, call, _) = call_with(|good| Some(ClientCert::Named(good)));
    let failure = choice(&fixture, call, &ClientCerts::new()).unwrap_err();
    assert_eq!(failure.code.as_deref(), Some("V5114"));
    let wanted =
        "the client certificate `good` did not load; it is only sent under `--profile staging` or `--profile qa`";
    assert_eq!(failure.message, wanted);
}

/// Gives a bearer token and records the certificate choice each request for one carries.
struct Choices(Sender<CertChoice>);

#[async_trait]
impl AuthProvider for Choices {
    async fn credentials(&self, request: &AuthRequest) -> Result<Credentials, AuthError> {
        self.0.send(request.client_cert).ok();
        Ok(Credentials::header("authorization", "Bearer t0ken"))
    }
}

/// The call and its auth's token request both carry the certificate the auth names.
#[tokio::test]
async fn a_call_and_its_token_request_send_the_certificate_its_auth_names_from_those_loaded() {
    let (fixture, call, good) = call_with(|good| Some(ClientCert::Named(good)));
    let (transport, sent) = RecordingTransport::new(Arc::new(FakeTransport::scripted(vec![Ok(status(200))])));
    let (choices, asked) = mpsc::channel();
    let parts = PipelineParts::new(Arc::new(transport), HostLimits::spawn(8));
    let cx = ActionCx::new(Arc::new(fixture.program), Arc::new(ActionExecutors::standard(parts)), fixture::inputs(&[]))
        .with_auth(Arc::new(Choices(choices)))
        .with_client_certs(ClientCerts::from([(good, CertId::new(7))]));
    let cx = Arc::new(cx);
    let request = ActionRequest { action: call, owner: OwnerKey::Run, cx: Arc::clone(&cx), frame: Frame::default() };
    let report = cx.executors.execute(request).await;
    assert_eq!(report.outcome.failure(), None);
    let named = CertChoice::Named(CertId::new(7));
    assert_eq!(sent.take().into_iter().map(|sent| sent.client_cert).collect::<Vec<_>>(), [named]);
    assert_eq!(asked.try_iter().collect::<Vec<_>>(), [named]);
}
