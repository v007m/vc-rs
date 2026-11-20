use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use vc_diagnostics::{Code, Diagnostic};
use vc_interfaces::{
    Environment, MaterialSource, SourceProvider, TlsError, TlsFinding, TlsPart, TlsProbe, TlsSpec, Transport,
    TransportFactory,
};
use vc_model::Date;
use vc_testkit::{FakeVcs, FixedClock, MemoryCompileCache, MemoryEnvironment, MemorySources};

use super::{Dated, dated, shell_word};
use crate::{CheckRequest, Engine, EngineBuilder};

/// The day every check here runs on.
const TODAY: (u16, u8, u8) = (2026, 11, 5);

/// [`TODAY`] as days since 1970.
const TODAY_DAYS: i64 = 20_762;

/// A factory that finds what it is given, and says what each inspect was asked about.
struct Scripted {
    findings: Vec<TlsFinding>,
    asked: Sender<Vec<String>>,
}

impl TransportFactory for Scripted {
    fn build(&self, _tls: &TlsSpec, _environment: &dyn Environment) -> Result<Arc<dyn Transport>, TlsError> {
        Err(TlsError::Transport { reason: "a check builds no transport".to_owned() })
    }

    fn inspect(&self, probes: &[TlsProbe]) -> Vec<TlsFinding> {
        self.asked.send(probes.iter().map(described).collect()).unwrap();
        self.findings.clone()
    }
}

/// `probe` as a line: its part, and each source as `file <path>`, `inline <text>` or `-`.
fn described(probe: &TlsProbe) -> String {
    match probe {
        TlsProbe::Ca { index, source } => format!("ca {index} {}", material(Some(source))),
        TlsProbe::Pem { id, cert, key } => format!("pem {id} {} {}", material(cert.as_ref()), material(key.as_ref())),
        TlsProbe::Pkcs12 { id, bundle, passphrase } => {
            let passphrase = passphrase.as_ref().map_or("-", |passphrase| passphrase.expose());
            format!("p12 {id} {} pass={passphrase}", material(Some(bundle)))
        }
    }
}

fn material(source: Option<&MaterialSource>) -> String {
    match source {
        None => "-".to_owned(),
        Some(MaterialSource::Inline(text)) => format!("inline {}", text.expose().lines().next().unwrap_or("")),
        Some(MaterialSource::File { path, shown }) => {
            let shown = shown.as_deref().unwrap();
            assert_eq!(path, &Path::new("/project").join(shown), "the path is from the project root");
            format!("file {shown}")
        }
    }
}

/// The config with `tls` under `http.tls`.
fn config(tls: &str) -> String {
    format!("syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n  - id: staging\nhttp:\n  tls:\n{tls}")
}

/// A diagnostic and the text it underlines.
type Found = (Diagnostic, String);

/// `vc check` of a project whose config holds `tls`, with a factory finding `findings`: what it
/// reported from the factory, the text each underlines, and what the factory was asked about.
fn check(tls: &str, findings: Vec<TlsFinding>) -> (Vec<Found>, Receiver<Vec<String>>) {
    let config = config(tls);
    let (engine, told) = engine(&config, findings);
    let report = engine.check(&CheckRequest::default()).unwrap();
    let ours = [Code::V2011, Code::V2012, Code::V3107, Code::V3108, Code::V4012];
    let found = report.diagnostics.iter().filter(|found| ours.contains(&found.code));
    let span = |found: &Diagnostic| config[found.primary.span.start.offset..found.primary.span.end.offset].to_owned();
    (found.map(|found| (found.clone(), span(found))).collect(), told)
}

