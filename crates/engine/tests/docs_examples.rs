//! The YAML examples in `docs/design/*.md` are checked, so the docs cannot drift from the code.
//!
//! A block fenced as `yaml` is a **whole file** when its first line is exactly `# <path>` and
//! `<path>` names a definition file (`# services/users/users.api.yaml`), or when it is the first
//! block under a `###` heading that is just such a path in backticks (as in "What `vc init`
//! writes"). Each doc's whole files are checked together as one project, with a config holding a
//! default profile when the doc writes none. They must have no errors (warnings are fine) but
//! those about names defined in files the doc does not show ([`ELSEWHERE`]), since a doc writes
//! out the files its point needs, not a whole project:
//!
//! - V2002: an auth from a config the doc does not show;
//! - V2003: an api or test from a file the doc does not show;
//! - V2004: a body, schema or fixture (`$file: ../../schemas/user.json`) not written out;
//! - V2101: a var set by such a config, or saved by such an api.
//!
//! Any other YAML block is a **fragment**: an excerpt that only has to parse (no V1 error). A
//! block fenced as `yaml skip` is left out. Two whole files with the same path in one doc are an
//! error, since one would hide the other.

mod support;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use support::{Block, Outcome};
use vc_compiler::{CompileInput, compile};
use vc_diagnostics::{Code, Diagnostic, Severity};
use vc_engine::SourceFiles;
use vc_model::{ContentHash, FileKind, SourceFile, SourcePath};

/// A doc's whole files: their text by path.
type Files = BTreeMap<String, String>;

/// Errors about names defined in files a doc does not write out (see the top of this file).
const ELSEWHERE: [Code; 4] = [Code::V2002, Code::V2003, Code::V2004, Code::V2101];

fn docs() -> Outcome<Vec<PathBuf>> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/design");
    let mut docs = fs::read_dir(dir)?.map(|entry| entry.map(|entry| entry.path())).collect::<Result<Vec<_>, _>>()?;
    docs.retain(|path| path.extension().is_some_and(|ext| ext == "md"));
    docs.sort();
    Ok(docs)
}

/// `text` when it is the path of a definition file.
fn definition(text: &str) -> Option<String> {
    let path = SourcePath::new(text).ok()?;
    FileKind::from_path(&path).map(|_| text.to_owned())
}

/// The path a block is the whole of, if it is a whole file.
fn whole_path(heading: &str, block: &Block) -> Option<String> {
    let named = block.first.strip_prefix("# ").and_then(definition);
    named.or_else(|| heading.strip_prefix('`').and_then(|rest| rest.strip_suffix('`')).and_then(definition))
}

/// The V1 codes a fragment's text gets when parsed on its own.
fn parse_errors(text: &str) -> Outcome<Vec<Code>> {
    let path = SourcePath::new("fragment.yaml")?;
    let file = SourceFile { path, text: text.to_owned(), hash: ContentHash::new([0; 32]) };
    let compiled = compile(CompileInput { files: vec![file], ..CompileInput::default() });
    Ok(compiled.diagnostics.iter().map(|found| found.code).filter(|code| code.number() < 2000).collect())
}

/// The whole files of `doc`, by path, and the problems found so far (fragments that do not
/// parse, paths written twice).
fn read(name: &str, doc: &str) -> Outcome<(Files, Vec<String>)> {
    let (mut files, mut problems) = (BTreeMap::new(), Vec::new());
    let mut seen = String::new();
    for (_, heading, block) in support::blocks(doc).into_iter().filter(|(_, _, block)| block.info == "yaml") {
        let text = format!("{}\n{}", block.first, block.body);
        let first_under = heading != seen;
        seen.clone_from(&heading);
        match whole_path(if first_under { &heading } else { "" }, &block) {
            Some(path) if files.contains_key(&path) => problems.push(format!("{name}: {path} is written twice")),
            Some(path) => drop(files.insert(path, text)),
            None => {
                let codes = parse_errors(&text)?;
                if !codes.is_empty() {
                    problems.push(format!("{name}: a fragment does not parse ({codes:?}):\n{text}"));
                }
            }
        }
    }
    Ok((files, problems))
}

/// The errors `vc check` reports on a doc's whole files, but for [`ELSEWHERE`].
fn errors(files: Files) -> Outcome<Vec<String>> {
    let files: Vec<(String, String)> = files.into_iter().collect();
    let report = support::check(&files)?;
    let errors =
        report.diagnostics.iter().filter(|found| found.severity == Severity::Error && !ELSEWHERE.contains(&found.code));
    Ok(errors.map(|found| describe(found, &report.files)).collect())
}

fn describe(found: &Diagnostic, files: &SourceFiles) -> String {
    let span = found.primary.span;
    let path = files.get(span.file).map_or_else(String::new, |file| file.path.to_string());
    format!("{} {} ({path}:{})", found.code, found.message, span.start.line)
}

#[test]
fn every_yaml_example_in_the_docs_checks() -> Outcome<()> {
    let mut problems = Vec::new();
    for doc in docs()? {
        let name = doc.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        let (files, found) = read(&name, &fs::read_to_string(&doc)?)?;
        problems.extend(found);
        problems.extend(errors(files)?.into_iter().map(|error| format!("{name}: {error}")));
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    Ok(())
}
