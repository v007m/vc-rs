//! An `npm:` source: `npm pack` downloads the package's tarball into the staging folder, `tar`
//! unpacks it there, and npm's own report names the version and integrity.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;

/// How npm starts each line of its error output, from npm 10 and before it.
const NPM_PREFIXES: [&str; 2] = ["npm error ", "npm ERR! "];

/// The starts of npm error lines that say where, not why: the system call, the error number and
/// the pointer to its log.
const NOT_A_REASON: [&str; 3] = ["syscall ", "errno ", "A complete log"];

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
    let packed = packed(&run(&mut pack, "npm", npm_reason)?.stdout)?;
    let tarball = staged.join(&packed.filename);
    let mut unpack = Command::new("tar");
    unpack.arg("-xzf").arg(&tarball).arg("-C").arg(staged).args(["--strip-components", "1"]);
    run(&mut unpack, "tar", last_line)?;
    fs::remove_file(&tarball).map_err(|error| format!("{}: {error}", packed.filename))?;
    Ok((format!("{}@{}", packed.name, packed.version), packed.integrity))
}

/// Runs `command`; a failure is why, as `reason` reads it from the output.
fn run(command: &mut Command, program: &str, reason: fn(&Output) -> String) -> Result<Output, String> {
    let output = command.output().map_err(|error| format!("cannot run `{program}`: {error}"))?;
    if output.status.success() {
        return Ok(output);
    }
    Err(format!("`{program}` failed: {}", reason(&output)))
}

/// The last line of a failed program's error output.
fn last_line(output: &Output) -> String {
    let errors = String::from_utf8_lossy(&output.stderr);
    errors.lines().rev().find(|line| !line.trim().is_empty()).unwrap_or("no output").trim().to_owned()
}

/// Why npm failed, with its error code: from the `error` of its JSON report, else its first
/// `npm error` line that says why (its last points at its log), else its last line.
fn npm_reason(output: &Output) -> String {
    reported(&output.stdout)
        .or_else(|| logged(&String::from_utf8_lossy(&output.stderr)))
        .unwrap_or_else(|| last_line(output))
}

/// `E404: Not Found - GET …` from the `error` of npm's `--json` report.
fn reported(stdout: &[u8]) -> Option<String> {
    let report: Value = serde_json::from_slice(stdout).ok()?;
    let error = report.get("error")?;
    let summary = error.get("summary").and_then(Value::as_str)?.lines().find(|line| !line.trim().is_empty())?;
    Some(coded(error.get("code").and_then(Value::as_str), summary.trim()))
}

/// `E404: 404 Not Found - GET …` from npm's `npm error` lines: the first that says why.
fn logged(stderr: &str) -> Option<String> {
    let lines: Vec<&str> = stderr
        .lines()
        .filter_map(|line| NPM_PREFIXES.iter().find_map(|prefix| line.strip_prefix(prefix)))
        .map(str::trim)
        .collect();
    let code = lines.iter().find_map(|line| line.strip_prefix("code "));
    let says_why = |line: &&&str| {
        line.contains(' ') && !line.starts_with("code ") && !NOT_A_REASON.iter().any(|start| line.starts_with(start))
    };
    lines.iter().find(says_why).map(|reason| coded(code, reason))
}

/// `reason`, after npm's error `code` when it does not name it already.
fn coded(code: Option<&str>, reason: &str) -> String {
    match code {
        Some(code) if !reason.contains(code) => format!("{code}: {reason}"),
        _ => reason.to_owned(),
    }
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
    use super::{logged, packed, reported};

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

    #[test]
    fn npm_s_reason_is_its_code_and_first_message_not_its_log_pointer() {
        let json = br#"{"error":{"code":"E404","summary":"Not Found - GET https://registry.npmjs.org/x","detail":""}}"#;
        assert_eq!(reported(json).as_deref(), Some("E404: Not Found - GET https://registry.npmjs.org/x"));
        assert_eq!(reported(b"[]"), None);
        let network = concat!(
            "npm ERR! code ENOTFOUND\nnpm ERR! syscall getaddrinfo\nnpm ERR! errno ENOTFOUND\n",
            "npm ERR! network request to https://registry.npmjs.org/x failed, reason: getaddrinfo ENOTFOUND\n",
            "npm ERR! A complete log of this run can be found in: /home/a/.npm/_logs/debug-0.log\n",
        );
        let reason = "network request to https://registry.npmjs.org/x failed, reason: getaddrinfo ENOTFOUND";
        assert_eq!(logged(network).as_deref(), Some(reason));
        assert_eq!(logged("tar: Error opening archive\n"), None);
    }
}
