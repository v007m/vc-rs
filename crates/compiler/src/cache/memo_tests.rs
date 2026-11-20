use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use vc_model::{ContentHash, Date, FileKind, SourceFile, SourcePath};

use super::LinkKey;
use crate::cache::{BUILD, Body, MAGIC, Recall, SUM_AT, SUMMED, seal};
use crate::{CompileCache, CompileInput, Compiled, compile};

#[path = "../../tests/common/tree.rs"]
mod tree;

use tree::read_tree;

type Outcome = Result<(), Box<dyn Error>>;
/// A compile in serialized form: the program without its files, the files, the diagnostics as
/// JSON, and the files wanted.
type Serialized = (Option<Vec<u8>>, String, String, String);
/// A project's last compile round: its input and output.
type Settled = Result<(CompileInput, Compiled), Box<dyn Error>>;
/// Whether a project's hit replayed a warning, so a project with warnings still hits.
type Replayed = Result<bool, Box<dyn Error>>;

/// The day the golden cases compile on.
const TODAY: &str = "2026-11-03";
const CONFIG: &str = "vibranium.config.yaml";
const CONFIG_TEXT: &str = "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n";
const API: &str = "services/users/users.api.yaml";
const API_TEXT: &str = "apis:\n  - id: getUser\n    path: /users\n";

fn source(path: &str, text: &str, byte: u8) -> SourceFile {
    SourceFile { path: SourcePath::new(path).unwrap(), text: text.to_owned(), hash: ContentHash::new([byte; 32]) }
}

fn key(input: &CompileInput) -> LinkKey {
    LinkKey::of(input.today, input.assets.as_ref(), input.secret_schema.as_ref())
}

/// Whether `cache` would give back its output for `compiled`'s files under `input`'s key.
fn hits(cache: &CompileCache, compiled: &Compiled, input: &CompileInput) -> bool {
    matches!(cache.replay(&compiled.files, &key(input)), Recall::Hit(_))
}

/// `compiled` in serialized form, to compare one compile with another.
fn serialized(compiled: &Compiled) -> Serialized {
    let program = compiled.program.as_deref().map(|program| postcard::to_stdvec(program).unwrap());
    let files = format!("{:?} {:?}", compiled.files, compiled.program.as_ref().map(|program| &program.files));
    let diagnostics = serde_json::to_string(&compiled.diagnostics).unwrap();
    (program, files, diagnostics, format!("{:?} {:?}", compiled.data_wanted, compiled.hooks_wanted))
}

/// The project at `dir` compiled as the golden harness does, again with the data and hook
/// files it wants until it wants none: the last round's input and output.
fn settled(dir: &Path) -> Settled {
    let mut read = Vec::new();
    read_tree(dir, dir, &mut read)?;
    let (files, mut others): (Vec<_>, Vec<_>) =
        read.into_iter().partition(|file| FileKind::from_path(&file.path).is_some());
    let assets = Some(others.iter().map(|file| file.path.clone()).collect());
    let secret_schema = others.iter().find(|file| file.path.as_str() == ".env.schema").cloned();
    let mut input =
        CompileInput { files, previous: CompileCache::new(), today: Date::parse(TODAY), assets, secret_schema };
    loop {
        let compiled = compile(input.clone());
        let data = compiled.data_wanted.iter().map(|data| &data.path);
        let wanted: Vec<SourcePath> = data.chain(&compiled.hooks_wanted).cloned().collect();
        let (more, rest): (Vec<_>, Vec<_>) = others.into_iter().partition(|file| wanted.contains(&file.path));
        if more.is_empty() {
            return Ok((input, compiled));
        }
        (others, input.previous) = (rest, compiled.cache);
        input.files.extend(more);
    }
}

/// A hit gives back what a fresh link of the project at `dir` gives, from memory and from
/// bytes; a compile with errors is never memoized.
fn replays_a_fresh_link(dir: &Path) -> Replayed {
    let (input, fresh) = settled(dir)?;
    let read = CompileCache::from_bytes(&fresh.cache.to_bytes());
    let name = dir.display();
    if fresh.program.is_none() {
        assert!(matches!(fresh.cache.replay(&fresh.files, &key(&input)), Recall::Miss), "{name} has errors");
        return Ok(false);
    }
    for previous in [fresh.cache.clone(), read] {
        assert!(hits(&previous, &fresh, &input), "{name}");
        let again = compile(CompileInput { previous, ..input.clone() });
        assert_eq!(serialized(&again), serialized(&fresh), "{name}");
    }
    Ok(!fresh.diagnostics.is_empty())
}

