//! The YAML examples in `docs/design/*.md`, on the docs site (`site/src/content/docs/**/*.mdx`)
//! and in its example project (`site/src/examples`) are checked, so the docs cannot drift from
//! the code.
//!
//! **The example project** is checked as it stands, as one project: no errors at all (warnings
//! are fine).
//!
//! **A page** (a design doc or a site page) holds whole files and fragments. A block fenced as
//! `yaml` is a **whole file** when it names a definition file (`services/users/users.api.yaml`):
//! by its title (```` ```yaml title="<path>" ````), by a first line that is exactly `# <path>`, or
//! by being the first block under a `###` heading that is just the path in backticks (as in "What
//! `vc init` writes"). It is still an excerpt, and so a fragment, when its top level is a list or
//! indented (the entries of a file's list) or when a line of just `...` stands for lines left
//! out. A title marked `(excerpt)`, such as `vibranium.config.yaml (excerpt)`, makes its block a
//! fragment whatever its first line or the heading over it.
//!
//! A page's whole files are checked together as one project. Writing a path again (a walkthrough
//! growing a file, or a reference page showing one file several ways) starts a new version of the
//! project: the files so far are checked as they stood, and the next version keeps the others. A
//! config the page writes is checked with `syntaxVersion` and a default profile added when it
//! leaves them out, and a page with no config gets [`support::CONFIG`]. The project must have no
//! errors but those about names defined in files the page does not show ([`elsewhere`]), since a
//! page writes out the files its point needs, not a whole project:
//!
//! - V1012: a dataset (`datasets/cities.csv`) not written out;
//! - V2002: an auth from a config the page does not show;
//! - V2003: an api or test from a file the page does not show;
//! - V2004: a body, schema or fixture (`$file: ../../schemas/user.json`) not written out;
//! - V2101: a var set by such a config, or saved by such an api.
//!
//! A **fragment** only has to parse (no V1 error), with its `...` lines dropped.
//!
//! A block whose info string holds `skip` is left out. One that holds `wrong` (```` ```yaml wrong
//! title="…" ````; expressive-code ignores a bare word it does not know) is wrong on purpose, as
//! a diagnostic page's "Wrong" example is: it is kept out of the page's project, and must itself
//! have an error, checked with the page's files so far when it names its file and parsed when
//! it is a fragment, so an example that has become right cannot hide behind the word. An example
//! that passes `vc check` and fails only when sent is not marked.

mod support;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use support::{Block, Outcome};
use vc_compiler::CompileInput;
use vc_diagnostics::{Code, Diagnostic, Severity};
use vc_engine::SourceFiles;
use vc_model::{ContentHash, FileKind, SourceFile, SourcePath};

/// A project's files: their text by path.
type Files = BTreeMap<String, String>;

/// Which errors a project may have.
type Allowed = fn(&Diagnostic) -> bool;

/// A page's project versions and the problems found while reading it.
type Page = (Vec<Files>, Vec<String>);

/// Errors about names defined in files a page does not write out (see the top of this file).
const ELSEWHERE: [Code; 4] = [Code::V2002, Code::V2003, Code::V2004, Code::V2101];

/// The word in a fence's info string that leaves its block out.
const SKIP: &str = "skip";

/// The word in a fence's info string that marks its block wrong on purpose.
const WRONG: &str = "wrong";

/// The mark at the end of a title that makes its block a fragment.
const EXCERPT: &str = "(excerpt)";

/// The default profile a config gets when it writes none.
const PROFILES: &str = "profiles:\n  - id: dev\n    default: true\n";

/// The repository root.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The files under `dir`, at any depth, whose extension is `extension`, sorted.
fn files_under(dir: &Path, extension: &str) -> Outcome<Vec<PathBuf>> {
    let mut found = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            found.extend(files_under(&path, extension)?);
        } else if path.extension().is_some_and(|ext| ext == extension) {
            found.push(path);
        }
    }
    found.sort();
    Ok(found)
}

/// `text` when it is the path of a definition file.
fn definition(text: &str) -> Option<String> {
    let path = SourcePath::new(text).ok()?;
    FileKind::from_path(&path).map(|_| text.to_owned())
}

