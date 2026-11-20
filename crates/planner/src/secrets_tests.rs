use std::collections::BTreeSet;

use vc_model::{ClientMaterial, Namespace, Part, ProfileId, Template, TlsClient, TlsClientId};

use super::secrets_from;
use crate::testing::{Fixture, read};

/// `${secret.<name>.token}`.
fn secret(name: &str) -> Template {
    let mut template = read(name);
    if let Some(Part::Ref(reference)) = template.parts.last_mut() {
        reference.ns = Namespace::Secret;
    }
    template
}

fn client(fixture: &Fixture, id: &str, profiles: Vec<ProfileId>, material: ClientMaterial) -> (TlsClientId, TlsClient) {
    let id = TlsClientId::new(fixture.sym(id));
    (id, TlsClient { id, name: None, hosts: Vec::new(), profiles, material })
}

#[test]
fn the_run_scope_reads_the_tls_material_of_the_clients_the_profiles_allow() {
    let mut fixture = Fixture::new();
    let (dev, admin) = (ProfileId::new(fixture.sym("dev")), ProfileId::new(fixture.sym("admin")));
    let pem = ClientMaterial::Pem { cert: secret("CERT"), key: secret("KEY") };
    let bundle = ClientMaterial::Pkcs12 { file: secret("BUNDLE"), passphrase: Some(secret("PASS")) };
    let clients = [client(&fixture, "warm", vec![admin], pem), client(&fixture, "seed", Vec::new(), bundle)];
    fixture.program.config.http_tls.ca = vec![secret("CA")];
    fixture.program.config.http_tls.clients = clients.into_iter().collect();
    let read = |profiles: &[ProfileId]| secrets_from(&fixture.program, &[], profiles);
    let names = |names: &[&str]| names.iter().map(|name| (*name).to_owned()).collect::<BTreeSet<_>>();
    assert_eq!(read(&[dev]), names(&["BUNDLE", "CA", "PASS"]));
    assert_eq!(read(&[admin]), names(&["BUNDLE", "CA", "CERT", "KEY", "PASS"]));
}