/// An engine on a project of `config` and an api file, with a factory finding `findings`.
fn engine(config: &str, findings: Vec<TlsFinding>) -> (Engine, Receiver<Vec<String>>) {
    let files = [
        ("vibranium.config.yaml", config.as_bytes()),
        (API, b"baseUrl: http://x\napis:\n  - id: list\n    method: GET\n    path: /users\n".as_slice()),
    ];
    let sources: Arc<dyn SourceProvider> = Arc::new(MemorySources::new(&files));
    let (asked, told) = channel();
    let env = [("CA", "x"), ("KEY", "x"), ("P12", "x"), ("PASS", "x")];
    let engine = EngineBuilder::new(PathBuf::from("/project"))
        .sources(sources)
        .compile_cache(Arc::new(MemoryCompileCache::new()))
        .clock(Arc::new(FixedClock(Date::new(TODAY.0, TODAY.1, TODAY.2).unwrap())))
        .environment(Arc::new(MemoryEnvironment::new(&env)))
        .vcs(Arc::new(FakeVcs::default()))
        .transport_factory(Arc::new(Scripted { findings, asked }))
        .build()
        .unwrap();
    (engine, told)
}

/// The api file next to the config.
const API: &str = "services/users/users.api.yaml";

/// The one diagnostic a check of `tls` gives for `finding`, and the text it underlines.
fn one(tls: &str, finding: TlsFinding) -> Found {
    let (mut found, _) = check(tls, vec![finding]);
    assert_eq!(found.len(), 1, "{found:?}");
    found.remove(0)
}

fn client(id: &str, field: &'static str) -> TlsPart {
    TlsPart::Client { id: id.to_owned(), field: Some(field) }
}

fn unreadable(part: TlsPart, near: &[&str]) -> TlsFinding {
    let near = near.iter().map(|&name| name.to_owned()).collect();
    TlsFinding::Unreadable { part, reason: "No such file or directory (os error 2)".to_owned(), near }
}

/// A finding that `good`'s certificate is valid from `from` until `until`, as days from today.
fn valid(from: i64, until: i64) -> TlsFinding {
    TlsFinding::Validity { part: client("good", "cert"), not_before: at(from, 43_200), not_after: at(until, 43_200) }
}

const GOOD: &str = "    clients:\n      - id: good\n        cert: certs/god.pem\n        key: certs/good.key\n";

const PEM: &str = "-----BEGIN CERTIFICATE-----";

#[test]
fn probes_each_value_written_as_text_under_every_profile() {
    let tls = format!(
        "    ca:\n      - certs/ca.pem\n      - ${{env.CA}}\n      - |\n        {PEM}\n        AA==\n        -----END CERTIFICATE-----\n\
         \x20   clients:\n\
         \x20     - id: good\n        cert: ' certs/good.pem '\n        key: ${{env.KEY}}\n\
         \x20     - id: stage\n        profiles: [staging]\n        cert: certs/stage.pem\n        key: certs/stage.key\n\
         \x20     - id: rendered\n        cert: ${{env.KEY}}\n        key: ${{env.KEY}}\n\
         \x20     - id: bank\n        pkcs12: certs/bank.p12\n        passphrase: fixture\n\
         \x20     - id: open\n        pkcs12: certs/open.p12\n\
         \x20     - id: hidden\n        pkcs12: certs/hidden.p12\n        passphrase: ${{env.PASS}}\n\
         \x20     - id: elsewhere\n        pkcs12: ${{env.P12}}\n        passphrase: fixture\n"
    );
    let (found, told) = check(&tls, Vec::new());
    assert!(found.is_empty(), "{found:?}");
    let asked = told.try_recv().unwrap();
    let inline = format!("ca 2 inline {PEM}");
    let expected = [
        "ca 0 file certs/ca.pem",
        inline.as_str(),
        "pem good file certs/good.pem -",
        "pem stage file certs/stage.pem file certs/stage.key",
        "p12 bank file certs/bank.p12 pass=fixture",
        "p12 open file certs/open.p12 pass=",
        "p12 hidden file certs/hidden.p12 pass=-",
    ];
    assert_eq!(asked, expected);
    assert!(told.try_recv().is_err(), "one inspect a check");
}

#[test]
fn asks_nothing_when_no_value_is_written_as_text() {
    let tls =
        "    ca: ${env.CA}\n    clients:\n      - id: rendered\n        cert: ${env.KEY}\n        key: ${env.KEY}\n";
    let (found, told) = check(tls, vec![unreadable(TlsPart::Ca(0), &[])]);
    assert!(found.is_empty() && told.try_recv().is_err(), "{found:?}");
}

