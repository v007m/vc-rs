use std::sync::Arc;

use indexmap::IndexMap;
use vc_diagnostics::{Code, Diagnostics};
use vc_model::{ContentHash, SourceFile, SourceFileId, SourcePath, StringTable};

use super::{CacheEntry, CompileCache, MAGIC, VERSION};
use crate::parts::Tools;
use crate::stage::{self, FileUnit};
use crate::{CompileInput, Compiled, compile};

const API: &str = "services/users/users.api.yaml";
const API_TEXT: &str = "apis:\n  - id: getUser\n    path: /users/${var.userId}\n";
const BAD: &str = "services/orders/orders.api.yaml";
const HEADER: usize = MAGIC.len() + 8;

fn entry(path: &str, byte: u8, file: u32) -> CacheEntry {
    let unit = FileUnit { kind: None, root: None, parts: IndexMap::new(), diagnostics: Diagnostics::new() };
    let path = SourcePath::new(path).unwrap();
    CacheEntry { path, hash: ContentHash::new([byte; 32]), file: SourceFileId::new(file), unit: Some(Arc::new(unit)) }
}

fn cache() -> CompileCache {
    CompileCache::of(vec![entry("a.api.yaml", 1, 0), entry("services/b.test.yaml", 2, 1)], StringTable::new())
}

fn source(path: &str, text: &str, byte: u8) -> SourceFile {
    SourceFile { path: SourcePath::new(path).unwrap(), text: text.to_owned(), hash: ContentHash::new([byte; 32]) }
}

/// A cache of `sources` run through the per-file stages, numbered in order, with a few names
/// interned first so the string table is not empty.
fn compiled(sources: &[SourceFile]) -> CompileCache {
    let (tools, mut strings) = (Tools::new(), StringTable::new());
    for name in ["users#getUser", "orders", "users#getUser", "userId"] {
        strings.intern(name).unwrap();
    }
    let mut entries = Vec::new();
    for (source, number) in sources.iter().zip(0..) {
        let file = SourceFileId::new(number);
        let unit = stage::unit(file, source, (&tools, &mut strings));
        entries.push(CacheEntry { path: source.path.clone(), hash: source.hash, file, unit: Some(Arc::new(unit)) });
    }
    CompileCache::of(entries, strings)
}

fn kept(cache: &CompileCache, path: &str, byte: u8, file: u32) -> Option<Arc<FileUnit>> {
    cache.unit(&SourcePath::new(path).unwrap(), &ContentHash::new([byte; 32]), SourceFileId::new(file))
}

/// A blob with this version's header around the header `json`, then `units`.
fn blob(json: &str, units: &str) -> Vec<u8> {
    let mut bytes = MAGIC.to_vec();
    bytes.extend(VERSION.to_le_bytes());
    bytes.extend(u32::try_from(json.len()).unwrap().to_le_bytes());
    bytes.extend(json.as_bytes());
    bytes.extend(units.as_bytes());
    bytes
}