#[test]
fn a_hit_equals_a_fresh_link_for_every_golden_case_and_the_docs_examples() -> Outcome {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut cases = fs::read_dir(root.join("tests/cases"))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    cases.sort();
    assert!(cases.len() > 50, "the golden cases are there");
    let mut warned = 0;
    for case in cases {
        warned += usize::from(replays_a_fresh_link(&case.join("project"))?);
    }
    replays_a_fresh_link(&root.join("../../site/src/examples"))?;
    assert_eq!(warned, 9, "the golden cases with warnings hit too, and replay them");
    Ok(())
}

/// A small clean project's input: a config and an api, with today, assets and a schema.
fn base() -> CompileInput {
    let assets = BTreeSet::from([SourcePath::new("data/users.csv").unwrap()]);
    CompileInput {
        files: vec![source(CONFIG, CONFIG_TEXT, 1), source(API, API_TEXT, 2)],
        previous: CompileCache::new(),
        today: Date::parse(TODAY),
        assets: Some(assets),
        secret_schema: Some(source(".env.schema", "API_TOKEN=\n", 3)),
    }
}

/// The cache a compile of [`base`] leaves, read back from its bytes.
fn warm() -> CompileCache {
    let first = compile(base());
    assert!(first.program.is_some(), "{:?}", first.diagnostics);
    CompileCache::from_bytes(&first.cache.to_bytes())
}

/// After `change` to [`base`], the warm cache misses, and the compile equals one with no cache.
fn misses(change: impl Fn(&mut CompileInput)) {
    let mut input = base();
    change(&mut input);
    let cold = compile(input.clone());
    let previous = warm();
    assert!(!hits(&previous, &cold, &input), "a miss");
    let again = compile(CompileInput { previous, ..input });
    assert_eq!(serialized(&again), serialized(&cold));
}

#[test]
fn the_same_files_and_world_hit() {
    let previous = warm();
    let fresh = compile(base());
    assert!(hits(&previous, &fresh, &base()));
    let again = compile(CompileInput { previous: previous.clone(), ..base() });
    assert_eq!(again.cache, previous, "a hit leaves the cache as it was, so it is not saved again");
}

#[test]
fn a_changed_file_misses() {
    misses(|input| input.files[1] = source(API, "apis:\n  - id: getUser\n    path: /people\n", 9));
}

#[test]
fn a_changed_config_or_profile_misses() {
    misses(|input| input.files[0] = source(CONFIG, &format!("{CONFIG_TEXT}  - id: staging\n"), 9));
}

#[test]
fn an_added_file_misses() {
    misses(|input| input.files.push(source("services/orders/orders.api.yaml", API_TEXT, 4)));
}

#[test]
fn a_removed_file_misses() {
    misses(|input| {
        input.files.pop();
    });
}

#[test]
fn another_day_misses() {
    misses(|input| input.today = Date::parse("2026-11-04"));
    misses(|input| input.today = None);
}

#[test]
fn other_assets_miss() {
    misses(|input| input.assets = Some(BTreeSet::new()));
    misses(|input| input.assets = None);
}

#[test]
fn another_secret_schema_misses() {
    misses(|input| input.secret_schema = Some(source(".env.schema", "API_TOKEN=\nOTHER=\n", 8)));
    misses(|input| input.secret_schema = None);
}

#[test]
fn a_damaged_output_is_a_silent_miss_that_is_saved_again() {
    let mut bytes = compile(base()).cache.to_bytes();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xFF;
    seal(&mut bytes);
    let damaged = CompileCache::from_bytes(&bytes);
    let fresh = compile(base());
    assert!(matches!(damaged.replay(&fresh.files, &key(&base())), Recall::Damaged));
    let healed = compile(CompileInput { previous: damaged.clone(), ..base() });
    assert_eq!(serialized(&healed), serialized(&fresh));
    assert_ne!(healed.cache, damaged, "so it is saved");
    assert!(hits(&CompileCache::from_bytes(&healed.cache.to_bytes()), &fresh, &base()));
    let cut = CompileCache::from_bytes(&bytes[..last]);
    assert!(cut.is_empty(), "a cut blob reads as empty");
}

/// The blob of a compile of [`base`], after `change`, misses, and the compile equals one with no
/// cache.
fn blob_misses(change: impl Fn(&mut Vec<u8>)) {
    let mut bytes = compile(base()).cache.to_bytes();
    change(&mut bytes);
    let previous = CompileCache::from_bytes(&bytes);
    let cold = compile(base());
    assert!(!hits(&previous, &cold, &base()), "a miss");
    let again = compile(CompileInput { previous, ..base() });
    assert_eq!(serialized(&again), serialized(&cold));
}