#[test]
fn offers_the_one_close_name_in_the_folder_as_a_certain_fix() {
    let (found, span) = one(GOOD, unreadable(client("good", "cert"), &["good.pem", "ca.pem", "good.key"]));
    assert_eq!(
        (found.code, span.as_str(), found.primary.text.as_str()),
        (Code::V2011, "certs/god.pem", "cannot be read")
    );
    assert_eq!(
        found.message,
        "client certificate `good`, its `cert`: `certs/god.pem` cannot be read: No such file or directory (os error 2)"
    );
    assert_eq!(found.help, ["did you mean `certs/good.pem`?"]);
    let [fix] = found.fixes.as_slice() else { panic!("{:?}", found.fixes) };
    assert!(fix.certain && fix.title == "Replace with certs/good.pem", "{fix:?}");
    let [edit] = fix.edits.as_slice() else { panic!("{:?}", fix.edits) };
    assert_eq!(
        (edit.range.clone(), edit.new_text.as_str()),
        (found.primary.span.start.offset..found.primary.span.end.offset, "certs/good.pem")
    );
}

#[test]
fn only_suggests_a_name_for_a_quoted_path_or_one_of_several() {
    let quoted = GOOD.replace("certs/god.pem", "'certs/god.pem'");
    let (found, _) = one(&quoted, unreadable(client("good", "cert"), &["good.pem"]));
    assert!(found.fixes.is_empty() && found.help == ["did you mean `certs/good.pem`?"], "{found:?}");
    let (found, _) = one(GOOD, unreadable(client("good", "cert"), &["good.pem", "gods.pem"]));
    assert!(
        found.fixes.is_empty() && found.help == ["did you mean `certs/gods.pem` or `certs/good.pem`?"],
        "{found:?}"
    );
    let (found, _) = one(GOOD, unreadable(client("good", "cert"), &["ca.pem"]));
    assert!(found.fixes.is_empty() && found.help == ["paths are relative to the project root"], "{found:?}");
}

#[test]
fn underlines_the_ca_entry_a_finding_names() {
    let (found, span) = one("    ca: certs/ca.pem\n", unreadable(TlsPart::Ca(0), &[]));
    assert_eq!((found.code, span.as_str()), (Code::V2011, "certs/ca.pem"));
    let list = "    ca:\n      - certs/one.pem\n      - certs/two.pem\n";
    let (found, span) = one(list, unreadable(TlsPart::Ca(1), &[]));
    assert_eq!((span.as_str(), found.message.split(':').next()), ("certs/two.pem", Some("`ca` certificate 2")));
    let (found, span) = one("    ca: certs/ca.pem\n", unreadable(TlsPart::Ca(5), &[]));
    assert_eq!((found.code, span.as_str()), (Code::V2011, "certs/ca.pem"), "no entry 5: the whole `ca`");
    let (_, span) = one(list, unreadable(TlsPart::Ca(5), &[]));
    assert!(span.contains("certs/one.pem") && span.contains("certs/two.pem"), "the whole list: {span}");
    let (_, span) = one(GOOD, unreadable(TlsPart::Ca(0), &[]));
    assert!(span.starts_with("clients:"), "no `ca`: all of `http.tls`: {span}");
}

#[test]
fn fixes_a_name_in_a_flow_list_in_place() {
    let (found, span) = one("    ca: [ certs/ca.pm ]\n", unreadable(TlsPart::Ca(0), &["ca.pem"]));
    assert_eq!(span, "certs/ca.pm", "the name alone, without the list's padding");
    let [fix] = found.fixes.as_slice() else { panic!("{:?}", found.fixes) };
    assert_eq!(fix.edits[0].new_text, "certs/ca.pem");
}

