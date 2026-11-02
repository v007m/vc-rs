//! discover -> parse (with spans) -> validate (garde) -> link -> cycle-detect

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rayon::prelude::*;
use serde::de::DeserializeOwned;
use serde_saphyr::{Error as YamlError, Spanned};

use crate::diag::{did_you_mean, render, Diagnostic, Note};
use crate::model::{Api, ApiFile, Given, GivenFile, Located, Program, Span, Test, TestFile};
use crate::ports::ExprEngine;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Api,
    Given,
    Test,
}

const SUFFIXES: [(&str, Kind); 3] = [(".api.yaml", Kind::Api), (".given.yaml", Kind::Given), (".test.yaml", Kind::Test)];

pub fn discover(root: &Path) -> std::io::Result<Vec<(PathBuf, Kind)>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                stack.push(path);
            } else if let Some(name) = path.file_name().and_then(|n| n.to_str())
                && let Some((_, kind)) = SUFFIXES.iter().find(|(s, _)| name.ends_with(s))
            {
                out.push((path, *kind));
            }
        }
    }
    out.sort();
    Ok(out)
}

/// The typed content of one file.
enum Parsed {
    Apis(Vec<Api>),
    Givens(Vec<Given>),
    Tests(Vec<Test>),
}

struct FileOutcome {
    file: Arc<str>,
    source: String,
    parsed: Option<Parsed>,
    diags: Vec<Diagnostic>,
}

pub struct CompileResult {
    pub program: Program,
    pub diagnostics: Vec<Diagnostic>,
    pub sources: HashMap<Arc<str>, String>,
    pub file_count: usize,
}

impl CompileResult {
    pub fn format(&self) -> String {
        self.diagnostics.iter().map(|d| render(d, &self.sources)).collect::<Vec<_>>().join("\n\n")
    }
}

const KNOWN_KEYS: &[&str] = &[
    "apis", "givens", "tests", "id", "method", "path", "inputs", "body", "outputs", "guards", "status", "scope", "api", "uses",
    "with", "expose", "repeat", "expect", "match", "schema", "cel",
];

fn span_at(file: &Arc<str>, loc: serde_saphyr::Location, len: usize) -> Span {
    Span { file: file.clone(), line: loc.line() as u32, col: loc.column() as u32, len: len.max(1) as u32 }
}

fn yaml_diags(file: &Arc<str>, err: &YamlError) -> Vec<Diagnostic> {
    let err = err.without_snippet();
    match err {
        YamlError::ValidationErrors { errors, .. } => errors.iter().flat_map(|e| yaml_diags(file, e)).collect(),
        YamlError::ValidationError { issues, locations, .. } => issues
            .iter()
            .map(|issue| {
                let loc = locations.locations_for(&issue.path).and_then(|l| l.primary_location());
                let path = issue.path.segments().map(|(_, s)| s).collect::<Vec<_>>().join(".");
                let msg = issue.message.clone().unwrap_or_else(|| issue.code.clone());
                let span = loc.map(|l| span_at(file, l, l.span().len() as usize)).unwrap_or_else(|| span_at(file, serde_saphyr::Location::UNKNOWN, 1));
                Diagnostic::new("E002", format!("{path}: {msg}"), span)
            })
            .collect(),
        YamlError::SerdeUnknownField { field, location, .. } => {
            vec![Diagnostic::new("E002", format!("unknown field `{field}`"), span_at(file, *location, field.len()))
                .with_help(did_you_mean(field, KNOWN_KEYS.iter().copied()))]
        }
        other => {
            let loc = other.location().unwrap_or(serde_saphyr::Location::UNKNOWN);
            let first_line = other.to_string().lines().next().unwrap_or_default().to_owned();
            vec![Diagnostic::new("E002", first_line, span_at(file, loc, 1))]
        }
    }
}

fn parse_as<T: DeserializeOwned + garde::Validate<Context = ()>>(file: &Arc<str>, src: &str) -> Result<T, Vec<Diagnostic>> {
    serde_saphyr::from_str_valid::<T>(src).map_err(|e| yaml_diags(file, &e))
}

