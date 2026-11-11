use std::sync::Arc;

use vc_model::{Action, AllowFailure, ContentHash, Program, SourceFile, SourcePath, Test, TestBody};

use crate::CompileInput;

const CONFIG: &str = "syntaxVersion: 2\nvars: {baseUrl: 'http://x'}\nprofiles:\n  - id: dev\n    default: true\n";

const APIS: &str = "baseUrl: ${var.baseUrl}\napis:\n  - id: create\n    method: POST\n    path: /orders\n";

const ALLOWED: &str = "allowFailure: {reason: sandbox is down, until: 2099-03-31}\ntests:\n  - id: makesOrder\n    call: orders#create\n    allowFailure: true\n  - id: makesAnother\n    call: orders#create\n    allowFailure: false\n  - id: makesAThird\n    call: orders#create\n";

const REUSED: &str = "tests:\n  - id: savesOrder\n    standalone: false\n    call: orders#create\n    save: {madeId: '${response.body.id}'}\n  - id: reusesTheOrder\n    steps:\n      - test: savesOrder\n      - call: orders#create\n        body: {id: '${var.madeId}'}\n        save: {otherId: '${response.body.id}'}\n";

fn program(tests: &str) -> Arc<Program> {
    let file = |path: &str, text: &str| SourceFile {
        path: SourcePath::new(path).unwrap(),
        text: text.to_owned(),
        hash: ContentHash::new([0; 32]),
    };
    let files = vec![
        file("vibranium.config.yaml", CONFIG),
        file("services/orders/orders.api.yaml", APIS),
        file("services/orders/flow.test.yaml", tests),
    ];
    let compiled = crate::compile(CompileInput { files, ..CompileInput::default() });
    assert!(!compiled.diagnostics.has_errors(), "{:?}", compiled.diagnostics);
    compiled.program.unwrap()
}

fn test<'p>(program: &'p Program, id: &str) -> &'p Test {
    program.tests.values().find(|test| program.str(test.id.sym()) == Some(id)).unwrap()
}

fn names(program: &Program, action: &Action) -> Vec<String> {
    action.implied.iter().map(|name| program.str(name.sym()).unwrap().to_owned()).collect()
}

/// D248: the file's `allowFailure` keeps its reason and date; `true` allows with no reason, and
/// `false` or no key allows none on the test itself.
#[test]
fn reads_allow_failure_on_the_file_and_on_each_test() {
    let program = program(ALLOWED);
    let file = program.test_files.values().next().unwrap();
    let reason = Some("sandbox is down".to_owned());
    let expected = AllowFailure { reason, until: Some("2099-03-31".to_owned()) };
    assert_eq!(file.allow_failure.as_ref(), Some(&expected));
    let own = |id: &str| test(&program, id).allow_failure.clone();
    assert_eq!(own("orders.flow#makesOrder"), Some(AllowFailure::default()));
    assert_eq!(own("orders.flow#makesAnother"), None);
    assert_eq!(own("orders.flow#makesAThird"), None);
}

/// D282: a `test:` step is seen to save what its test saves; any other step implies nothing.
#[test]
fn a_test_step_implies_the_saves_of_its_test_and_a_call_step_implies_none() {
    let program = program(REUSED);
    let TestBody::Steps(steps) = &test(&program, "orders.flow#reusesTheOrder").body else { panic!("expected steps") };
    let action = |index: usize| &program.actions[usize::try_from(steps[index].index()).unwrap()];
    assert_eq!(names(&program, action(0)), ["madeId"]);
    assert_eq!(names(&program, action(1)), Vec::<String>::new());
}