/// A blob body with the string table `strings` and one file at `path` with no unit.
fn body(strings: &str, path: &str) -> String {
    let hash = ["0"; 32].join(",");
    format!(r#"{{"strings":{strings},"entries":[{{"path":"{path}","hash":[{hash}],"file":0,"unit":null}}]}}"#)
}

#[test]
fn bytes_round_trip_with_the_units_and_names() {
    let cache = compiled(&[source(API, API_TEXT, 1), source(BAD, "nme: x\n", 2)]);
    let read = CompileCache::from_bytes(&cache.to_bytes());
    assert_eq!(read, cache);
    let unit = kept(&read, API, 1, 0).unwrap();
    assert!(unit.root.is_some() && !unit.parts.is_empty() && unit.diagnostics.is_empty());
    assert_eq!(Some(&*unit), kept(&cache, API, 1, 0).as_deref());
    assert_eq!((cache.units(), read.units()), (2, 1));
    assert!(kept(&cache, BAD, 2, 1).is_some_and(|bad| !bad.diagnostics.is_empty()));
    assert!(kept(&read, BAD, 2, 1).is_none(), "a unit with diagnostics is compiled again");
    assert_eq!(CompileCache::from_bytes(&CompileCache::new().to_bytes()), CompileCache::new());
}

#[test]
fn bytes_round_trip_with_the_names_in_symbol_order() {
    let cache = compiled(&[source(API, API_TEXT, 1)]);
    let read = CompileCache::from_bytes(&cache.to_bytes());
    assert!(read.strings().texts().eq(cache.strings().texts()));
    for name in ["users#getUser", "orders", "userId"] {
        let sym = cache.strings().get(name).unwrap();
        assert_eq!(read.strings().resolve(sym), Some(name));
    }
}

#[test]
fn finds_a_unit_only_for_the_same_path_hash_and_number() {
    let cache = cache();
    let path = SourcePath::new("a.api.yaml").unwrap();
    assert!(cache.unit(&path, &ContentHash::new([1; 32]), SourceFileId::new(0)).is_some());
    assert!(cache.unit(&path, &ContentHash::new([9; 32]), SourceFileId::new(0)).is_none());
    assert!(cache.unit(&path, &ContentHash::new([1; 32]), SourceFileId::new(1)).is_none());
    assert_ne!(cache, CompileCache::new());
}

#[test]
fn another_version_reads_as_empty() {
    for version in [1, 2, 3] {
        let mut bytes = cache().to_bytes();
        bytes[MAGIC.len()] = version;
        assert!(CompileCache::from_bytes(&bytes).is_empty(), "version {version}");
    }
}

#[test]
fn damaged_bytes_read_as_empty() {
    let bytes = compiled(&[source(API, API_TEXT, 1)]).to_bytes();
    assert_eq!(CompileCache::from_bytes(&bytes).len(), 1);
    for cut in [0, 3, 7, HEADER + 1, bytes.len() - 1] {
        assert!(CompileCache::from_bytes(&bytes[..cut]).is_empty(), "cut at {cut}");
    }
    let mut bad_json = bytes;
    bad_json[HEADER] = b'x';
    assert!(CompileCache::from_bytes(&bad_json).is_empty());
    assert!(CompileCache::from_bytes(b"not a cache at all").is_empty());
}

#[test]
fn a_path_or_string_table_that_does_not_read_back_reads_as_empty() {
    assert_eq!(CompileCache::from_bytes(&blob(&body(r#"["a","b"]"#, "a.api.yaml"), "")).len(), 1);
    assert!(CompileCache::from_bytes(&blob(&body("[]", "../a.api.yaml"), "")).is_empty());
    assert!(CompileCache::from_bytes(&blob(&body(r#"["a","a"]"#, "a.api.yaml"), "")).is_empty());
}

#[test]
fn a_unit_that_does_not_read_back_is_only_compiled_again() {
    let json = body("[]", "a.api.yaml").replace(r#""unit":null"#, r#""unit":9"#);
    let read = CompileCache::from_bytes(&blob(&json, "{not json"));
    assert_eq!((read.len(), read.units()), (1, 0));
}

#[test]
fn unit_lengths_that_do_not_match_the_units_read_as_empty() {
    let json = body("[]", "a.api.yaml").replace(r#""unit":null"#, r#""unit":9"#);
    assert!(CompileCache::from_bytes(&blob(&json, "{not jso")).is_empty(), "too short");
    assert!(CompileCache::from_bytes(&blob(&json, "{not json}")).is_empty(), "too long");
    assert!(CompileCache::from_bytes(&blob(&body("[]", "a.api.yaml"), "{}")).is_empty(), "left over");
}

fn codes(compiled: &Compiled) -> Vec<Code> {
    compiled.diagnostics.iter().map(|diagnostic| diagnostic.code).collect()
}

#[test]
fn a_cache_read_back_from_bytes_does_not_parse_unchanged_files_again() {
    const CONFIG: &str = "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n";
    let config = source("vibranium.config.yaml", CONFIG, 0);
    let api = "apis:\n  - id: getUser\n    path: /users\n";
    let first = compile(CompileInput { files: vec![config.clone(), source(API, api, 1)], ..CompileInput::default() });
    assert!(!codes(&first).contains(&Code::V1004));
    let previous = CompileCache::from_bytes(&first.cache.to_bytes());
    assert_eq!(previous.units(), 2);
    let files = vec![config.clone(), source(API, "nme: x\n", 1)];
    let same_hash = compile(CompileInput { files, previous: previous.clone() });
    assert_eq!(codes(&same_hash), codes(&first), "the stored unit was used, so the new text was not read");
    let changed = compile(CompileInput { files: vec![config, source(API, "nme: x\n", 2)], previous });
    assert!(codes(&changed).contains(&Code::V1004));
}
