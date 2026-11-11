use vc_interfaces::{Discovery, SourceError, SourceFilter, SourceProvider, SourceWriter};
use vc_model::SourcePath;

/// Builds a provider over a project holding these files (relative path, bytes).
pub type MakeSources<'a> = dyn FnMut(&[(&str, &[u8])]) -> Box<dyn SourceProvider> + 'a;

/// The contract every [`SourceProvider`] meets (D112). `make` builds a provider over a
/// project holding the given files; paths use `/` and every name is one any OS accepts.
pub fn source_provider_suite(make: &mut MakeSources<'_>) {
    finds_definition_files_in_path_order(make);
    applies_the_filter(make);
    reports_files_that_are_not_utf8(make);
    finds_nothing_in_an_empty_project(make);
    reads_any_file_by_path(make);
    reads_any_bytes_by_path(make);
}

const CONFIG: &[u8] = b"syntaxVersion: 2\n";
const API: &[u8] = b"apis:\n  get:\n    method: GET\n";

fn discover(provider: &dyn SourceProvider, filter: &SourceFilter) -> Discovery {
    let result = provider.discover(filter);
    assert_eq!(result.as_ref().err(), None, "discover failed");
    result.unwrap_or_default()
}

fn paths(discovery: &Discovery) -> Vec<&str> {
    discovery.files.iter().map(|file| file.path.as_str()).collect()
}

fn asset_paths(discovery: &Discovery) -> Vec<&str> {
    discovery.assets.iter().map(vc_model::SourcePath::as_str).collect()
}

fn finds_definition_files_in_path_order(make: &mut MakeSources<'_>) {
    let provider = make(&[
        ("vibranium.config.yaml", CONFIG),
        ("services/users/users.api.yaml", API),
        ("services/users/crud.test.yaml", b"tests: []\n"),
        ("load/peak.load.yaml", b"scenario: {}\n"),
        ("schemas/user.json", b"{}"),
        ("README.md", b"# project\n"),
    ]);
    let found = discover(provider.as_ref(), &SourceFilter::default());
    let expected = [
        "load/peak.load.yaml",
        "services/users/crud.test.yaml",
        "services/users/users.api.yaml",
        "vibranium.config.yaml",
    ];
    assert_eq!(paths(&found), expected, "only definition files, in path order");
    assert_eq!(found.problems, []);
    assert_eq!(asset_paths(&found), ["README.md", "schemas/user.json"], "the other files are listed as assets");
    let api = &found.files[2];
    assert_eq!(
        (api.text.as_bytes(), api.hash),
        (API, vc_interfaces::content_hash(API)),
        "text and BLAKE3 hash of the bytes"
    );
}

fn applies_the_filter(make: &mut MakeSources<'_>) {
    let provider = make(&[
        ("services/a.api.yaml", API),
        ("legacy/old.api.yaml", API),
        ("node_modules/pkg/x.api.yaml", API),
        (".vibranium/cache/x.api.yaml", API),
        ("legacy/body.json", b"{}"),
        ("node_modules/pkg/package.json", b"{}"),
    ]);
    let all = discover(provider.as_ref(), &SourceFilter::default());
    assert_eq!(paths(&all), ["legacy/old.api.yaml", "services/a.api.yaml"], "node_modules and .vibranium left out");
    assert_eq!(asset_paths(&all), ["legacy/body.json"]);
    let include = discover(provider.as_ref(), &SourceFilter::new(&["services/**"], &[]).unwrap_or_default());
    assert_eq!(paths(&include), ["services/a.api.yaml"]);
    let assets = ["legacy/body.json", "node_modules/pkg/package.json"];
    assert_eq!(asset_paths(&include), assets, "include globs pick definition files only; no default excludes");
    let exclude = discover(provider.as_ref(), &SourceFilter::new(&[], &["legacy"]).unwrap_or_default());
    assert_eq!(paths(&exclude), ["node_modules/pkg/x.api.yaml", "services/a.api.yaml"]);
    assert_eq!(asset_paths(&exclude), ["node_modules/pkg/package.json"]);
}

