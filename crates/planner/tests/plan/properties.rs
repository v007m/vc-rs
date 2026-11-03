//! Properties over generated projects: files with or without a `beforeAll`, tests with random
//! tags, repeats, skips and reads of the shared actions.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use proptest::prelude::*;
use vc_model::{Plan, Program};
use vc_planner::{Selection, plan};

use crate::common::{USERS, compile_files, name, today};

const CONFIG: &str = "syntaxVersion: 2\nprofiles: [{id: dev, default: true}]\nvars: {baseUrl: 'http://x'}\nbeforeRun:\n  - id: warm\n    call: users#list\n";
const TAGS: &[&str] = &["a", "b", "c"];
const EXPRS: &[&str] = &["a", "!b", "a & !c", "b,c"];

#[derive(Clone, Debug)]
struct GenTest {
    tags: BTreeSet<usize>,
    repeat: u32,
    reads: u8,
    skip: bool,
}

#[derive(Clone, Debug)]
struct GenFile {
    shared: bool,
    tests: Vec<GenTest>,
}

fn gen_test() -> impl Strategy<Value = GenTest> {
    (prop::collection::btree_set(0..TAGS.len(), 0..3), 1..3_u32, 0..3_u8, any::<bool>())
        .prop_map(|(tags, repeat, reads, skip)| GenTest { tags, repeat, reads, skip })
}

fn gen_files() -> impl Strategy<Value = Vec<GenFile>> {
    let file =
        (any::<bool>(), prop::collection::vec(gen_test(), 1..4)).prop_map(|(shared, tests)| GenFile { shared, tests });
    prop::collection::vec(file, 1..4)
}

fn yaml(index: usize, file: &GenFile) -> String {
    let mut text = format!("name: G{index}\n");
    if file.shared {
        let _ = write!(text, "beforeAll:\n  - id: s{index}\n    call: users#list\n");
    }
    text.push_str("tests:\n");
    for (at, test) in file.tests.iter().enumerate() {
        let _ = write!(text, "  - id: runs{at}\n    call: users#list\n    repeat: {}\n", test.repeat);
        if !test.tags.is_empty() {
            let tags: Vec<&str> = test.tags.iter().map(|tag| TAGS[*tag]).collect();
            let _ = writeln!(text, "    tags: [{}]", tags.join(", "));
        }
        match (test.reads, file.shared) {
            (1, _) | (2, false) => text.push_str("    headers: {x: '${var.warm.token}'}\n"),
            (2, true) => {
                let _ = writeln!(text, "    headers: {{x: '${{var.s{index}.token}}'}}");
            }
            _ => {}
        }
        if test.skip {
            text.push_str("    skip: {reason: r}\n");
        }
    }
    text
}

fn generated(files: &[GenFile]) -> Program {
    let texts: Vec<(String, String)> =
        files.iter().enumerate().map(|(at, file)| (format!("services/g{at}/t.test.yaml"), yaml(at, file))).collect();
    let mut sources = vec![("vibranium.config.yaml", CONFIG), ("services/users/users.api.yaml", USERS)];
    sources.extend(texts.iter().map(|(path, text)| (path.as_str(), text.as_str())));
    compile_files(&sources)
}

/// Test IDs, file IDs and a glob, for picking selectors from.
fn pool(program: &Program) -> Vec<String> {
    let tests = program.tests.keys().map(|test| name(program, test.sym()).to_owned());
    let files = program.test_files.keys().map(|file| name(program, file.sym()).to_owned());
    tests.chain(files).chain(["g*".to_owned()]).collect()
}

fn selection(program: &Program, picks: &[usize], expr: Option<usize>, include_skipped: bool) -> Selection {
    let pool = pool(program);
    let ids = picks.iter().map(|pick| pool[pick % pool.len()].clone()).collect();
    let tags = expr.map(|expr| EXPRS[expr].to_owned()).into_iter().collect();
    Selection { ids, tags, include_skipped, ..Selection::default() }
}

fn run(program: &Program, selection: &Selection) -> Plan {
    plan(program, selection, &today()).unwrap()
}

fn planned_tests(program: &Program, plan: &Plan) -> Vec<String> {
    let mut tests: Vec<String> = Vec::new();
    for instance in plan.files.iter().flat_map(|file| &file.instances) {
        let test = name(program, instance.test.sym()).to_owned();
        if !tests.contains(&test) {
            tests.push(test);
        }
    }
    tests
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn every_need_is_planned_before_it_is_read(
        files in gen_files(),
        picks in prop::collection::vec(0..32_usize, 0..3),
        expr in prop::option::of(0..EXPRS.len()),
        include_skipped: bool,
    ) {
        let program = generated(&files);
        let plan = run(&program, &selection(&program, &picks, expr, include_skipped));
        let run_wide: BTreeSet<_> = plan.before_run.iter().map(|planned| planned.action).collect();
        for file in &plan.files {
            let mut in_scope = run_wide.clone();
            in_scope.extend(file.before_all.iter().map(|planned| planned.action));
            for instance in &file.instances {
                prop_assert!(instance.needs.iter().all(|need| in_scope.contains(&need)));
            }
        }
    }

    #[test]
    fn no_lifecycle_runs_without_an_instance(
        files in gen_files(),
        picks in prop::collection::vec(0..32_usize, 0..3),
        expr in prop::option::of(0..EXPRS.len()),
    ) {
        let program = generated(&files);
        let plan = run(&program, &selection(&program, &picks, expr, false));
        if plan.files.is_empty() {
            prop_assert!(plan.before_run.is_empty() && plan.after_run.is_empty());
        }
        prop_assert!(plan.files.iter().all(|file| !file.instances.is_empty()));
    }

    #[test]
    fn selecting_the_selected_tests_again_selects_them(
        files in gen_files(),
        picks in prop::collection::vec(0..32_usize, 0..3),
        expr in prop::option::of(0..EXPRS.len()),
        include_skipped: bool,
    ) {
        let program = generated(&files);
        let first = selection(&program, &picks, expr, include_skipped);
        let plan = run(&program, &first);
        prop_assert_eq!(&run(&program, &first), &plan);
        let tests = planned_tests(&program, &plan);
        if !tests.is_empty() {
            let again = Selection { ids: tests.clone(), include_skipped, ..Selection::default() };
            prop_assert_eq!(planned_tests(&program, &run(&program, &again)), tests);
        }
    }

    #[test]
    fn selecting_a_file_is_selecting_its_tests(files in gen_files(), pick in 0..4_usize, include_skipped: bool) {
        let program = generated(&files);
        let file = program.test_files.values().nth(pick % program.test_files.len()).unwrap();
        let by_file = Selection { ids: vec![name(&program, file.id.sym()).to_owned()], include_skipped, ..Selection::default() };
        let tests = file.tests.iter().map(|test| name(&program, test.sym()).to_owned()).collect();
        let by_tests = Selection { ids: tests, include_skipped, ..Selection::default() };
        prop_assert_eq!(run(&program, &by_file), run(&program, &by_tests));
    }
}
