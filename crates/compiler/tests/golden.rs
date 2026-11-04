//! Golden cases: each `tests/cases/<case>/project` is compiled, and its diagnostics, rendered
//! as text and as JSON, are compared with the case's `expected.txt.snap` and `expected.json.snap`.
//! Definition files are compiled; every other file in the project is an asset `$file` may name.
//! `cargo insta review` updates them after a deliberate change.

use std::error::Error;
use std::fs;
use std::path::Path;

use vc_compiler::{CompileCache, CompileInput, compile};
use vc_diagnostics::{JsonRenderer, Render, TextRenderer};
use vc_model::{ContentHash, Date, FileKind, SourceFile, SourcePath};

type Outcome = Result<(), Box<dyn Error>>;

/// The day every case is compiled on, so expired skips (V3104) do not depend on the calendar.
const TODAY: &str = "2026-11-03";

/// Every file under `dir`, with its path relative to `root`.
fn read_tree(root: &Path, dir: &Path, files: &mut Vec<SourceFile>) -> Outcome {
    let mut entries = fs::read_dir(dir)?.map(|entry| entry.map(|entry| entry.path())).collect::<Result<Vec<_>, _>>()?;
    entries.sort();
    for path in entries {
        if path.is_dir() {
            read_tree(root, &path, files)?;
            continue;
        }
        let relative = path.strip_prefix(root)?.to_string_lossy().replace('\\', "/");
        let text = fs::read_to_string(&path)?;
        files.push(SourceFile { path: SourcePath::new(&relative)?, text, hash: ContentHash::new([0; 32]) });
    }
    Ok(())
}

fn golden(case: &str) -> Outcome {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cases").join(case);
    let project = dir.join("project");
    let mut read = Vec::new();
    read_tree(&project, &project, &mut read)?;
    let (files, others): (Vec<_>, Vec<_>) =
        read.into_iter().partition(|file| FileKind::from_path(&file.path).is_some());
    let input = CompileInput {
        files,
        previous: CompileCache::new(),
        today: Date::parse(TODAY),
        assets: Some(others.into_iter().map(|file| file.path).collect()),
    };
    let mut compiled = compile(input);
    compiled.diagnostics.sort();
    let text = TextRenderer::default().render(&compiled.diagnostics, &compiled.files)?;
    let json = JsonRenderer.render(&compiled.diagnostics, &compiled.files)?;
    assert_eq!(compiled.program.is_some(), !compiled.diagnostics.has_errors());
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_path(dir);
    settings.set_prepend_module_to_snapshot(false);
    settings.set_omit_expression(true);
    settings.set_input_file(project);
    settings.bind(|| {
        insta::assert_snapshot!("expected.txt", text);
        insta::assert_snapshot!("expected.json", json);
    });
    Ok(())
}

macro_rules! cases {
    ($($case:ident),* $(,)?) => {
        $(
            #[test]
            fn $case() -> Outcome {
                golden(stringify!($case))
            }
        )*
    };
}

cases!(
    clean_sample,
    v1001_invalid_yaml,
    v1002_structure,
    v1003_missing_version,
    v1003_newer_version,
    v1004_unknown_key,
    v1005_missing_key,
    v1005_poll_keys,
    v1006_duplicate_key,
    v1007_alias,
    v1008_tag,
    v1009_complex_key,
    v1010_two_documents,
    v1011_too_deep,
    v2201_one_of,
    v2301_bad_unit,
    v2302_wrong_type,
    v2303_bad_status,
    v2305_template_syntax,
    v2306_matcher_arg,
    v2308_unknown_pipe,
    v2105_unavailable_root,
    v2102_loop_root,
    cel_v2305_syntax,
    cel_v2306_regex,
    cel_v2105_root,
    cel_v2102_loop_root,
    v2001_duplicate_id,
    v2002_unknown_profile,
    v2003_unknown_api,
    v2006_reserved_id,
    v2004_file_not_found,
    v2007_file_id,
    v2101_unset_var,
    v2103_example_column,
    v2105_step_order,
    v2205_cycle,
    v2210_shared_without_id,
    v2304_pipe_type,
    v3101_no_profile,
    v3102_default_profiles,
    v3103_skip_reason,
    v3104_expired_skip,
    v3201_shared_write,
    v3202_test_cache,
    v3203_deprecated_call,
    v3205_body_on_read,
    v3206_unused_default,
);