fn reports_files_that_are_not_utf8(make: &mut MakeSources<'_>) {
    let provider = make(&[
        ("bad.api.yaml", &[0xff, 0xfe, b'x']),
        ("good.api.yaml", API),
        ("data.bin", &[0xff]),
        ("z.test.yaml", b""),
    ]);
    let found = discover(provider.as_ref(), &SourceFilter::default());
    assert_eq!(paths(&found), ["good.api.yaml", "z.test.yaml"], "the other files are still read");
    assert_eq!(found.problems, [SourceError::NotUtf8 { path: "bad.api.yaml".to_owned() }]);
}

fn finds_nothing_in_an_empty_project(make: &mut MakeSources<'_>) {
    let provider = make(&[]);
    assert_eq!(discover(provider.as_ref(), &SourceFilter::default()), Discovery::default());
}

fn reads_any_file_by_path(make: &mut MakeSources<'_>) {
    let provider = make(&[
        ("vibranium.config.yaml", CONFIG),
        ("hooks/sign.ts", b"export const a = 1;\n"),
        ("datasets/users.json", b"[]"),
        ("datasets/bad.csv", &[0xff]),
    ]);
    let wanted = ["hooks/sign.ts", "datasets/users.json", "datasets/bad.csv", "datasets/gone.json", "hooks/sign.ts"];
    let wanted: Vec<SourcePath> = wanted.iter().filter_map(|path| SourcePath::new(path).ok()).collect();
    let result = provider.read(&wanted);
    assert_eq!(result.as_ref().err(), None, "read failed");
    let found = result.unwrap_or_default();
    assert_eq!(paths(&found), ["datasets/users.json", "hooks/sign.ts"], "any kind, in path order, each once");
    assert_eq!(found.files.get(1).map(|file| file.text.as_str()), Some("export const a = 1;\n"));
    let problems: Vec<(Option<&str>, bool)> =
        found.problems.iter().map(|problem| (problem.path(), matches!(problem, SourceError::NotUtf8 { .. }))).collect();
    assert_eq!(problems, [(Some("datasets/bad.csv"), true), (Some("datasets/gone.json"), false)], "the rest are read");
    assert!(
        matches!(found.problems.get(1), Some(SourceError::Missing { .. })),
        "a missing file is named as missing: {:?}",
        found.problems
    );
    assert_eq!(asset_paths(&found), Vec::<&str>::new(), "nothing is listed");
    assert_eq!(
        discover(provider.as_ref(), &SourceFilter::default()).files.len(),
        1,
        "discover still reads only definitions"
    );
}

fn reads_any_bytes_by_path(make: &mut MakeSources<'_>) {
    let provider = make(&[("vibranium.config.yaml", CONFIG), ("fixtures/logo.png", &[0x89, b'P', 0xff, 0])]);
    let path = |text: &str| SourcePath::new(text).ok();
    let logo = path("fixtures/logo.png").map(|logo| provider.read_bytes(&logo));
    assert_eq!(logo, Some(Ok(vec![0x89, b'P', 0xff, 0])), "the bytes as they are, text or not");
    let gone = path("fixtures/gone.png").map(|gone| provider.read_bytes(&gone));
    assert!(matches!(gone, Some(Err(SourceError::Missing { .. }))), "a missing file is named as missing: {gone:?}");
}

/// Builds a writer, and a provider that reads the same project, over a project holding these
/// files (relative path, bytes).
pub type MakeWriter<'a> = dyn FnMut(&[(&str, &[u8])]) -> (Box<dyn SourceWriter>, Box<dyn SourceProvider>) + 'a;

/// The contract every [`SourceWriter`] meets (D107).
///
/// A write replaces a file or creates it with its folders, a remove takes it away, and removing
/// a file that is not there names its path. A write of an unchanged file replaces it only while it still has the hash it was read with,
/// and creates one only where there is none.
pub fn source_writer_suite(make: &mut MakeWriter<'_>) {
    writes_a_file_with_its_folders_and_replaces_one(make);
    removes_a_file_and_names_one_that_is_not_there(make);
    writes_an_unchanged_file_only_while_it_has_the_hash_read(make);
    creates_an_unchanged_file_only_where_there_is_none(make);
}

/// Writes `text` at `path` and says what the writer answered.
fn write(writer: &dyn SourceWriter, path: &str, text: &str) -> Result<(), SourceError> {
    let path = SourcePath::new(path).map_err(|_| SourceError::InvalidPath { path: path.to_owned() })?;
    writer.write(&path, text)
}

