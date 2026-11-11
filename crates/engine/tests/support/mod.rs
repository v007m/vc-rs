//! Checks a small project written out in a Markdown page: the explain pages and the docs'
//! examples both hold their files as fenced blocks.

use std::error::Error;
use std::path::PathBuf;
use std::sync::Arc;

use vc_engine::{CheckReport, CheckRequest, EngineBuilder};
use vc_model::Date;
use vc_testkit::{FixedClock, MemoryCompileCache, MemorySources};

/// What a helper gives: a value, or why the page could not be read or checked.
pub type Outcome<T> = Result<T, Box<dyn Error>>;

/// The config a project gets when its page writes none: one default profile.
pub const CONFIG: &str = "syntaxVersion: 2\nproject: shop\nprofiles:\n  - id: dev\n    default: true\n";

/// The config's path.
pub const CONFIG_PATH: &str = "vibranium.config.yaml";

/// The day every page is checked on, so an expired skip (V3104) does not depend on the calendar.
const TODAY: &str = "2026-11-03";

/// One fenced block: its info string, its first line and its lines after the first.
pub struct Block {
    /// The line of its opening fence, from 1.
    pub line: usize,
    pub info: String,
    pub first: String,
    pub body: String,
}

/// The fenced blocks of `markdown`, each with the `##`-level heading it sits under (empty before
/// the first) and the nearest `###` heading.
pub fn blocks(markdown: &str) -> Vec<(String, String, Block)> {
    let (mut section, mut heading, mut found) = (String::new(), String::new(), Vec::new());
    let mut lines = markdown.lines().enumerate();
    while let Some((at, line)) = lines.next() {
        if let Some(title) = line.strip_prefix("## ") {
            (section, heading) = (title.trim().to_owned(), String::new());
        } else if let Some(title) = line.strip_prefix("### ") {
            title.trim().clone_into(&mut heading);
        } else if let Some(info) = line.trim_start().strip_prefix("```").filter(|info| !info.is_empty()) {
            let indent = line.len() - line.trim_start().len();
            let inner: Vec<&str> =
                lines.by_ref().map(|(_, line)| line).take_while(|line| line.trim_start() != "```").collect();
            let inner: Vec<&str> =
                inner.iter().map(|line| line.get(indent..).unwrap_or_else(|| line.trim_start())).collect();
            let first = inner.first().copied().unwrap_or_default().to_owned();
            let body = inner.iter().skip(1).fold(String::new(), |body, line| body + line + "\n");
            found.push((section.clone(), heading.clone(), Block { line: at + 1, info: info.trim().to_owned(), first, body }));
        }
    }
    found
}

/// What `vc check` reports on `files` (path, text), with [`CONFIG`] added when they
/// hold no config, checked on a fixed day.
pub fn check(files: &[(String, String)]) -> Outcome<CheckReport> {
    let mut owned: Vec<(&str, &[u8])> = files.iter().map(|(path, text)| (path.as_str(), text.as_bytes())).collect();
    if !owned.iter().any(|(path, _)| *path == CONFIG_PATH) {
        owned.push((CONFIG_PATH, CONFIG.as_bytes()));
    }
    let today = Date::parse(TODAY).ok_or("TODAY is not a date")?;
    let engine = EngineBuilder::new(PathBuf::from("/project"))
        .sources(Arc::new(MemorySources::new(&owned)))
        .compile_cache(Arc::new(MemoryCompileCache::new()))
        .clock(Arc::new(FixedClock(today)))
        .build()?;
    Ok(engine.check(&CheckRequest::default())?)
}
