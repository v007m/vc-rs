use std::sync::Arc;
use std::sync::mpsc::{self, Sender};

use async_trait::async_trait;
use vc_interfaces::{CertChoice, CertId, SendContext, Transport, TransportError};
use vc_model::{
    AuthDef, AuthId, AuthRef, ClientCert, ClientMaterial, Exchange, OwnerKey, ProfileId, RenderedRequest, TlsClient,
    TlsClientId, VarDefs,
};

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

/// Answers every request with 200 and records the certificate choice each one was sent with.
struct Choices(Sender<CertChoice>);

#[async_trait]
impl Transport for Choices {
    async fn send(&self, request: RenderedRequest, ctx: &SendContext) -> Result<Exchange, TransportError> {
        self.0.send(ctx.client_cert).ok();
        vc_testkit::FakeTransport::scripted(vec![Ok(vc_testkit::status(200))]).send(request, ctx).await
    }
}

#[tokio::test]
async fn a_call_sends_the_certificate_its_auth_names_from_those_the_run_loaded() {
    let (fixture, call, good) = call_with(|good| Some(ClientCert::Named(good)));
    let (choices, sent) = mpsc::channel();
    let parts = PipelineParts::new(Arc::new(Choices(choices)), HostLimits::spawn(8));
    let cx = ActionCx::new(Arc::new(fixture.program), Arc::new(ActionExecutors::standard(parts)), fixture::inputs(&[]))
        .with_auth(Arc::new(vc_testkit::FakeAuth::new(&["mtls"])))
        .with_client_certs(ClientCerts::from([(good, CertId::new(7))]));
    let cx = Arc::new(cx);
    let request = ActionRequest { action: call, owner: OwnerKey::Run, cx: Arc::clone(&cx), frame: Frame::default() };
    let report = cx.executors.execute(request).await;
    assert_eq!(report.outcome.failure(), None);
    assert_eq!(sent.try_iter().collect::<Vec<_>>(), [CertChoice::Named(CertId::new(7))]);
}