/// Whether the check reads `block`: fenced as `yaml`, without [`SKIP`].
fn checked(block: &Block) -> bool {
    let mut words = block.info.split_whitespace();
    words.next() == Some("yaml") && !words.any(|word| word == SKIP)
}

/// Whether `block` is marked [`WRONG`].
fn wrong(block: &Block) -> bool {
    block.info.split_whitespace().any(|word| word == WRONG)
}

/// The `title="…"` of a fence's info string.
fn title(info: &str) -> Option<&str> {
    let (_, rest) = info.split_once("title=\"")?;
    rest.split_once('"').map(|(title, _)| title)
}

/// The path a block names as its file, by its title, its first line or the heading over it; none
/// when its title is marked [`EXCERPT`].
fn named_path(heading: &str, block: &Block) -> Option<String> {
    let given = title(&block.info);
    if given.is_some_and(|title| title.ends_with(EXCERPT)) {
        return None;
    }
    let titled = given.and_then(definition);
    let first = block.first.strip_prefix("# ").and_then(definition);
    let headed = || heading.strip_prefix('`').and_then(|rest| rest.strip_suffix('`')).and_then(definition);
    titled.or(first).or_else(headed)
}

/// Whether `text` is an excerpt though it names its file: its top level is a list or indented,
/// or a line of just `...` stands for lines left out.
fn excerpt(text: &str) -> bool {
    let content = |line: &&str| !line.trim().is_empty() && !line.trim_start().starts_with('#');
    let first = text.lines().find(content);
    let listed = first.is_some_and(|line| line.starts_with([' ', '\t']) || line.starts_with("- "));
    listed || text.lines().any(|line| line.trim() == "...")
}

/// The V1 codes a fragment's text gets when parsed on its own, its `...` lines dropped.
fn parse_errors(text: &str) -> Outcome<Vec<Code>> {
    let text = text.lines().filter(|line| line.trim() != "...").fold(String::new(), |kept, line| kept + line + "\n");
    let file = SourceFile { path: SourcePath::new("fragment.yaml")?, text, hash: ContentHash::new([0; 32]) };
    let compiled = vc_compiler::compile(CompileInput { files: vec![file], ..CompileInput::default() });
    Ok(compiled.diagnostics.iter().map(|found| found.code).filter(|code| code.number() < 2000).collect())
}

/// A page as read so far: the versions of its project, the files of the current one, and the
/// fragments that do not parse.
#[derive(Default)]
struct Reading {
    versions: Vec<Files>,
    files: Files,
    problems: Vec<String>,
}

impl Reading {
    /// Writes a whole file, first closing the current version when it holds `path` already.
    fn write(&mut self, path: String, text: String) {
        if self.files.contains_key(&path) {
            self.versions.push(self.files.clone());
        }
        self.files.insert(path, text);
    }

    /// Checks a block marked [`WRONG`], at `at` on the page: its `file` (path, when it names one,
    /// and text) must have an error, with the files so far.
    fn wrong(&mut self, at: &str, file: (Option<String>, String)) -> Outcome<()> {
        let (path, text) = file;
        let failed = match path {
            Some(path) => {
                let mut files = self.files.clone();
                files.insert(path.clone(), text.clone());
                fails(&files, &path)?
            }
            None => !parse_errors(&text)?.is_empty(),
        };
        if !failed {
            self.problems.push(format!("{at}: a `wrong` example has no error; drop `wrong`:\n{text}"));
        }
        Ok(())
    }

    fn fragment(&mut self, name: &str, text: &str) -> Outcome<()> {
        let codes = parse_errors(text)?;
        if !codes.is_empty() {
            self.problems.push(format!("{name}: a fragment does not parse ({codes:?}):\n{text}"));
        }
        Ok(())
    }

    /// The page's versions, the last one included, and its problems so far.
    fn finish(mut self) -> Page {
        if !self.files.is_empty() {
            self.versions.push(self.files);
        }
        (self.versions, self.problems)
    }
}

