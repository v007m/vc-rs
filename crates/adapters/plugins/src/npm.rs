//! An `npm:` source: `npm pack` downloads the package's tarball into the staging folder, `tar`
//! unpacks it there, and npm's own report names the version and integrity.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;

/// What `npm pack --json` reports about the tarball it wrote.
struct Packed {
    name: String,
    version: String,
    filename: String,
    integrity: String,
}

/// Packs `spec` with the program `npm` into `staged`, unpacks it there and returns what it
/// resolved to (`name@version`) and npm's integrity.
pub fn fetch(npm: &Path, spec: &str, staged: &Path) -> Result<(String, String), String> {
    if spec.is_empty() || spec.starts_with('-') {
        return Err("an npm source names a package: `npm:<package>[@<range>]`".to_owned());
    }
    let mut pack = Command::new(npm);
    pack.args(["pack", spec, "--json", "--pack-destination"]).arg(staged).current_dir(staged);
    let packed = packed(&run(&mut pack, "npm")?.stdout)?;
    let tarball = staged.join(&packed.filename);
    let mut unpack = Command::new("tar");
    unpack.arg("-xzf").arg(&tarball).arg("-C").arg(staged).args(["--strip-components", "1"]);
    run(&mut unpack, "tar")?;
    fs::remove_file(&tarball).map_err(|error| format!("{}: {error}", packed.filename))?;
    Ok((format!("{}@{}", packed.name, packed.version), packed.integrity))
}

/// Runs `command`; a failure is its last line of error output.
fn run(command: &mut Command, program: &str) -> Result<Output, String> {
    let output = command.output().map_err(|error| format!("cannot run `{program}`: {error}"))?;
    if output.status.success() {
        return Ok(output);
    }
    let errors = String::from_utf8_lossy(&output.stderr);
    let last = errors.lines().rev().find(|line| !line.trim().is_empty()).unwrap_or("no output");
    Err(format!("`{program}` failed: {}", last.trim()))
}

/// The one tarball in `npm pack --json`'s output; its file name must be a plain name.
fn packed(stdout: &[u8]) -> Result<Packed, String> {
    let report: Value = serde_json::from_slice(stdout).map_err(|error| format!("npm's report: {error}"))?;
    let first = report.as_array().and_then(|tarballs| tarballs.first());
    let text = |key: &str| first.and_then(|tarball| tarball.get(key)).and_then(Value::as_str).map(str::to_owned);
    let (Some(name), Some(version), Some(filename), Some(integrity)) =
        (text("name"), text("version"), text("filename"), text("integrity"))
    else {
        return Err("npm's report has no name, version, filename and integrity".to_owned());
    };
    if Path::new(&filename).file_name().is_none_or(|plain| plain != filename.as_str()) {
        return Err(format!("npm named the tarball `{filename}`, not a plain file name"));
    }
    Ok(Packed { name, version, filename, integrity })
}

#[cfg(test)]
mod tests {
    use super::packed;

    #[test]
    fn reads_npm_s_report_and_refuses_a_tarball_outside_the_folder() {
        let report = concat!(
            r#"[{"name":"@acme/report","version":"2.1.0","#,
            r#""filename":"acme-report-2.1.0.tgz","integrity":"sha512-x"}]"#
        );
        let tarball = packed(report.as_bytes()).unwrap();
        assert_eq!((tarball.name.as_str(), tarball.version.as_str()), ("@acme/report", "2.1.0"));
        assert_eq!((tarball.filename.as_str(), tarball.integrity.as_str()), ("acme-report-2.1.0.tgz", "sha512-x"));
        let escaping = br#"[{"name":"a","version":"1","filename":"../a.tgz","integrity":"sha512-x"}]"#;
        assert!(packed(escaping).is_err_and(|error| error.contains("plain file name")));
        assert!(packed(b"[]").is_err());
        assert!(packed(b"not json").is_err());
    }
}
