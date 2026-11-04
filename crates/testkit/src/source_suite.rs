use vc_interfaces::{Discovery, SourceError, SourceFilter, SourceProvider};

/// Builds a provider over a project holding these files (relative path, bytes).
pub type MakeSources<'a> = dyn FnMut(&[(&str, &[u8])]) -> Box<dyn SourceProvider> + 'a;

/// The contract every [`SourceProvider`] meets (D112). `make` builds a provider over a
/// project holding the given files; paths use `/` and every name is one any OS accepts.
pub fn source_provider_suite(make: &mut MakeSources<'_>) {
    finds_definition_files_in_path_order(make);
    applies_the_filter(make);
    reports_files_that_are_not_utf8(make);
    finds_nothing_in_an_empty_project(make);
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
