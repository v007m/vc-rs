//! Datasets (D173): every CSV, JSON or YAML file under `datasets/`, and every entry of the
//! config's `datasets.files`, by ID; and the rows a test's `examples` or an action's `forEach`
//! reads, from `${dataset.<id>}` or a literal `$file`; and the JSON Schema files `body.schema`
//! names (D201).
//!
//! The compiler does no I/O (D132): the engine reads the data files the compile asks for
//! ([`DataWanted`]) and compiles again with them in [`crate::CompileInput::files`]. Until a file is
//! given its rows are unknown, so nothing about them is checked or lowered. A `datasets.files`
//! entry outside the project must be under one of `datasets.allowRoots` (V1012). That check is
//! lexical; the engine checks the canonical paths again, which catches symlinks.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use vc_diagnostics::{Code, Diagnostic, Diagnostics};
use vc_model::{FileKind, Namespace, Reference, Segment, SourceFiles, SourcePath};
use vc_parser::{Node, Value};

use super::assets;
use super::body_schema;
use super::project;
use super::project::{Project, Source};
use super::uses;
use crate::cache::CacheEntry;
use crate::suggest;

/// Where the datasets named by their path live.
const FOLDER: &str = "datasets/";
/// The suffixes a file under [`FOLDER`] has to be a dataset.
const SUFFIXES: [&str; 4] = [".csv", ".json", ".yaml", ".yml"];

/// A data file the compile names but was not given: the engine reads it and compiles again.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct DataWanted {
    /// The file, from the project root; it leads out of the project only under `root`.
    pub path: SourcePath,
    /// The `datasets.allowRoots` entry an outside `path` is under, normalised (`../shared`).
    /// The engine canonicalises both, and reads the file only when it is still under the root;
    /// otherwise it reports V1012 at the path.
    pub root: Option<SourcePath>,
}

/// One dataset's file, and the allowed root it is under when outside the project.
struct Set {
    path: String,
    root: Option<SourcePath>,
}

/// The project's datasets, and the data files the compile was given.
pub struct Data<'a> {
    given: HashMap<&'a str, &'a Node>,
    sets: BTreeMap<String, Set>,
    files: &'a SourceFiles,
    /// Whether the project's other files were listed, so every dataset is known.
    pub complete: bool,
}

impl<'a> Data<'a> {
    /// The datasets of `project`: the folder's files first, then the config's entries, which win.
    pub fn new(
        project: &Project<'a>,
        (entries, files): (&'a [CacheEntry], &'a SourceFiles),
        assets: Option<&BTreeSet<SourcePath>>,
    ) -> (Self, Diagnostics) {
        let given = given(entries, files);
        let listed = assets.into_iter().flatten().map(SourcePath::as_str).chain(given.keys().copied());
        let mut sets: BTreeMap<String, Set> =
            listed.filter_map(|path| Some((folder_id(path)?, Set { path: path.to_owned(), root: None }))).collect();
        let mut diagnostics = Diagnostics::new();
        let datasets = project.config().and_then(|config| config.root.get("datasets"));
        let roots: Vec<SourcePath> = datasets.map(allow_roots).unwrap_or_default();
        for node in datasets.map(|datasets| project::list(datasets, "files")).unwrap_or_default() {
            match configured(node, &roots) {
                Ok(Some((id, set))) => {
                    sets.insert(id.to_owned(), set);
                }
                Ok(None) => {}
                Err(diagnostic) => diagnostics.push(*diagnostic),
            }
        }
        let data = Self { given, sets, files, complete: assets.is_some() };
        diagnostics.extend(data.sets.values().filter_map(|set| data.not_rows(set)));
        (data, diagnostics)
    }

    /// The rows `node` names, when it is `${dataset.<id>}` or a literal `$file` given as a list.
    pub fn rows_at(&self, source: &Source<'_>, node: &Node) -> Option<&'a Node> {
        let path = self.path_at(source, node)?;
        self.given.get(path.as_str()).copied().filter(|rows| matches!(rows.value, Value::Seq(_)))
    }

    /// The dataset IDs, sorted.
    pub fn ids(&self) -> Vec<&str> {
        self.sets.keys().map(String::as_str).collect()
    }

    /// Each dataset whose file was given as a list: its ID, file and rows.
    pub fn loaded(&self) -> impl Iterator<Item = (&str, &str, &'a Node)> {
        let rows =
            |set: &Set| self.given.get(set.path.as_str()).copied().filter(|rows| matches!(rows.value, Value::Seq(_)));
        self.sets.iter().filter_map(move |(id, set)| Some((id.as_str(), set.path.as_str(), rows(set)?)))
    }

    /// The tree of the data file at `path`, when the compile was given it.
    pub fn file(&self, path: &str) -> Option<&'a Node> {
        self.given.get(path).copied()
    }

