use std::fs;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::str::contains;

const KINDS: [&str; 4] = ["config", "api", "test", "load"];

#[test]
fn prints_every_schema_as_one_object() {
    let dir = tempfile::tempdir().unwrap();
    let output = cargo_bin_cmd!("vc").current_dir(dir.path()).args(["schema", "export"]).assert().success();
    let text = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    assert!(text.starts_with('{') && text.ends_with("}\n"), "{text}");
    for kind in KINDS {
        assert!(text.contains(&format!("\"{kind}\": {{")), "{kind} missing from {text}");
        assert!(text.contains(&format!("schema/{kind}.schema.json")), "{kind} $id missing");
    }
}

#[test]
fn writes_one_file_per_kind_with_out() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("schema");
    let mut vc = cargo_bin_cmd!("vc");
    vc.args(["schema", "export", "--out"]).arg(&out).assert().success().stdout("").stderr(contains("wrote 4 schemas"));
    for kind in KINDS {
        let text = fs::read_to_string(out.join(format!("{kind}.schema.json"))).unwrap();
        assert!(text.contains("\"$schema\"") && text.ends_with("}\n"), "{text}");
    }
}

#[test]
fn refuses_an_out_folder_it_cannot_create() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("taken");
    fs::write(&file, b"").unwrap();
    let mut vc = cargo_bin_cmd!("vc");
    vc.args(["schema", "export", "--out"]).arg(&file).assert().code(2).stderr(contains("error: "));
}