fn parse_file(path: &Path, kind: Kind) -> FileOutcome {
    let file: Arc<str> = Arc::from(path.to_string_lossy().as_ref());
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            let d = Diagnostic::new("E001", e.to_string(), Span { file: file.clone(), line: 1, col: 1, len: 1 });
            return FileOutcome { file, source: String::new(), parsed: None, diags: vec![d] };
        }
    };
    let parsed = match kind {
        Kind::Api => parse_as::<ApiFile>(&file, &source).map(|f| Parsed::Apis(f.apis)),
        Kind::Given => parse_as::<GivenFile>(&file, &source).map(|f| Parsed::Givens(f.givens)),
        Kind::Test => parse_as::<TestFile>(&file, &source).map(|f| Parsed::Tests(f.tests)),
    };
    match parsed {
        Ok(p) => FileOutcome { file, source, parsed: Some(p), diags: vec![] },
        Err(diags) => FileOutcome { file, source, parsed: None, diags },
    }
}

fn id_span<T>(loc: &Located<T>, id: &Spanned<String>) -> Span {
    loc.span_of(id, id.value.len())
}

pub fn compile(root: &Path, exprs: &dyn ExprEngine) -> std::io::Result<CompileResult> {
    let files = discover(root)?;
    // Parse + validate in parallel; files are independent.
    let outcomes: Vec<FileOutcome> = files.par_iter().map(|(p, k)| parse_file(p, *k)).collect();

    let mut program = Program::default();
    let mut diagnostics = Vec::new();
    let mut sources = HashMap::with_capacity(outcomes.len());
    let mut first_def: HashMap<String, Span> = HashMap::new();

    // Registers an id; reports E003 and returns false for duplicates.
    let mut claim = |id: &str, span: Span, diagnostics: &mut Vec<Diagnostic>| -> bool {
        if let Some(prev) = first_def.get(id) {
            diagnostics.push(Diagnostic {
                code: "E003",
                message: format!("duplicate id `{id}`"),
                span,
                notes: vec![Note { message: "first defined here".into(), span: Some(prev.clone()) }],
                help: Some("ids must be unique across all *.api.yaml, *.given.yaml and *.test.yaml files".into()),
            });
            return false;
        }
        first_def.insert(id.to_owned(), span);
        true
    };

    for o in outcomes {
        diagnostics.extend(o.diags);
        let file = o.file.clone();
        sources.insert(o.file, o.source);
        match o.parsed {
            None => {}
            Some(Parsed::Apis(apis)) => {
                for def in apis {
                    let loc = Located { def, file: file.clone() };
                    if claim(&loc.def.id.value, id_span(&loc, &loc.def.id), &mut diagnostics) {
                        program.apis.insert(loc.def.id.value.clone(), loc);
                    }
                }
            }
            Some(Parsed::Givens(givens)) => {
                for def in givens {
                    let loc = Located { def, file: file.clone() };
                    if claim(&loc.def.id.value, id_span(&loc, &loc.def.id), &mut diagnostics) {
                        program.given_order.push(loc.def.id.value.clone());
                        program.givens.insert(loc.def.id.value.clone(), loc);
                    }
                }
            }
            Some(Parsed::Tests(tests)) => {
                for def in tests {
                    let loc = Located { def, file: file.clone() };
                    if claim(&loc.def.id.value, id_span(&loc, &loc.def.id), &mut diagnostics) {
                        program.tests.push(loc);
                    }
                }
            }
        }
    }

    diagnostics.extend(link(&program, exprs));
    diagnostics.extend(detect_cycles(&program));
    diagnostics.sort_by(|a, b| (&a.span.file, a.span.line, a.span.col).cmp(&(&b.span.file, b.span.line, b.span.col)));
    Ok(CompileResult { program, diagnostics, sources, file_count: files.len() })
}

