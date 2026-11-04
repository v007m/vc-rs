//! Every V1–V3 code has an explain page, `crates/diagnostics/explain/<code>.md`, and its
//! examples do what the page says.
//!
//! A page starts with `# <code>: <title>` (the code's title), then one paragraph, then a
//! `## Wrong` and a `## Right` section. Each section is a small project: one fenced block per
//! file, whose first line is `# file: <path>` (relative to the project root; the line itself is
//! not part of the file). A project without a `vibranium.config.yaml` gets one with a default
//! profile. The wrong project must report the code; the right one must report neither the code
//! nor any error. Both are checked through the engine on 2026-11-03, so V1012 (a file discovery
//! cannot read) and V3104 (an expired skip) can be shown too.

mod support;

use std::fs;
use std::path::PathBuf;

use support::Outcome;
use vc_diagnostics::{Code, Diagnostics, Severity};

/// The languages an example block may be written in.
const LANGUAGES: [&str; 2] = ["yaml", "json"];

/// V2005 (hook export not found) is linked with hooks in M4; its page lands then. V2106 and
/// V2107 need `.env.schema`, which reaches the compiler as `CompileInput::secret_schema` once
/// the engine reads it from the project root; their pages land with that wiring.
const LATER: [Code; 3] = [Code::V2005, Code::V2106, Code::V2107];

fn page_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../diagnostics/explain")
}

fn page(code: Code) -> Outcome<String> {
    Ok(fs::read_to_string(page_dir().join(format!("{code}.md")))?)
}

fn explained() -> impl Iterator<Item = Code> {
    Code::ALL.iter().copied().filter(|code| code.number() < 4000 && !LATER.contains(code))
}

/// The files of the section headed `## <section>`.
fn project(page: &str, section: &str) -> Outcome<Vec<(String, String)>> {
    let blocks = support::blocks(page).into_iter().filter(|(heading, _, _)| heading == section);
    let file = |(_, _, block): (String, String, support::Block)| -> Outcome<(String, String)> {
        if !LANGUAGES.contains(&block.info.as_str()) {
            return Err(format!("a block in {section} is `{}`", block.info).into());
        }
        let path = block
            .first
            .strip_prefix("# file: ")
            .ok_or_else(|| format!("a block in {section} has no `# file:` line"))?;
        Ok((path.to_owned(), block.body))
    };
    blocks.map(file).collect()
}

fn codes(diagnostics: &Diagnostics) -> Vec<Code> {
    diagnostics.iter().map(|diagnostic| diagnostic.code).collect()
}

#[test]
fn every_v1_to_v3_code_has_a_page_and_every_page_a_code() -> Outcome<()> {
    let entries = fs::read_dir(page_dir())?.collect::<Result<Vec<_>, _>>()?;
    let mut pages: Vec<String> = entries.iter().map(|entry| entry.file_name().to_string_lossy().into_owned()).collect();
    pages.sort();
    let expected: Vec<String> = explained().map(|code| format!("{code}.md")).collect();
    assert_eq!(pages, expected);
    Ok(())
}

#[test]
fn each_page_is_titled_with_its_code() -> Outcome<()> {
    for code in explained() {
        assert_eq!(page(code)?.lines().next(), Some(format!("# {code}: {}", code.title()).as_str()));
    }
    Ok(())
}

/// What is wrong with `code`'s examples, if anything.
fn misfit(code: Code) -> Outcome<Option<String>> {
    let page = page(code)?;
    let wrong = support::check(&project(&page, "Wrong")?)?.diagnostics;
    if !codes(&wrong).contains(&code) {
        return Ok(Some(format!("{code}: the wrong example reports {:?}", codes(&wrong))));
    }
    let right = support::check(&project(&page, "Right")?)?.diagnostics;
    let bad = right.iter().any(|found| found.code == code || found.severity == Severity::Error);
    Ok(bad.then(|| format!("{code}: the right example reports {:?}", codes(&right))))
}

#[test]
fn each_wrong_example_reports_its_code_and_each_right_example_does_not() -> Outcome<()> {
    let misfits: Vec<String> = explained().map(misfit).collect::<Outcome<Vec<_>>>()?.into_iter().flatten().collect();
    assert!(misfits.is_empty(), "{misfits:#?}");
    Ok(())
}