/// The versions of the project `doc` writes out, and the fragments that do not parse.
fn read(name: &str, doc: &str) -> Outcome<Page> {
    let mut reading = Reading::default();
    let mut seen = String::new();
    for (_, heading, block) in support::blocks(doc).into_iter().filter(|(_, _, block)| checked(block)) {
        let text = format!("{}\n{}", block.first, block.body);
        let first_under = heading != seen;
        let path = named_path(if first_under { &heading } else { "" }, &block).filter(|_| !excerpt(&text));
        if wrong(&block) {
            reading.wrong(&format!("{name}:{}", block.line), (path, text))?;
            continue;
        }
        seen.clone_from(&heading);
        match path {
            Some(path) => reading.write(path, text),
            None => reading.fragment(name, &text)?,
        }
    }
    Ok(reading.finish())
}

/// `config` with `syntaxVersion` and a default profile added after it when it leaves them out (a
/// page often shows only the keys its point needs), so its lines keep their numbers.
fn completed(config: &str) -> String {
    let has = |key: &str| config.lines().any(|line| line.strip_prefix(key).is_some_and(|rest| rest.starts_with(':')));
    let ended = if config.is_empty() || config.ends_with('\n') { "" } else { "\n" };
    let version = if has("syntaxVersion") { "" } else { "syntaxVersion: 2\n" };
    let profiles = if has("profiles") { "" } else { PROFILES };
    format!("{config}{ended}{version}{profiles}")
}

/// Whether `found` is about a name defined in a file a page does not show.
fn elsewhere(found: &Diagnostic) -> bool {
    ELSEWHERE.contains(&found.code) || (found.code == Code::V1012 && found.message.ends_with("no such file"))
}

const fn none(_: &Diagnostic) -> bool {
    false
}

/// A page's `files`, its config [`completed`].
fn with_config(files: &Files) -> Vec<(String, String)> {
    let config = |(path, text): (&String, &String)| {
        let text = if path == support::CONFIG_PATH { completed(text) } else { text.clone() };
        (path.clone(), text)
    };
    files.iter().map(config).collect()
}

/// The errors `vc check` reports on a page's `files`, its config [`completed`], but for those
/// `allowed`.
fn errors(files: &Files, allowed: Allowed) -> Outcome<Vec<String>> {
    reported(&with_config(files), allowed)
}

/// Whether `vc check` reports an error in the file at `path` among a page's `files`.
fn fails(files: &Files, path: &str) -> Outcome<bool> {
    let report = support::check(&with_config(files))?;
    let in_path = |found: &&Diagnostic| file_of(found, &report.files) == path;
    Ok(report.diagnostics.iter().filter(in_path).any(|found| found.severity == Severity::Error))
}

/// The errors `vc check` reports on `files` (path, text) as they are, but for those `allowed`.
fn reported(files: &[(String, String)], allowed: Allowed) -> Outcome<Vec<String>> {
    let report = support::check(files)?;
    let errors = report.diagnostics.iter().filter(|found| found.severity == Severity::Error && !allowed(found));
    Ok(errors.map(|found| describe(found, &report.files)).collect())
}

/// The path of the file `found` points into.
fn file_of(found: &Diagnostic, files: &SourceFiles) -> String {
    files.get(found.primary.span.file).map_or_else(String::new, |file| file.path.to_string())
}

fn describe(found: &Diagnostic, files: &SourceFiles) -> String {
    let path = file_of(found, files);
    format!("{} {} ({path}:{})", found.code, found.message, found.primary.span.start.line)
}

/// Every problem with the examples on the page `doc`, each once.
fn problems(name: &str, doc: &str) -> Outcome<Vec<String>> {
    let (versions, mut found) = read(name, doc)?;
    for files in &versions {
        found.extend(errors(files, elsewhere)?.into_iter().map(|error| format!("{name}: {error}")));
    }
    let mut seen = BTreeSet::new();
    found.retain(|problem| seen.insert(problem.clone()));
    Ok(found)
}

/// Checks every page in `pages`, failing with all their problems.
fn check_pages(pages: &[PathBuf]) -> Outcome<()> {
    assert!(!pages.is_empty(), "no pages found");
    let root = root();
    let mut found = Vec::new();
    for page in pages {
        let name = page.strip_prefix(&root).unwrap_or(page).display().to_string();
        found.extend(problems(&name, &fs::read_to_string(page)?)?);
    }
    assert!(found.is_empty(), "{}", found.join("\n"));
    Ok(())
}