#[test]
fn another_layout_version_misses() {
    blob_misses(|bytes| bytes[MAGIC.len()..MAGIC.len() + 4].copy_from_slice(&12_u32.to_le_bytes()));
}

/// `bytes` with its header saying that `build` wrote it, sealed again.
fn written_by(bytes: &mut Vec<u8>, build: &str) {
    let at = SUMMED + 4;
    let length = usize::try_from(u32::from_le_bytes(bytes[SUMMED..at].try_into().unwrap())).unwrap();
    let mut body: Body = postcard::from_bytes(&bytes[at..at + length]).unwrap();
    body.build = build.to_owned();
    let header = postcard::to_stdvec(&body).unwrap();
    let mut out = bytes[..SUMMED].to_vec();
    out.extend(u32::try_from(header.len()).unwrap().to_le_bytes());
    out.extend(header);
    out.extend(&bytes[at + length..]);
    seal(&mut out);
    *bytes = out;
}

#[test]
fn another_release_misses() {
    blob_misses(|bytes| written_by(bytes, "0.0.0-other"));
}

#[test]
fn another_build_of_the_same_release_misses() {
    let id = env!("VC_LINK_BUILD_ID");
    assert_eq!(BUILD, format!("{}+{id}", env!("CARGO_PKG_VERSION")));
    assert!(id.len() == 64 && id.bytes().all(|byte| byte.is_ascii_hexdigit()), "a blake3 hash: {id}");
    blob_misses(|bytes| written_by(bytes, &format!("{}+{}", env!("CARGO_PKG_VERSION"), "0".repeat(64))));
    let mut same = compile(base()).cache.to_bytes();
    written_by(&mut same, BUILD);
    assert!(hits(&CompileCache::from_bytes(&same), &compile(base()), &base()), "the same build still hits");
}

#[test]
fn a_truncated_or_corrupt_blob_is_a_silent_miss() {
    let length = compile(base()).cache.to_bytes().len();
    for cut in [0, 3, MAGIC.len() + 6, length / 2, length - 1] {
        blob_misses(|bytes| bytes.truncate(cut));
    }
    blob_misses(|bytes| bytes[..MAGIC.len()].copy_from_slice(b"XXXX"));
    blob_misses(|bytes| bytes.push(0));
}

#[test]
fn a_blob_whose_checksum_does_not_hold_is_a_silent_miss_that_is_rebuilt() {
    let bytes = compile(base()).cache.to_bytes();
    assert!(hits(&CompileCache::from_bytes(&bytes), &compile(base()), &base()), "an intact blob hits");
    for at in [SUM_AT, SUMMED - 1, SUMMED, SUMMED + 4, bytes.len() / 2, bytes.len() - 1] {
        let mut flipped = bytes.clone();
        flipped[at] ^= 0x01;
        assert!(CompileCache::from_bytes(&flipped).is_empty(), "a byte flipped at {at}");
        blob_misses(|bytes| bytes[at] ^= 0x01);
    }
}

#[test]
fn a_compile_with_errors_is_not_memoized() {
    let input = CompileInput { files: vec![source(API, "nme: x\n", 2)], ..base() };
    let compiled = compile(input.clone());
    assert!(compiled.program.is_none());
    assert!(matches!(compiled.cache.replay(&compiled.files, &key(&input)), Recall::Miss));
}

#[test]
fn a_hit_in_memory_shares_the_program() {
    let first = compile(base());
    let again = compile(CompileInput { previous: first.cache.clone(), ..base() });
    let (program, shared) = (first.program.unwrap(), again.program.unwrap());
    assert!(Arc::ptr_eq(&program, &shared));
}

#[test]
fn a_hit_hands_out_the_program_it_made_or_one_holding_the_files_it_was_given() {
    let first = compile(base());
    let again = compile(CompileInput { previous: first.cache.clone(), ..base() });
    assert!(hits(&first.cache, &again, &base()), "the same paths, hashes and numbers");
    let (made, replayed) = (first.program.clone().unwrap(), again.program.unwrap());
    assert!(Arc::ptr_eq(&made, &replayed), "a memo made in this process hands out its own program");
    let previous = CompileCache::from_bytes(&first.cache.to_bytes());
    let read = compile(CompileInput { previous, ..base() });
    assert_eq!(read.program.map(|program| program.files.clone()), Some(read.files));
}