    /// The data files the project names and the compile was not given.
    pub fn wanted(&self, project: &Project<'_>, assets: Option<&BTreeSet<SourcePath>>) -> Vec<DataWanted> {
        let mut wanted: BTreeMap<String, Option<SourcePath>> = BTreeMap::new();
        wanted.extend(self.sets.values().map(|set| (set.path.clone(), set.root.clone())));
        for source in &project.sources {
            let rows = loops(source).into_iter().filter_map(|node| self.path_at(source, node));
            let schemas =
                body_schema::nodes(source.root).into_iter().filter_map(|node| body_schema::path(source, node));
            let known = rows.chain(schemas).filter(|path| assets.is_some_and(|assets| listed(assets, path)));
            wanted.extend(known.map(|path| (path, None)));
        }
        let missing = wanted.into_iter().filter(|(path, _)| !self.given.contains_key(path.as_str()));
        missing.filter_map(|(path, root)| Some(DataWanted { path: SourcePath::data(&path).ok()?, root })).collect()
    }

    /// The path of the data file `node` names, if it names one the compiler can know.
    fn path_at(&self, source: &Source<'_>, node: &Node) -> Option<String> {
        if let Some(text) = node.as_str() {
            return named(text).and_then(|id| self.sets.get(id)).map(|set| set.path.clone());
        }
        let [only] = node.entries() else { return None };
        let written =
            only.value.as_str().filter(|text| only.key == vc_model::FILE_KEY && !text.contains(['#', '$']))?;
        assets::resolve(&assets::folder(self.files.get(source.file)?.path.as_str()), written)
    }

    /// V1002 when the file of `set` was given but is not a list of rows.
    fn not_rows(&self, set: &Set) -> Option<Diagnostic> {
        let root = self.given.get(set.path.as_str()).filter(|root| !matches!(root.value, Value::Seq(_)))?;
        let message = format!("the dataset {} is not a list of rows", set.path);
        let diagnostic = Diagnostic::new(Code::V1002, root.span, message).label("a list expected");
        Some(diagnostic.help("a dataset is a CSV file, or a JSON or YAML list with one item per row"))
    }
}

/// V2002 for each `${dataset.…}` outside CEL that names no dataset, once every dataset is known.
pub fn unknown(project: &Project<'_>, data: &Data<'_>) -> Diagnostics {
    let mut diagnostics = Diagnostics::new();
    if !data.complete {
        return diagnostics;
    }
    let ids = data.ids();
    for source in &project.sources {
        let datasets = uses::references(source, Namespace::Dataset).into_iter();
        diagnostics.extend(datasets.filter_map(|reference| missing(source, reference, &ids)));
    }
    diagnostics
}

/// V2002 when no leading part of the reference's dotted path is a dataset ID.
fn missing(source: &Source<'_>, reference: &Reference, ids: &[&str]) -> Option<Diagnostic> {
    let keys = reference.path.segments.iter().map_while(|segment| match segment {
        Segment::Key(key) => Some(key.as_str()),
        Segment::Index(_) | Segment::All => None,
    });
    let keys: Vec<&str> = keys.collect();
    if (1..=keys.len()).any(|len| ids.contains(&keys[..len].join(".").as_str())) {
        return None;
    }
    let name = keys.join(".");
    let message = format!("`dataset.{name}` names no dataset");
    let diagnostic = Diagnostic::new(Code::V2002, reference.span, message).label("no dataset has this ID");
    let diagnostic = uses::did_you_mean(diagnostic, (source, reference, &name), ids);
    let known = if ids.is_empty() {
        "there are no datasets".to_owned()
    } else {
        format!("the datasets are {}", suggest::quoted(ids))
    };
    Some(
        diagnostic.note(format!(
            "{known}; a dataset is a file under `datasets/` or an entry of `datasets.files` in the config"
        )),
    )
}

/// The data files among `entries`: those of no kind that parsed.
fn given<'a>(entries: &'a [CacheEntry], files: &'a SourceFiles) -> HashMap<&'a str, &'a Node> {
    let unit = |entry: &'a CacheEntry| entry.unit.as_deref().filter(|unit| unit.kind.is_none());
    let root = |entry: &'a CacheEntry| Some((files.get(entry.file)?.path.as_str(), unit(entry)?.root.as_ref()?));
    entries.iter().filter_map(root).collect()
}

/// The ID a file under `datasets/` has: its path below the folder, dotted, without its suffix.
fn folder_id(path: &str) -> Option<String> {
    let inside = path.strip_prefix(FOLDER)?;
    let stem = SUFFIXES.iter().find_map(|suffix| inside.strip_suffix(suffix)).filter(|stem| !stem.is_empty())?;
    Some(stem.replace('/', "."))
}

/// The dataset ID in a template that is exactly `${dataset.<id>}`.
fn named(text: &str) -> Option<&str> {
    let id = text.trim().strip_prefix("${dataset.")?.strip_suffix('}')?;
    let plain = !id.is_empty() && id.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '-'));
    plain.then_some(id)
}