fn link(program: &Program, exprs: &dyn ExprEngine) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let check_call = |out: &mut Vec<Diagnostic>,
                      file: &Arc<str>,
                      api_id: &Spanned<String>,
                      uses: &std::collections::BTreeMap<String, Spanned<String>>,
                      with: &std::collections::BTreeMap<String, serde_json::Value>| {
        let span = |s: &Spanned<String>| span_at(file, s.referenced, s.value.len());
        for gid in uses.values() {
            if !program.givens.contains_key(&gid.value) {
                out.push(
                    Diagnostic::new("E004", format!("unknown given `{}`", gid.value), span(gid))
                        .with_help(did_you_mean(&gid.value, program.givens.keys().map(String::as_str))),
                );
            }
        }
        let Some(api) = program.apis.get(&api_id.value) else {
            out.push(
                Diagnostic::new("E004", format!("unknown api `{}`", api_id.value), span(api_id))
                    .with_help(did_you_mean(&api_id.value, program.apis.keys().map(String::as_str))),
            );
            return;
        };
        for input in with.keys() {
            if !api.def.inputs.contains(input) {
                // `with` keys are plain Strings (no span): point at the api reference instead.
                out.push(
                    Diagnostic::new("E006", format!("api `{}` has no input `{input}`", api_id.value), span(api_id))
                        .with_help(Some(format!("declared inputs: [{}]", api.def.inputs.join(", ")))),
                );
            }
        }
    };
    for id in &program.given_order {
        let g = &program.givens[id];
        check_call(&mut out, &g.file, &g.def.api, &g.def.uses, &g.def.with_);
    }
    let mut cel_seen: HashSet<&str> = HashSet::new();
    for t in &program.tests {
        check_call(&mut out, &t.file, &t.def.api, &t.def.uses, &t.def.with_);
        if let Some(cel) = &t.def.expect.cel
            && !cel_seen.contains(cel.value.as_str())
        {
            match exprs.compile(&cel.value) {
                Ok(_) => {
                    cel_seen.insert(&cel.value);
                }
                Err(e) => out.push(Diagnostic::new(
                    "E007",
                    format!("invalid CEL expression: {}", e.lines().next().unwrap_or_default()),
                    t.span_of(cel, cel.value.len()),
                )),
            }
        }
    }
    out
}

fn detect_cycles(program: &Program) -> Vec<Diagnostic> {
    #[derive(Clone, Copy, PartialEq)]
    enum State {
        Visiting,
        Done,
    }
    fn visit<'p>(
        id: &'p str,
        program: &'p Program,
        state: &mut HashMap<&'p str, State>,
        stack: &mut Vec<&'p str>,
        out: &mut Vec<Diagnostic>,
    ) {
        state.insert(id, State::Visiting);
        stack.push(id);
        let g = &program.givens[id];
        for dep in g.def.uses.values() {
            let Some((dep_id, _)) = program.givens.get_key_value(&dep.value) else { continue };
            match state.get(dep_id.as_str()) {
                Some(State::Visiting) => {
                    let start = stack.iter().position(|s| *s == dep_id).unwrap_or(0);
                    let mut cycle: Vec<&str> = stack[start..].to_vec();
                    cycle.push(dep_id);
                    let notes = cycle[..cycle.len() - 1]
                        .iter()
                        .map(|c| {
                            let cg = &program.givens[*c];
                            Note { message: format!("`{c}` defined here"), span: Some(id_span(cg, &cg.def.id)) }
                        })
                        .collect();
                    out.push(Diagnostic {
                        code: "E005",
                        message: format!("dependency cycle between givens: {}", cycle.join(" -> ")),
                        span: g.span_of(dep, dep.value.len()),
                        notes,
                        help: Some("break the cycle by removing one of the `uses` entries".into()),
                    });
                }
                Some(State::Done) => {}
                None => visit(dep_id, program, state, stack, out),
            }
        }
        stack.pop();
        state.insert(id, State::Done);
    }
    let mut out = Vec::new();
    let mut state = HashMap::new();
    let mut stack = Vec::new();
    for id in &program.given_order {
        if !state.contains_key(id.as_str()) {
            visit(id, program, &mut state, &mut stack, &mut out);
        }
    }
    out
}
