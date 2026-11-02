use assert_cmd::{Command, cargo::cargo_bin_cmd};

fn vc() -> Command {
    cargo_bin_cmd!("vc")
}

#[test]
fn prints_its_version() {
    let expected = format!("vc {}\n", env!("CARGO_PKG_VERSION"));
    vc().arg("--version").assert().success().stdout(expected);
}

#[test]
fn shows_usage_and_exits_2_without_arguments() {
    vc().assert().code(2).stderr(predicates::str::contains("Usage: vc"));
}