fn writes_a_file_with_its_folders_and_replaces_one(make: &mut MakeWriter<'_>) {
    let (writer, provider) = make(&[("users.api.yaml", b"apis: []\n")]);
    assert_eq!(write(writer.as_ref(), "users.api.yaml", "apis: {}\n"), Ok(()), "replaces a file");
    assert_eq!(write(writer.as_ref(), "services/new/new.test.yaml", "tests: []\n"), Ok(()), "creates one");
    let wanted = ["users.api.yaml", "services/new/new.test.yaml"];
    let wanted: Vec<SourcePath> = wanted.iter().filter_map(|path| SourcePath::new(path).ok()).collect();
    let found = provider.read(&wanted).unwrap_or_default();
    let texts: Vec<(&str, &str)> = found.files.iter().map(|file| (file.path.as_str(), file.text.as_str())).collect();
    let expected = [("services/new/new.test.yaml", "tests: []\n"), ("users.api.yaml", "apis: {}\n")];
    assert_eq!(texts, expected, "reads back what was written");
}

fn removes_a_file_and_names_one_that_is_not_there(make: &mut MakeWriter<'_>) {
    let (writer, provider) = make(&[("services/old/old.api.yaml", b"apis: []\n"), ("kept.api.yaml", API)]);
    let Ok(path) = SourcePath::new("services/old/old.api.yaml") else { return };
    assert_eq!(writer.remove(&path), Ok(()), "removes a file");
    let left = discover(provider.as_ref(), &SourceFilter::default());
    assert_eq!(paths(&left), ["kept.api.yaml"], "the rest stay");
    let again = writer.remove(&path);
    assert!(
        matches!(&again, Err(SourceError::Write { path, .. }) if path == "services/old/old.api.yaml"),
        "a file that is not there names its path: {again:?}"
    );
}

/// The text of the file at `path`, as `provider` reads it now; `None` when there is none.
fn text_at(provider: &dyn SourceProvider, path: &SourcePath) -> Option<String> {
    provider.read(std::slice::from_ref(path)).ok()?.files.pop().map(|file| file.text)
}

fn writes_an_unchanged_file_only_while_it_has_the_hash_read(make: &mut MakeWriter<'_>) {
    let (writer, provider) = make(&[("users.api.yaml", b"apis: []\n")]);
    let Ok(path) = SourcePath::new("users.api.yaml") else { return };
    let read = vc_interfaces::content_hash(b"apis: []\n");
    let other = vc_interfaces::content_hash(b"apis: {}\n");
    let changed = Err(SourceError::Changed { path: "users.api.yaml".to_owned() });
    assert_eq!(writer.write_unchanged(&path, "x: 1\n", Some(other)), changed, "another hash is refused");
    assert_eq!(text_at(provider.as_ref(), &path).as_deref(), Some("apis: []\n"), "the file is left as it was");
    assert_eq!(writer.write_unchanged(&path, "x: 1\n", None), changed, "a file where none was expected is refused");
    assert_eq!(writer.write_unchanged(&path, "x: 1\n", Some(read)), Ok(()), "the hash read is replaced");
    assert_eq!(text_at(provider.as_ref(), &path).as_deref(), Some("x: 1\n"));
    assert_eq!(writer.write_unchanged(&path, "x: 2\n", Some(read)), changed, "the old hash no longer matches");
}

fn creates_an_unchanged_file_only_where_there_is_none(make: &mut MakeWriter<'_>) {
    let (writer, provider) = make(&[]);
    let Ok(path) = SourcePath::new("services/new/new.test.yaml") else { return };
    let changed = Err(SourceError::Changed { path: "services/new/new.test.yaml".to_owned() });
    let read = vc_interfaces::content_hash(b"tests: []\n");
    assert_eq!(writer.write_unchanged(&path, "tests: []\n", Some(read)), changed, "a removed file is refused");
    assert_eq!(text_at(provider.as_ref(), &path), None, "and not created");
    assert_eq!(writer.write_unchanged(&path, "tests: []\n", None), Ok(()), "creates it with its folders");
    assert_eq!(text_at(provider.as_ref(), &path).as_deref(), Some("tests: []\n"));
}