#[test]
fn reports_material_that_does_not_load_or_fit_as_v2012() {
    let reason = "it holds no PEM certificate".to_owned();
    let (found, span) = one(GOOD, TlsFinding::Unusable { part: client("good", "cert"), reason });
    assert_eq!(
        (found.code, span.as_str(), found.primary.text.as_str()),
        (Code::V2012, "certs/god.pem", "does not load")
    );
    assert_eq!(found.message, "client certificate `good`, its `cert` does not load: it holds no PEM certificate");
    assert!(found.help[0].starts_with("give a PEM certificate"), "{:?}", found.help);
    let part = TlsPart::Client { id: "good".to_owned(), field: None };
    let (found, span) = one(GOOD, TlsFinding::Mismatch { part, reason: "the key is not the certificate's".to_owned() });
    assert_eq!((found.code, found.primary.text.as_str()), (Code::V2012, "does not fit"));
    assert!(span.starts_with("id: good") && span.contains("key: certs/good.key"), "the whole entry: {span}");
    assert_eq!(found.help, ["use the key issued with the certificate, or the bundle's own passphrase"]);
}

#[test]
fn warns_of_a_key_others_can_read_with_the_command_that_fixes_it() {
    let (found, span) = one(GOOD, TlsFinding::Exposed { part: client("good", "key"), mode: 0o644 });
    assert_eq!(
        (found.code, span.as_str(), found.primary.text.as_str()),
        (Code::V4012, "certs/good.key", "open to other users")
    );
    assert_eq!(
        found.message,
        "client certificate `good`, its `key`: `certs/good.key` is open to other users (mode 644)"
    );
    assert_eq!(found.help, ["make it readable by you alone, from the project root: `chmod 600 certs/good.key`"]);
    assert!(found.fixes.is_empty(), "a mode is not text to edit");
    let (found, _) = one(GOOD, TlsFinding::Exposed { part: client("good", "key"), mode: 0o40 });
    assert!(found.message.ends_with("(mode 040)"), "{}", found.message);
}

#[test]
fn quotes_a_path_the_shell_would_split() {
    assert_eq!(shell_word("certs/good-1_a.key"), "certs/good-1_a.key");
    assert_eq!(shell_word("my certs/a.key"), "'my certs/a.key'");
    assert_eq!(shell_word("it's.key"), r"'it'\''s.key'");
    assert_eq!(shell_word("a$b"), "'a$b'");
    assert_eq!(shell_word(""), "''");
}

fn today() -> Date {
    Date::new(TODAY.0, TODAY.1, TODAY.2).unwrap()
}

/// The day `days` from today.
fn day(days: i64) -> Date {
    Date::from_unix_days(TODAY_DAYS + days).unwrap()
}

/// `second` into the day `days` from today, in seconds since 1970.
const fn at(days: i64, second: i64) -> i64 {
    (TODAY_DAYS + days) * 86_400 + second
}

#[test]
fn dates_the_end_of_a_certificate_by_the_utc_day_against_today() {
    assert_eq!(day(0), today());
    assert_eq!(dated((at(-400, 0), at(0, 0)), today()), Some(Dated::LastDay(today())), "ends today, at midnight");
    assert_eq!(dated((at(-400, 0), at(0, 86_399)), today()), Some(Dated::LastDay(today())), "ends late today");
    assert_eq!(dated((at(-400, 0), at(1, 0)), today()), Some(Dated::Soon(day(1))), "ends tomorrow");
    assert_eq!(dated((at(-400, 0), at(-1, 86_399)), today()), Some(Dated::Expired(day(-1))), "ended yesterday");
    assert_eq!(dated((at(-400, 0), at(30, 86_399)), today()), Some(Dated::Soon(day(30))));
    assert_eq!(dated((at(-400, 0), at(31, 0)), today()), None);
    assert_eq!(dated((at(-400, 0), -1), today()), Date::from_unix_days(-1).map(Dated::Expired), "before 1970");
}

#[test]
fn dates_the_start_of_a_certificate_by_the_utc_day_against_today() {
    assert_eq!(dated((at(0, 86_399), at(400, 0)), today()), None, "valid from late today");
    assert_eq!(dated((at(1, 0), at(400, 0)), today()), Some(Dated::NotYet(day(1))));
    assert_eq!(dated((at(1, 0), at(20, 0)), today()), Some(Dated::NotYet(day(1))), "not valid yet comes first");
    assert_eq!(dated((-86_400 * 3 - 1, at(400, 0)), today()), None, "a start before 1970 is its own day");
}

