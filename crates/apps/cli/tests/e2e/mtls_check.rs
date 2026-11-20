//! `vc check` reads the `http.tls` files the config writes as text (#105, D408–D411): a file that
//! cannot be read is V2011, with the close name in its folder as a certain fix; material that does
//! not load or whose key is another certificate's is V2012; a client certificate expiring within
//! 30 days is V3107 and one past its end V3108; and a key file others can read is V4012.

use std::time::{Duration, SystemTime};

use vc_testkit::{ClientCert, TestPki};

use crate::{Project, said};

/// A project whose config's `http.tls.clients` holds `clients`.
fn project(clients: &str) -> Project {
    let project = Project::new(&[]);
    let config =
        format!("syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nhttp:\n  tls:\n    clients:\n{clients}");
    project.write("vibranium.config.yaml", &config);
    project
}

/// `good`'s certificate and key in `certs/`, the key readable by its owner alone.
fn good(project: &Project, pki: &TestPki) {
    project.write("certs/good.pem", pki.good().cert_pem());
    project.write("certs/good.key", pki.good().key_pem());
    private(project, "certs/good.key");
}

/// The file at `path` readable by its owner alone, where files have modes.
fn private(project: &Project, path: &str) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let file = project.path().join(path);
        std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    #[cfg(not(unix))]
    let _ = (project, path);
}

/// The `clients` entry `id` with `cert` and `key`.
fn entry(id: &str, cert: &str, key: &str) -> String {
    format!("      - id: {id}\n        cert: {cert}\n        key: {key}\n")
}

/// What `vc check` said, and its exit code (2 on errors).
fn check(project: &Project) -> (String, Option<i32>) {
    let checked = project.vc(&["check"]).output().unwrap();
    (said(&checked), checked.status.code())
}

/// `days` days from now.
pub fn days(days: i64) -> SystemTime {
    let span = Duration::from_hours(24 * days.unsigned_abs());
    if days < 0 { SystemTime::now() - span } else { SystemTime::now() + span }
}

#[test]
fn check_names_the_close_file_for_a_path_it_cannot_read() {
    let pki = TestPki::new().unwrap();
    let project = project(&entry("good", "certs/god.pem", "certs/good.key"));
    good(&project, &pki);
    let (said, code) = check(&project);
    assert!(
        said.contains("error[V2011]: client certificate `good`, its `cert`: `certs/god.pem` cannot be read"),
        "{said}"
    );
    assert!(said.contains("did you mean `certs/good.pem`?"), "{said}");
    assert!(said.contains("fix: Replace with certs/good.pem (certain)"), "{said}");
    assert_eq!(code, Some(2), "{said}");
    let fixed = project.vc(&["check", "--fix", "--yes"]).output().unwrap();
    let config = std::fs::read_to_string(project.path().join("vibranium.config.yaml")).unwrap();
    assert!(config.contains("        cert: certs/good.pem\n"), "{config}");
    let (said, code) = (crate::said(&fixed), fixed.status.code());
    assert!(!said.contains("error[") && !said.contains("warning["), "{said}");
    assert_eq!(code, Some(0), "{said}");
}

#[test]
fn check_refuses_material_that_does_not_load_or_fit() {
    let pki = TestPki::new().unwrap();
    let other = pki.client(&ClientCert::new("other")).unwrap();
    let clients =
        entry("good", "certs/good.pem", "certs/other.key") + &entry("text", "certs/text.pem", "certs/good.key");
    let project = project(&clients);
    good(&project, &pki);
    project.write("certs/other.key", other.key_pem());
    private(&project, "certs/other.key");
    project.write("certs/text.pem", "not a certificate\n");
    let (said, code) = check(&project);
    assert!(said.contains("error[V2012]: client certificate `good`, its `key` does not fit:"), "{said}");
    assert!(said.contains("error[V2012]: client certificate `text`, its `cert` does not load:"), "{said}");
    assert!(!said.contains("BEGIN"), "no material is quoted: {said}");
    assert_eq!(code, Some(2), "{said}");
}

#[test]
fn check_warns_of_a_client_certificate_expiring_soon_or_expired() {
    let pki = TestPki::new().unwrap();
    let soon = pki.client(&ClientCert::new("soon").valid(days(-400), days(10))).unwrap();
    let old = pki.client(&ClientCert::new("old").valid(days(-400), days(-2))).unwrap();
    let clients = entry("soon", "certs/soon.pem", "certs/soon.key") + &entry("old", "certs/old.pem", "certs/old.key");
    let project = project(&clients);
    for (name, identity) in [("soon", &soon), ("old", &old)] {
        project.write(&format!("certs/{name}.pem"), identity.cert_pem());
        project.write(&format!("certs/{name}.key"), identity.key_pem());
        private(&project, &format!("certs/{name}.key"));
    }
    let (said, code) = check(&project);
    assert!(said.contains("warning[V3107]: client certificate `soon`, its `cert` expires on "), "{said}");
    assert!(said.contains(", within 30 days"), "{said}");
    assert!(said.contains("warning[V3108]: client certificate `old`, its `cert` expired on "), "{said}");
    assert_eq!(code, Some(0), "warnings only: {said}");
}

#[cfg(unix)]
#[test]
fn check_warns_of_a_key_others_can_read() {
    use std::os::unix::fs::PermissionsExt as _;
    let pki = TestPki::new().unwrap();
    let project = project(&entry("good", "certs/good.pem", "certs/good.key"));
    good(&project, &pki);
    let key = project.path().join("certs/good.key");
    std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o644)).unwrap();
    let (said, code) = check(&project);
    assert!(
        said.contains(
            "warning[V4012]: client certificate `good`, its `key`: `certs/good.key` is open to other users (mode 644)"
        ),
        "{said}"
    );
    assert!(said.contains("chmod 600 certs/good.key"), "{said}");
    assert_eq!(code, Some(0), "{said}");
}
