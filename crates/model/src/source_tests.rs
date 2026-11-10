use super::{ContentHash, FileKind, SourcePath, hidden_part};

fn kind(path: &str) -> Option<FileKind> {
    FileKind::from_path(&SourcePath::new(path).unwrap())
}

#[test]
fn paths_use_forward_slashes_and_drop_leading_dot() {
    let path = SourcePath::new(r".\services\orders\checkout.test.yaml").unwrap();
    assert_eq!(path.as_str(), "services/orders/checkout.test.yaml");
    assert_eq!(path.file_name(), "checkout.test.yaml");
    assert_eq!(path.to_string(), "services/orders/checkout.test.yaml");
    assert_eq!(SourcePath::new("vibranium.config.yaml").unwrap().file_name(), "vibranium.config.yaml");
}

#[test]
fn paths_outside_the_project_are_rejected() {
    for bad in ["", "./", "/etc/passwd", r"C:\x.api.yaml", "../x.api.yaml", "a/../../b", "C:/x", "C:", "c:"] {
        assert!(SourcePath::new(bad).is_err(), "{bad:?} was accepted");
    }
    for bad in ["C:x.api.yaml", "C:..", "c:../x", "a/C:x"] {
        assert!(SourcePath::new(bad).is_err(), "{bad:?} could leave the project on Windows");
    }
}

#[test]
fn paths_collapse_empty_and_dot_parts() {
    for (text, expected) in [("a//b", "a/b"), ("a/./b", "a/b"), ("./a/.//b", "a/b"), (r"a\\b", "a/b")] {
        assert_eq!(SourcePath::new(text).unwrap().as_str(), expected, "{text:?}");
    }
}

#[test]
fn paths_naming_no_file_are_rejected() {
    for bad in [".", "./.", "//", "a/", "a/.", "a/./", "services/orders/", r"a\"] {
        assert!(SourcePath::new(bad).is_err(), "{bad:?} was accepted");
    }
}

#[test]
fn colons_are_rejected_in_every_part() {
    for bad in ["k:v.api.yaml", "services/k:v.api.yaml", "1:/x.api.yaml", "a.yaml:stream"] {
        assert!(SourcePath::new(bad).is_err(), "{bad:?} was accepted");
    }
}

#[test]
fn control_and_bidi_characters_are_rejected() {
    for hidden in ['\x1b', '\t', '\r', '\x7f', '\u{9b}', '\u{202e}', '\u{2066}', '\u{200e}', '\u{200f}', '\u{61c}'] {
        let bad = format!("services/x{hidden}.test.yaml");
        assert!(SourcePath::new(&bad).is_err(), "{bad:?} was accepted");
    }
}

#[test]
fn lossy_paths_replace_what_new_rejects_but_still_stay_inside() {
    let lossy = SourcePath::lossy("services/k:v\u{202e}.api.yaml").unwrap();
    assert_eq!(lossy.as_str(), "services/k\u{fffd}v\u{fffd}.api.yaml");
    assert_eq!(SourcePath::lossy("a/b.api.yaml").unwrap().as_str(), "a/b.api.yaml");
    for bad in ["/etc/x.api.yaml", "../x.api.yaml", "a/"] {
        assert!(SourcePath::lossy(bad).is_err(), "{bad:?} was accepted");
    }
}

#[test]
fn kind_comes_from_the_file_name() {
    assert_eq!(kind("vibranium.config.yaml"), Some(FileKind::Config));
    assert_eq!(kind("services/users/users.api.yaml"), Some(FileKind::Api));
    assert_eq!(kind("services/payments/refunds/partial.test.yaml"), Some(FileKind::Test));
    assert_eq!(kind("load/checkout.load.yaml"), Some(FileKind::Load));
}

#[test]
fn other_names_have_no_kind() {
    for other in ["schemas/user.json", "datasets/cities.csv", "hooks/sign.ts", ".api.yaml", "x.api.yml"] {
        assert_eq!(kind(other), None, "{other}");
    }
    assert_eq!(kind("services/vibranium.config.yaml.bak"), None);
    assert_eq!(kind("services/vibranium.config.yaml"), None, "only the root config is a config");
}

#[test]
fn every_kind_has_its_serde_name() {
    for kind in FileKind::ALL {
        assert_eq!(serde_json::to_value(kind).unwrap(), kind.name());
    }
}

#[test]
fn a_path_names_the_file_id_of_its_kind() {
    let id = |path: &str| SourcePath::new(path).unwrap().file_id();
    assert_eq!(id("services/users/users.api.yaml").as_deref(), Some("users"));
    assert_eq!(id("services/users/admin.api.yaml").as_deref(), Some("users.admin"));
    assert_eq!(id("services/users/users.test.yaml").as_deref(), Some("users.users"));
    assert_eq!(id("services/payments/refunds/partial.test.yaml").as_deref(), Some("payments.refunds.partial"));
    assert_eq!(id("load/checkout.load.yaml").as_deref(), Some("checkout"));
    assert_eq!(id("users.api.yaml").as_deref(), Some("users"));
    assert_eq!(id("vibranium.config.yaml"), None);
    assert_eq!(id("datasets/cities.csv"), None);
}

#[test]
fn data_paths_may_climb_out_of_the_project_but_nothing_else() {
    let data = |text: &str| SourcePath::data(text).map(|path| (path.to_string(), path.is_outside()));
    assert_eq!(data(r"..\shared\cities.csv").unwrap(), ("../shared/cities.csv".to_owned(), true));
    assert_eq!(data("datasets/x/../../../a/./b.csv").unwrap(), ("../a/b.csv".to_owned(), true));
    assert_eq!(data("datasets//cities.csv").unwrap(), ("datasets/cities.csv".to_owned(), false));
    for bad in ["", "..", "../", "a/..", "/etc/x.csv", "../a/", "../C:/x.csv", "../a\u{202e}.csv", "a/."] {
        assert!(SourcePath::data(bad).is_err(), "{bad:?} was accepted");
    }
    assert_eq!(FileKind::from_path(&SourcePath::data("../shared/users.api.yaml").unwrap()), None);
}

#[test]
fn hash_keeps_its_bytes() {
    assert_eq!(ContentHash::new([7; 32]).as_bytes(), &[7; 32]);
}

#[test]
fn a_part_starting_with_a_dot_is_hidden_whichever_slash_splits_it() {
    for hidden in [".env", "a/.git/x", "..", "../x", r"a\.env", r"a\b\.x", ".vibranium/cache.api.yaml"] {
        assert!(hidden_part(hidden), "{hidden:?} is hidden");
    }
    for shown in ["a.test.yaml", "services/users/users.api.yaml", r"a\b.c", "vibranium.config.yaml", ""] {
        assert!(!hidden_part(shown), "{shown:?} is not hidden");
    }
    assert!(SourcePath::data("../shared/cities.csv").unwrap().is_hidden());
    assert!(SourcePath::new("services/.git/a.api.yaml").unwrap().is_hidden());
    assert!(!SourcePath::new("services/users/crud.test.yaml").unwrap().is_hidden());
}
