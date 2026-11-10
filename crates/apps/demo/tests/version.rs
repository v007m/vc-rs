//! The `vc-demo` binary itself. Having an integration test also makes `cargo test` build the
//! binary next to `vc`, where `vc demo` looks for it.

use std::process::Command;

#[test]
fn prints_its_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_vc-demo")).arg("--version").output().unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), format!("vc-demo {}\n", env!("CARGO_PKG_VERSION")));
}