/// Whether `path` is one of the listed files.
fn listed(assets: &BTreeSet<SourcePath>, path: &str) -> bool {
    SourcePath::new(path).is_ok_and(|path| assets.contains(&path))
}

/// The `examples` and `forEach` values of a file: where rows are read.
fn loops<'n>(source: &Source<'n>) -> Vec<&'n Node> {
    let tests = project::list(source.root, "tests").iter().filter(|_| source.kind == FileKind::Test);
    let mut nodes: Vec<&Node> = tests.filter_map(|test| test.get("examples")).collect();
    nodes.extend(project::actions(source).into_iter().filter_map(|action| action.get("forEach")));
    nodes
}

/// `datasets.allowRoots`, normalised; one that is no path allows nothing.
fn allow_roots(datasets: &Node) -> Vec<SourcePath> {
    let written = project::list(datasets, "allowRoots").iter().filter_map(Node::as_str);
    written.filter_map(|root| SourcePath::data(root).ok()).collect()
}

/// A `datasets.files` entry: its ID and file when it has both, or why the file may not be read.
type Configured<'n> = Result<Option<(&'n str, Set)>, Box<Diagnostic>>;

/// One `datasets.files` entry as a dataset; V1012 when its file is no path the project may read.
fn configured<'n>(node: &'n Node, roots: &[SourcePath]) -> Configured<'n> {
    let (Some(id), Some(file)) = (project::text(node, "id"), node.get("file")) else { return Ok(None) };
    let Some(written) = file.as_str() else { return Ok(None) };
    let Ok(path) = SourcePath::data(written) else {
        return Err(unreadable(file, format!("the dataset file `{written}` is not a relative path to a file")));
    };
    let under =
        |root: &&SourcePath| path.as_str().strip_prefix(root.as_str()).is_some_and(|rest| rest.starts_with('/'));
    let root = roots.iter().find(under).cloned().filter(|_| path.is_outside());
    if path.is_outside() && root.is_none() {
        return Err(unreadable(file, format!("the dataset file `{written}` is outside the project")));
    }
    Ok(Some((id, Set { path: path.to_string(), root })))
}

/// V1012 at a `datasets.files` path.
fn unreadable(file: &Node, message: String) -> Box<Diagnostic> {
    let diagnostic = Diagnostic::new(Code::V1012, file.span, message).label("not under the project or an allowed root");
    let help = "paths are relative to the project root; add a folder outside it to `datasets.allowRoots`";
    Box::new(diagnostic.help(help))
}

#[cfg(test)]
mod tests {
    use vc_model::SourcePath;

    use super::{allow_roots, configured, folder_id, named};

    fn node(yaml: &str) -> vc_parser::Node {
        vc_parser::parse(vc_model::SourceFileId::new(0), vc_parser::Syntax::Yaml, yaml).0.unwrap()
    }

    #[test]
    fn files_under_the_folder_are_named_by_their_path() {
        assert_eq!(folder_id("datasets/cities.csv").as_deref(), Some("cities"));
        assert_eq!(folder_id("datasets/users/seed.yml").as_deref(), Some("users.seed"));
        for other in ["datasets/notes.txt", "data/cities.csv", "datasets/.csv"] {
            assert_eq!(folder_id(other), None, "{other}");
        }
    }

    #[test]
    fn only_a_bare_reference_names_a_dataset() {
        assert_eq!(named(" ${dataset.users.seed} "), Some("users.seed"));
        for other in ["${dataset.cities | take(3)}", "${dataset.}", "${var.cities}", "x ${dataset.cities}"] {
            assert_eq!(named(other), None, "{other}");
        }
    }

    #[test]
    fn a_file_outside_the_project_must_be_under_an_allowed_root() {
        let roots = allow_roots(&node("allowRoots: [../shared, /abs/../.., 7]\n"));
        assert_eq!(roots, [SourcePath::data("../shared").unwrap()]);
        let set = |file: &str| {
            let entry = node(&format!("id: c\nfile: {file}\n"));
            configured(&entry, &roots).map(|found| found.map(|(id, set)| (id.to_owned(), set)))
        };
        let inside = set("data/../c.csv").unwrap().unwrap();
        assert_eq!((inside.0.as_str(), inside.1.path.as_str(), inside.1.root.is_none()), ("c", "c.csv", true));
        let outside = set("../shared/x/c.csv").unwrap().unwrap().1;
        assert_eq!((outside.path.as_str(), outside.root), ("../shared/x/c.csv", roots.first().cloned()));
        for bad in ["../shared-data/c.csv", "../shared/../other/c.csv", "/etc/c.csv", "../shared"] {
            assert_eq!(set(bad).err().map(|d| d.code), Some(vc_diagnostics::Code::V1012), "{bad}");
        }
        assert!(set("[a]").unwrap().is_none());
    }
}
