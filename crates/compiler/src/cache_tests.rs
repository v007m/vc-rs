use std::sync::Arc;

use indexmap::IndexMap;
use vc_diagnostics::{Code, Diagnostics};
use vc_model::{ContentHash, SourceFile, SourceFileId, SourcePath, StringTable};

use super::{BUILD, Body, CacheEntry, CompileCache, MAGIC, Stored, VERSION};
use crate::parts::Tools;
use crate::stage::{self, FileUnit};
use crate::{CompileInput, Compiled};

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

/// A cache of `sources` (give them in path order, as a compile does) run through the per-file
/// stages, numbered in order, with a few names
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

/// A blob with this version's magic and version around `header`, then `units`.
fn blob(header: &[u8], units: &[u8]) -> Vec<u8> {
    let mut bytes = MAGIC.to_vec();
    bytes.extend(VERSION.to_le_bytes());
    bytes.extend(u32::try_from(header.len()).unwrap().to_le_bytes());
    bytes.extend(header);
    bytes.extend(units);
    bytes
}

/// A header with the string table `strings` and one file at `path` whose unit is `unit` bytes long.
fn body(strings: &[&str], path: &str, unit: Option<u32>) -> Vec<u8> {
    let stored = Stored { path: path.to_owned(), hash: [0; 32], file: 0, unit };
    let strings = strings.iter().map(|text| (*text).to_owned()).collect();
    postcard::to_stdvec(&Body { build: BUILD.to_owned(), strings, entries: vec![stored] }).unwrap()
}

#[test]
fn bytes_round_trip_with_the_units_and_names() {
    let cache = compiled(&[source(BAD, "nme: x\n", 2), source(API, API_TEXT, 1)]);
    let read = CompileCache::from_bytes(&cache.to_bytes());
    assert_eq!(read, cache);
    let unit = kept(&read, API, 1, 1).unwrap();
    assert!(unit.root.is_some() && !unit.parts.is_empty() && unit.diagnostics.is_empty());
    assert_eq!(Some(&*unit), kept(&cache, API, 1, 1).as_deref());
    assert_eq!((cache.units(), read.units()), (2, 1));
    assert_eq!((read.len(), read.is_empty(), CompileCache::new().len()), (2, false, 0));
    assert!(kept(&cache, BAD, 2, 0).is_some_and(|bad| !bad.diagnostics.is_empty()));
    assert!(kept(&read, BAD, 2, 0).is_none(), "a unit with diagnostics is compiled again");
    assert_eq!(CompileCache::from_bytes(&CompileCache::new().to_bytes()), CompileCache::new());
}

#[test]
fn bytes_round_trip_numbers_and_literal_arguments() {
    let body = "    body:\n      age: 1.5\n      count: -3\n      name: \"${fake.lorem.words(5) | default('x')}\"\n";
    let text = format!("apis:\n  - id: createUser\n    method: POST\n    path: /users\n{body}");
    let cache = compiled(&[source(API, &text, 1)]);
    let unit = kept(&cache, API, 1, 0).unwrap();
    assert!(unit.diagnostics.is_empty(), "{:?}", unit.diagnostics);
    let read = CompileCache::from_bytes(&cache.to_bytes());
    assert_eq!(kept(&read, API, 1, 0).as_deref(), Some(&*unit));
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
    for version in [1, 2, 3, 4, 5] {
        let mut bytes = cache().to_bytes();
        bytes[MAGIC.len()] = version;
        assert!(CompileCache::from_bytes(&bytes).is_empty(), "version {version}");
    }
}

#[test]
fn a_blob_from_another_release_reads_as_empty() {
    let stored = Stored { path: "a.api.yaml".to_owned(), hash: [0; 32], file: 0, unit: None };
    let other = Body { build: "0.0.0-other".to_owned(), strings: Vec::new(), entries: vec![stored] };
    assert!(CompileCache::from_bytes(&blob(&postcard::to_stdvec(&other).unwrap(), &[])).is_empty());
    assert_eq!(CompileCache::from_bytes(&blob(&body(&[], "a.api.yaml", None), &[])).len(), 1);
}

#[test]
fn finds_each_unit_by_its_path_in_path_order() {
    let cache = cache();
    assert!(kept(&cache, "services/b.test.yaml", 2, 1).is_some());
    assert!(kept(&cache, "services/b.test.yaml", 2, 0).is_none());
    assert!(kept(&cache, "m.api.yaml", 1, 0).is_none() && kept(&cache, "z.api.yaml", 2, 1).is_none());
}