#[test]
fn reports_a_certificate_expiring_soon_or_not_valid_today() {
    let (found, span) = one(GOOD, valid(-400, 30));
    assert_eq!(
        (found.code, span.as_str(), found.primary.text.as_str()),
        (Code::V3107, "certs/god.pem", "expires soon")
    );
    assert_eq!(found.message, "client certificate `good`, its `cert` expires on 2026-12-05, within 30 days");
    assert!(found.help[0].starts_with("renew it before then"), "{:?}", found.help);
    let (found, _) = one(GOOD, valid(-400, -1));
    assert_eq!((found.code, found.primary.text.as_str()), (Code::V3108, "expired"));
    assert_eq!(found.message, "client certificate `good`, its `cert` expired on 2026-11-04");
    let (found, _) = one(GOOD, valid(1, 400));
    assert_eq!((found.code, found.primary.text.as_str()), (Code::V3108, "not valid yet"));
    assert_eq!(found.message, "client certificate `good`, its `cert` is not valid until 2026-11-06");
    let (found, _) = check(GOOD, vec![valid(-400, 31)]);
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn says_a_certificate_on_its_last_day_expires_today() {
    let (found, _) = one(GOOD, valid(-400, 0));
    assert_eq!((found.code, found.primary.text.as_str()), (Code::V3107, "expires today"));
    assert_eq!(found.message, "client certificate `good`, its `cert` expires today, 2026-11-05");
    assert!(found.help[0].starts_with("renew it now"), "{:?}", found.help);
}

#[test]
fn reports_every_finding_and_one_for_a_client_not_written_out_on_http_tls() {
    let findings = vec![
        unreadable(client("good", "cert"), &[]),
        TlsFinding::Exposed { part: client("good", "key"), mode: 0o644 },
        unreadable(client("gone", "cert"), &[]),
    ];
    let (found, _) = check(GOOD, findings);
    let codes: Vec<Code> = found.iter().map(|(found, _)| found.code).collect();
    assert_eq!(codes, [Code::V2011, Code::V2011, Code::V4012], "in the order of their place");
    assert!(found[0].1.starts_with("clients:"), "`gone` on all of `http.tls`: {:?}", found[0]);
    let part = TlsPart::Client { id: "good".to_owned(), field: Some("pkcs12") };
    let (_, span) = one(GOOD, unreadable(part, &[]));
    assert!(span.starts_with("id: good"), "no `pkcs12`: the entry: {span}");
}

#[test]
fn reads_nothing_for_a_check_of_paths_that_leave_out_the_config() {
    let (engine, told) = engine(&config(GOOD), vec![unreadable(client("good", "cert"), &[])]);
    let paths = |paths: &[&str]| CheckRequest {
        paths: paths.iter().map(|&path| path.to_owned()).collect(),
        ..CheckRequest::default()
    };
    let report = engine.check(&paths(&["services"])).unwrap();
    assert!(report.diagnostics.iter().all(|found| found.code != Code::V2011), "{:?}", report.diagnostics);
    assert!(told.try_recv().is_err(), "no file read for a check of `services`");
    let report = engine.check(&paths(&["vibranium.config.yaml", "services"])).unwrap();
    assert!(report.diagnostics.iter().any(|found| found.code == Code::V2011), "{:?}", report.diagnostics);
    assert!(told.try_recv().is_ok());
    let fixes = engine.fix(&paths(&["services"])).unwrap();
    assert!(told.try_recv().is_err() && fixes.changes.is_empty(), "a fix of `services` reads none either");
}

#[test]
fn a_fix_gathers_the_close_name_for_a_file_that_cannot_be_read() {
    let (engine, told) = engine(&config(GOOD), vec![unreadable(client("good", "cert"), &["good.pem"])]);
    let fixes = engine.fix(&CheckRequest::default()).unwrap();
    assert!(told.try_recv().is_ok(), "the fix reads the files as the check does");
    assert_eq!(fixes.changes.titles, ["Replace with certs/good.pem"]);
    let [change] = fixes.changes.files.as_slice() else { panic!("{:?}", fixes.changes.files) };
    assert!(change.after.contains("        cert: certs/good.pem\n"), "{}", change.after);
    assert!(fixes.report.diagnostics.iter().any(|found| found.code == Code::V2011), "the check as it is now");
}
