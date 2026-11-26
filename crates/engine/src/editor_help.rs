//! The YAML editor's completion and hover (D681). The cursor and the shapes come from the
//! compiler; the IDs and var names from the project on disk, as `vc __complete` reads them.
//! Var values are never offered: only their names and where they are set.

use std::fmt::Write as _;

use vc_compiler::{Cursor, KeyHelp, Place};
use vc_model::{FileKind, Program, SourcePath, Sym};

use crate::{CompleteKind, Engine};

/// The text an editor holds and the cursor in it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct EditorRequest {
    /// The file's path, relative to the project root; only its kind is read from it.
    pub path: String,
    /// The text as the editor holds it, saved or not.
    pub text: String,
    /// The cursor, as a byte offset into `text`.
    pub offset: usize,
}

/// One completion.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Completion {
    /// The text it writes.
    pub label: String,
    /// What it is.
    pub kind: CompletionKind,
    /// A short note beside it: a key's type, where a var is set.
    pub detail: String,
    /// A key's docs.
    pub doc: String,
}

/// What a completion is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CompletionKind {
    /// A key the map takes.
    Key,
    /// A word the value takes.
    Value,
    /// A test or api ID.
    Reference,
    /// A var, as `var.<name>`.
    Var,
    /// A matcher.
    Matcher,
}

/// What hovering shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Hover {
    /// The key's docs, as Markdown.
    pub markdown: String,
    /// The ID the value names, when the key takes a reference: what an app explains below.
    pub reference: Option<String>,
}

/// The keys whose value is a test or api ID, the IDs they complete with, and their note.
const REFERENCES: &[(&str, CompleteKind, &str)] =
    &[("call", CompleteKind::Apis, "api"), ("test", CompleteKind::Tests, "test")];

impl Engine {
    /// The completions at the cursor of `request`, starting with what is written before it:
    /// the keys the map takes and has not got, matchers where they go, the words or IDs a
    /// value takes, and the vars inside `${`. Nothing for a path of no definition kind.
    #[must_use]
    pub fn complete_at(&self, request: &EditorRequest) -> Vec<Completion> {
        let Some(kind) = kind_of(&request.path) else { return Vec::new() };
        let cursor = vc_compiler::cursor(&request.text, request.offset);
        let path: Vec<&str> = cursor.path.iter().map(String::as_str).collect();
        let mut found = match cursor.place {
            Place::Value => self.values(kind, &path, &cursor),
            Place::Template => self.vars(),
            _ => keys(kind, &path, &cursor),
        };
        found.retain(|completion| completion.label.starts_with(&cursor.word));
        found
    }

    /// The IDs a reference key takes, else the words its value takes.
    fn values(&self, kind: FileKind, path: &[&str], cursor: &Cursor) -> Vec<Completion> {
        let Some(key) = cursor.key.as_deref() else { return Vec::new() };
        let at = [path, &[key]].concat();
        let reference = vc_compiler::key_help(kind, &at).is_some_and(|help| help.reference);
        if let Some((_, ids, note)) = REFERENCES.iter().find(|(name, ..)| reference && *name == key) {
            let found = self.complete(*ids, &cursor.word).into_iter();
            return found.map(|id| completion(id, CompletionKind::Reference, (*note).to_owned())).collect();
        }
        let words = vc_compiler::value_words(kind, &at).into_iter();
        words.map(|word| completion(word.to_owned(), CompletionKind::Value, String::new())).collect()
    }

    /// `var.<name>` for each var the config and its profiles set, with where it is set.
    fn vars(&self) -> Vec<Completion> {
        let Ok((program, _)) = self.program() else { return Vec::new() };
        let vars = var_origins(&program).into_iter();
        vars.map(|(name, origin)| completion(format!("var.{name}"), CompletionKind::Var, origin)).collect()
    }
}

/// What hovering at the cursor of `request` shows: the docs of the key there, and the ID its
/// value names when it takes a reference and the cursor is on the value.
#[must_use]
pub fn hover_at(request: &EditorRequest) -> Option<Hover> {
    let kind = kind_of(&request.path)?;
    let cursor = vc_compiler::cursor(&request.text, request.offset);
    let key = cursor.key.as_deref()?;
    let at: Vec<&str> = cursor.path.iter().map(String::as_str).chain([key]).collect();
    let help = vc_compiler::key_help(kind, &at)?;
    let on_value = cursor.place == Place::Value && help.reference && !cursor.value.is_empty();
    Some(Hover { markdown: markdown(&help), reference: on_value.then(|| cursor.value.clone()) })
}

/// The keys the map at `path` takes that it has not got, then matchers where they go.
fn keys(kind: FileKind, path: &[&str], cursor: &Cursor) -> Vec<Completion> {
    let declared = vc_compiler::keys_at(kind, path).into_iter().filter(|key| !cursor.siblings.contains(&key.name));
    let mut found: Vec<Completion> =
        declared.map(|key| Completion { doc: key.doc, ..completion(key.name, CompletionKind::Key, key.ty) }).collect();
    if vc_compiler::takes_matchers(kind, path) {
        let matchers = vc_compiler::matcher_names().into_iter();
        found.extend(matchers.map(|name| completion(name.to_owned(), CompletionKind::Matcher, "matcher".to_owned())));
    }
    found
}

const fn completion(label: String, kind: CompletionKind, detail: String) -> Completion {
    Completion { label, kind, detail, doc: String::new() }
}

/// The definition kind of the file at `path`.
fn kind_of(path: &str) -> Option<FileKind> {
    FileKind::from_path(&SourcePath::new(path).ok()?)
}

/// Each var the config and its profiles set, once, with every place it is set.
fn var_origins(program: &Program) -> Vec<(String, String)> {
    let text = |sym: Sym| program.str(sym).unwrap_or_default().to_owned();
    let config = program.config.vars.values.keys().map(|name| (text(name.sym()), "config".to_owned()));
    let by_profile = program
        .config
        .vars
        .by_profile
        .iter()
        .chain(program.profiles.values().map(|profile| (&profile.id, &profile.vars)));
    let profiles = by_profile
        .flat_map(|(id, vars)| vars.keys().map(move |name| (text(name.sym()), format!("profile {}", text(id.sym())))));
    let mut vars: Vec<(String, String)> = Vec::new();
    for (name, origin) in config.chain(profiles) {
        match vars.iter_mut().find(|(known, _)| *known == name) {
            Some((_, origins)) if !origins.split(", ").any(|known| known == origin) => {
                *origins = format!("{origins}, {origin}");
            }
            Some(_) => {}
            None => vars.push((name, origin)),
        }
    }
    vars
}

/// A key's docs: its name, type, whether it is required and its default, then what it is for
/// and an example.
fn markdown(help: &KeyHelp) -> String {
    let mut out = format!("**`{}`** · {}", help.name, help.ty);
    if help.required {
        out.push_str(" · required");
    }
    if let Some(default) = &help.default {
        let _ = write!(out, " · default `{default}`");
    }
    for (prefix, part) in [("", Some(&help.doc).filter(|doc| !doc.is_empty())), ("Example: ", help.example.as_ref())] {
        if let Some(part) = part {
            let _ = write!(out, "\n\n{prefix}{part}");
        }
    }
    out
}

#[cfg(test)]
#[path = "editor_help_tests.rs"]
mod tests;
