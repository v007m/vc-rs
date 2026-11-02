//! Diagnostics with source snippets and "did you mean" suggestions.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;
use std::sync::Arc;

use crate::model::Span;

#[derive(Debug, Clone)]
pub struct Note {
    pub message: String,
    pub span: Option<Span>,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
    pub notes: Vec<Note>,
    pub help: Option<String>,
}

impl Diagnostic {
    pub fn new(code: &'static str, message: impl Into<String>, span: Span) -> Self {
        Self { code, message: message.into(), span, notes: vec![], help: None }
    }

    pub fn with_help(mut self, help: Option<String>) -> Self {
        self.help = help;
        self
    }
}

/// Optimal-string-alignment distance (Levenshtein + adjacent transpositions).
pub fn edit_distance(a: &str, b: &str) -> usize {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let cols = b.len() + 1;
    let mut d = vec![0usize; (a.len() + 1) * cols];
    for i in 0..=a.len() {
        d[i * cols] = i;
    }
    for j in 0..=b.len() {
        d[j] = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut v = (d[(i - 1) * cols + j] + 1).min(d[i * cols + j - 1] + 1).min(d[(i - 1) * cols + j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                v = v.min(d[(i - 2) * cols + j - 2] + 1);
            }
            d[i * cols + j] = v;
        }
    }
    d[a.len() * cols + b.len()]
}

pub fn suggest<'a>(unknown: &str, candidates: impl IntoIterator<Item = &'a str>) -> Option<String> {
    let mut best: Option<&str> = None;
    let mut best_dist = (unknown.len() / 4).max(2) + 1;
    for c in candidates {
        let dist = edit_distance(unknown, c);
        if dist < best_dist {
            best = Some(c);
            best_dist = dist;
        }
    }
    best.map(str::to_owned)
}

pub fn did_you_mean<'a>(unknown: &str, candidates: impl IntoIterator<Item = &'a str>) -> Option<String> {
    suggest(unknown, candidates).map(|s| format!("did you mean `{s}`?"))
}

fn loc(s: &Span) -> String {
    let rel = std::env::current_dir()
        .ok()
        .and_then(|cwd| pathdiff(Path::new(&*s.file), &cwd))
        .unwrap_or_else(|| s.file.to_string());
    format!("{rel}:{}:{}", s.line, s.col)
}

/// Minimal relative-path computation (std has none).
fn pathdiff(path: &Path, base: &Path) -> Option<String> {
    let path = std::fs::canonicalize(path).ok()?;
    let base = std::fs::canonicalize(base).ok()?;
    let common = path.components().zip(base.components()).take_while(|(a, b)| a == b).count();
    let ups = base.components().count() - common;
    let mut out: Vec<String> = std::iter::repeat_n("..".to_owned(), ups).collect();
    out.extend(path.components().skip(common).map(|c| c.as_os_str().to_string_lossy().into_owned()));
    Some(out.join("/"))
}

pub fn render(d: &Diagnostic, sources: &HashMap<Arc<str>, String>) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "error[{}]: {}\n  --> {}", d.code, d.message, loc(&d.span));
    if let Some(line) = sources.get(&d.span.file).and_then(|src| src.lines().nth(d.span.line as usize - 1)) {
        let pad = " ".repeat(d.span.line.to_string().len());
        let col = d.span.col as usize;
        let caret = (d.span.len as usize).min((line.len() + 1).saturating_sub(col)).max(1);
        let _ = writeln!(out, "  {pad} |\n  {} | {line}\n  {pad} | {}{}", d.span.line, " ".repeat(col - 1), "^".repeat(caret));
    }
    for n in &d.notes {
        match &n.span {
            Some(s) => {
                let _ = writeln!(out, "  = note: {} ({})", n.message, loc(s));
            }
            None => {
                let _ = writeln!(out, "  = note: {}", n.message);
            }
        }
    }
    if let Some(h) = &d.help {
        let _ = writeln!(out, "  = help: {h}");
    }
    out.trim_end().to_owned()
}