#[test]
fn every_yaml_example_in_the_design_docs_checks() -> Outcome<()> {
    check_pages(&files_under(&root().join("docs/design"), "md")?)
}

#[test]
fn every_yaml_example_on_the_docs_site_checks() -> Outcome<()> {
    check_pages(&files_under(&root().join("site/src/content/docs"), "mdx")?)
}

#[test]
fn the_site_example_project_checks_clean() -> Outcome<()> {
    let dir = root().join("site/src/examples");
    let mut files = Files::new();
    for path in files_under(&dir, "yaml")? {
        let relative = path.strip_prefix(&dir)?.to_string_lossy().into_owned();
        files.insert(relative, fs::read_to_string(&path)?);
    }
    assert!(files.contains_key(support::CONFIG_PATH), "the example project has no config");
    let errors = reported(&files.into_iter().collect::<Vec<_>>(), none)?;
    assert!(errors.is_empty(), "{}", errors.join("\n"));
    Ok(())
}

#[test]
fn excerpts_are_told_apart_from_whole_files() {
    assert!(excerpt("  - id: wait\n    path: /slow\n"));
    assert!(excerpt("# services/a/a.test.yaml\n- id: x\n"));
    assert!(excerpt("tests:\n  - id: x\n  ...\n"));
    assert!(!excerpt("# services/a/a.test.yaml\ntests:\n  - id: x\n"));
}

#[test]
fn wrong_examples_are_left_out_and_configs_completed() {
    let block = |info: &str| Block { line: 1, info: info.to_owned(), first: String::new(), body: String::new() };
    assert!(checked(&block(r#"yaml title="services/a/a.test.yaml""#)));
    assert!(checked(&block(r#"yaml wrong title="services/a/a.test.yaml""#)));
    assert!(wrong(&block(r#"yaml wrong title="services/a/a.test.yaml""#)) && !wrong(&block("yaml")));
    assert!(!checked(&block("yaml skip")));
    assert!(!checked(&block("console")));
    assert_eq!(named_path("", &block(r#"yaml title="vibranium.config.yaml (excerpt)""#)), None);
    assert_eq!(completed("vars: {}\n"), format!("vars: {{}}\nsyntaxVersion: 2\n{PROFILES}"));
    assert_eq!(completed("vars: {}"), format!("vars: {{}}\nsyntaxVersion: 2\n{PROFILES}"));
    assert_eq!(completed("syntaxVersion: 2\nprofiles: []\n"), "syntaxVersion: 2\nprofiles: []\n");
}

#[test]
fn an_excerpt_title_makes_a_fragment_whatever_names_the_file() {
    let block = |info: &str, first: &str| Block {
        line: 1,
        info: info.to_owned(),
        first: first.to_owned(),
        body: String::new(),
    };
    let excerpt = r#"yaml title="vibranium.config.yaml (excerpt)""#;
    let heading = "`vibranium.config.yaml`";
    assert_eq!(named_path("", &block(excerpt, "# vibranium.config.yaml")), None, "a `# path` first line");
    assert_eq!(named_path(heading, &block(excerpt, "vars: {}")), None, "a path heading");
    assert_eq!(named_path(heading, &block("yaml", "vars: {}")).as_deref(), Some("vibranium.config.yaml"));
    let first = block("yaml", "# vibranium.config.yaml");
    assert_eq!(named_path("", &first).as_deref(), Some("vibranium.config.yaml"));
}

#[test]
fn a_wrong_example_must_have_an_error() -> Outcome<()> {
    let page = |path: &str, text: &str| format!("```yaml wrong title=\"services/a/{path}\"\n{text}```\n");
    let (_, problems) = read("p", &page("a.test.yaml", "tests:\n  - id: callsNothing\n    call: a#missing\n"))?;
    assert!(problems.is_empty(), "{problems:?}");
    let (_, problems) = read("p", &page("a.api.yaml", "apis:\n  - id: lists\n    path: /a\n"))?;
    assert!(problems.len() == 1 && problems[0].starts_with("p:1: a `wrong` example has no error"), "{problems:?}");
    Ok(())
}
