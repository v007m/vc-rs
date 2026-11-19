use std::sync::Arc;

use vc_model::{ClientCert, ClientMaterial, Part, Program, Template, TlsClient};

const CONFIG: &str = "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n  - id: staging\nvars: {certDir: certs}\nhttp:\n  retry: {attempts: 1}\n  tls:\n    ca: certs/ca.pem\n    clients:\n      - id: partnerClient\n        name: Partner\n        hosts: [api.partner.example, '*.partner.example:8443']\n        profiles: [staging]\n        cert: ${var.certDir}/partner.crt\n        key: ${secret.PARTNER_KEY}\n      - id: tenantA\n        pkcs12: certs/a.p12\n        passphrase: ${secret.A_PASS}\n      - id: tenantB\n        pkcs12: certs/b.p12\nauth:\n  - id: token\n    type: bearer\n    token: t\n    clientCert: tenantA\n  - id: cert\n    type: mtls\n    clientCert: tenantB\n  - id: off\n    type: mtls\n    clientCert: none\n  - id: plain\n    type: bearer\n    token: t\n";

const SCHEMA: &str = "# @sensitive\nPARTNER_KEY=\n# @sensitive\nA_PASS=\n";

fn program(config: &str) -> Arc<Program> {
    super::config_tests::program(config, SCHEMA)
}

fn text(template: &Template) -> String {
    serde_json::to_string(template).unwrap()
}

fn literal(text: &str) -> Template {
    Template { parts: vec![Part::Literal(text.to_owned())] }
}

fn client<'p>(program: &'p Program, id: &str) -> &'p TlsClient {
    program.config.http_tls.clients.values().find(|client| program.str(client.id.sym()) == Some(id)).unwrap()
}

#[test]
fn lowers_the_roots_and_each_client_in_written_order() {
    let program = program(CONFIG);
    let tls = &program.config.http_tls;
    assert_eq!(tls.ca.iter().map(text).collect::<Vec<_>>(), [text(&literal("certs/ca.pem"))]);
    let ids: Vec<_> = tls.clients.values().map(|client| program.str(client.id.sym()).unwrap()).collect();
    assert_eq!(ids, ["partnerClient", "tenantA", "tenantB"]);
    assert_eq!(program.config.http_retry.attempts, 1, "`retry` still sits beside `tls`");
}

#[test]
fn a_pem_client_keeps_its_name_hosts_profiles_and_templates() {
    let program = program(CONFIG);
    let partner = client(&program, "partnerClient");
    assert_eq!(partner.name.as_deref(), Some("Partner"));
    assert_eq!(partner.hosts, ["api.partner.example", "*.partner.example:8443"]);
    let profiles: Vec<_> = partner.profiles.iter().map(|profile| program.str(profile.sym()).unwrap()).collect();
    assert_eq!(profiles, ["staging"]);
    let ClientMaterial::Pem { cert, key } = &partner.material else { panic!("{:?}", partner.material) };
    assert!(text(cert).contains("certDir") && text(key).contains("PARTNER_KEY"), "rendered when the run starts");
}

#[test]
fn a_pkcs12_client_keeps_its_passphrase_or_none() {
    let program = program(CONFIG);
    let ClientMaterial::Pkcs12 { file, passphrase } = &client(&program, "tenantA").material else { panic!() };
    assert_eq!(text(file), text(&literal("certs/a.p12")));
    assert!(passphrase.as_ref().is_some_and(|passphrase| text(passphrase).contains("A_PASS")));
    let ClientMaterial::Pkcs12 { passphrase, .. } = &client(&program, "tenantB").material else { panic!() };
    assert_eq!(*passphrase, None, "the empty passphrase");
}

#[test]
fn an_auth_names_its_client_turns_it_off_or_leaves_it_to_the_host() {
    let program = program(CONFIG);
    let cert = |id: &str| program.auth.values().find(|def| program.str(def.id.sym()) == Some(id)).unwrap();
    let named = |def: &vc_model::AuthDef| match def.client_cert {
        Some(ClientCert::Named(id)) => program.str(id.sym()).map(str::to_owned),
        _ => None,
    };
    assert_eq!(named(cert("token")).as_deref(), Some("tenantA"));
    assert_eq!(named(cert("cert")).as_deref(), Some("tenantB"));
    assert_eq!(cert("off").client_cert, Some(ClientCert::Off));
    assert_eq!(cert("plain").client_cert, None);
    assert!(!cert("token").settings.contains_key("clientCert"), "the def holds it itself");
}

#[test]
fn a_config_without_tls_has_none() {
    let program = program("syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n");
    assert_eq!(program.config.http_tls, vc_model::TlsConfig::default());
}

#[test]
fn a_clients_profiles_written_as_one_word_is_a_shape_error() {
    let config = CONFIG.replace("profiles: [staging]", "profiles: staging");
    let compiled = super::config_tests::compiled(&config, SCHEMA);
    let found: Vec<_> = compiled.diagnostics.iter().map(|found| (found.code, found.primary.span.start.line)).collect();
    assert!(found.contains(&(vc_diagnostics::Code::V2302, 15)), "a list of profiles, not a word: {found:?}");
}