#[test]
fn a_cache_whose_unit_did_not_read_back_differs_from_a_fresh_compile() {
    let fresh = compiled(&[source(API, API_TEXT, 1)]);
    let mut bytes = fresh.to_bytes();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xFF;
    let damaged = CompileCache::from_bytes(&bytes);
    assert_eq!((damaged.len(), damaged.units()), (1, 0));
    assert_ne!(damaged, fresh, "so the check saves the fresh cache");
    assert_eq!(CompileCache::from_bytes(&fresh.to_bytes()), fresh);
    let with_errors = compiled(&[source(BAD, "nme: x\n", 2)]);
    assert_eq!(
        CompileCache::from_bytes(&with_errors.to_bytes()),
        with_errors,
        "units with diagnostics are never stored"
    );
}

#[test]
fn damaged_bytes_read_as_empty() {
    let bytes = compiled(&[source(API, API_TEXT, 1)]).to_bytes();
    assert_eq!(CompileCache::from_bytes(&bytes).len(), 1);
    for cut in [0, 3, 7, HEADER + 1, bytes.len() - 1] {
        assert!(CompileCache::from_bytes(&bytes[..cut]).is_empty(), "cut at {cut}");
    }
    let mut bad_header = bytes.clone();
    bad_header[HEADER] = 0xFF;
    assert!(CompileCache::from_bytes(&bad_header).is_empty());
    let mut long_header = bytes;
    long_header[MAGIC.len() + 4] += 1;
    assert!(CompileCache::from_bytes(&long_header).is_empty());
    assert!(CompileCache::from_bytes(b"not a cache at all").is_empty());
}

#[test]
fn a_path_or_string_table_that_does_not_read_back_reads_as_empty() {
    assert_eq!(CompileCache::from_bytes(&blob(&body(&["a", "b"], "a.api.yaml", None), &[])).len(), 1);
    assert!(CompileCache::from_bytes(&blob(&body(&[], "/a.api.yaml", None), &[])).is_empty());
    assert_eq!(
        CompileCache::from_bytes(&blob(&body(&[], "../shared/a.csv", None), &[])).len(),
        1,
        "data may be outside"
    );
    assert!(CompileCache::from_bytes(&blob(&body(&["a", "a"], "a.api.yaml", None), &[])).is_empty());
}

#[test]
fn a_unit_that_does_not_read_back_is_only_compiled_again() {
    let header = body(&[], "a.api.yaml", Some(9));
    let read = CompileCache::from_bytes(&blob(&header, &[0xFF; 9]));
    assert_eq!((read.len(), read.units()), (1, 0));
    let unit = postcard::to_stdvec(&*entry("a.api.yaml", 0, 0).unit.unwrap()).unwrap();
    let mut trailing = unit.clone();
    trailing.push(0);
    let header = body(&[], "a.api.yaml", Some(u32::try_from(trailing.len()).unwrap()));
    assert_eq!(CompileCache::from_bytes(&blob(&header, &trailing)).units(), 0, "bytes after the unit");
    let header = body(&[], "a.api.yaml", Some(u32::try_from(unit.len()).unwrap()));
    assert_eq!(CompileCache::from_bytes(&blob(&header, &unit)).units(), 1);
}

#[test]
fn unit_lengths_that_do_not_match_the_units_read_as_empty() {
    let header = body(&[], "a.api.yaml", Some(9));
    assert!(CompileCache::from_bytes(&blob(&header, &[0; 8])).is_empty(), "too short");
    assert!(CompileCache::from_bytes(&blob(&header, &[0; 10])).is_empty(), "too long");
    assert!(CompileCache::from_bytes(&blob(&body(&[], "a.api.yaml", None), &[0; 2])).is_empty(), "left over");
}

fn codes(compiled: &Compiled) -> Vec<Code> {
    compiled.diagnostics.iter().map(|diagnostic| diagnostic.code).collect()
}

#[test]
fn a_cache_read_back_from_bytes_does_not_parse_unchanged_files_again() {
    const CONFIG: &str = "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n";
    let config = source("vibranium.config.yaml", CONFIG, 0);
    let api = "apis:\n  - id: getUser\n    path: /users\n";
    let first =
        crate::compile(CompileInput { files: vec![config.clone(), source(API, api, 1)], ..CompileInput::default() });
    assert!(!codes(&first).contains(&Code::V1004));
    let previous = CompileCache::from_bytes(&first.cache.to_bytes());
    assert_eq!(previous.units(), 2);
    let files = vec![config.clone(), source(API, "nme: x\n", 1)];
    let same_hash = crate::compile(CompileInput { files, previous: previous.clone(), ..CompileInput::default() });
    assert_eq!(codes(&same_hash), codes(&first), "the stored unit was used, so the new text was not read");
    let changed = crate::compile(CompileInput {
        files: vec![config, source(API, "nme: x\n", 2)],
        previous,
        ..CompileInput::default()
    });
    assert!(codes(&changed).contains(&Code::V1004));
}
