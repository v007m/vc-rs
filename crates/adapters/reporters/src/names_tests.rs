use std::sync::Arc;

use vc_model::{
    ActionId, CheckKind, InstanceId, Names, OwnerKey, Path, Plan, Pos, Program, Segment, SourceFileId, SourceFiles,
    SourceSpan, StringTable, TestId, TestResult, TestStatus, check_label,
};

use crate::testing;

/// Names over the sample project.
fn sample() -> Names {
    let (program, plan) = testing::project();
    Names::new(program, &plan)
}

fn test_id(names: &Names, short: &str) -> TestId {
    let full = format!("users.crud#{short}");
    names.program().tests.values().find(|t| names.test(t.id) == full).unwrap().id
}

fn result(test: TestId, instance: Option<u32>) -> TestResult {
    TestResult {
        test,
        instance: instance.map(InstanceId::new),
        status: TestStatus::Passed,
        attempts: 1,
        failure: None,
        skip_reason: None,
    }
}

/// An ID from a table the empty program does not have.
fn stranger() -> (Names, TestId) {
    let mut table = StringTable::new();
    let test = TestId::new(table.intern("orders.checkout#placesOrder").unwrap());
    let program = Program::new(StringTable::new(), SourceFiles::new());
    (Names::new(Arc::new(program), &Plan::default()), test)
}

#[test]
fn names_tests_files_and_instances() {
    let names = sample();
    let rejects = test_id(&names, "rejectsBadEmail");
    let lists = test_id(&names, "listsUsers");
    assert_eq!(names.test(rejects), "users.crud#rejectsBadEmail");
    assert_eq!(names.file_of(rejects), "users.crud");
    assert_eq!(names.file(names.program().tests[&rejects].file), "users.crud");
    assert_eq!(names.instance(&result(rejects, Some(2))), "users.crud#rejectsBadEmail [row 2]");
    assert_eq!(names.instance(&result(lists, Some(3))), "users.crud#listsUsers [repeat 1]");
    assert_eq!(names.instance(&result(lists, Some(4))), "users.crud#listsUsers [repeat 2]");
    assert_eq!(names.instance(&result(lists, None)), "users.crud#listsUsers");
    let gets = test_id(&names, "getsUser");
    assert_eq!(names.instance(&result(gets, Some(0))), "users.crud#getsUser");
}

#[test]
fn falls_back_for_ids_the_program_does_not_know() {
    let (names, test) = stranger();
    assert_eq!(names.test(test), "?");
    assert_eq!(names.file_of(test), "?");
    assert_eq!(names.instance(&result(test, Some(7))), "?");
    assert_eq!(names.action(ActionId::new(9)), "action 9");
    assert_eq!(names.test_location(test), None);
    assert_eq!(names.tags(test), Vec::<String>::new());
    let span = SourceSpan::new(SourceFileId::new(3), Pos::new(1, 1, 0), Pos::new(1, 2, 1));
    assert_eq!(names.location(span), None);
}

#[test]
fn takes_the_file_from_a_full_id_the_program_has_as_text_only() {
    let mut table = StringTable::new();
    let test = TestId::new(table.intern("orders.checkout#placesOrder").unwrap());
    let names = Names::new(Arc::new(Program::new(table, SourceFiles::new())), &Plan::default());
    assert_eq!(names.file_of(test), "orders.checkout");
    let mut table = StringTable::new();
    let bare = TestId::new(table.intern("loose").unwrap());
    let names = Names::new(Arc::new(Program::new(table, SourceFiles::new())), &Plan::default());
    assert_eq!(names.file_of(bare), "loose");
}

#[test]
fn names_actions_owners_and_places() {
    let names = sample();
    let (program, plan) = testing::project();
    let seed = plan.files[0].before_all[0].action;
    assert_eq!(names.action(seed), "seed");
    let gets = test_id(&names, "getsUser");
    let vc_model::TestBody::Call(call) = &program.tests[&gets].body else { panic!("getsUser calls an api") };
    assert_eq!(names.action(*call), "call users#get");
    assert_eq!(names.owner(OwnerKey::Run), "run");
    assert_eq!(names.owner(OwnerKey::File(plan.files[0].id)), "file users.crud");
    assert_eq!(names.owner(OwnerKey::Instance(InstanceId::new(3))), "instance 3");
    let at = names.test_location(gets).unwrap();
    assert_eq!((at.path.as_str(), at.line, at.column), ("services/users/crud.test.yaml", 7, 5));
    assert_eq!(names.text(gets.sym()), "users.crud#getsUser");
}

#[test]
fn lists_the_file_tags_first_without_repeats() {
    let names = sample();
    assert_eq!(names.tags(test_id(&names, "getsUser")), ["smoke", "read"]);
    assert_eq!(names.tags(test_id(&names, "listsUsers")), ["smoke"]);
}

#[test]
fn labels_checks_by_their_expect_key() {
    let (program, _) = testing::project();
    let test = *program.tests.keys().next().unwrap();
    let mut check = testing::sample_check(test);
    assert_eq!(check_label(&check), "status");
    let segments = vec![Segment::Key("items".into()), Segment::All, Segment::Key("price".into()), Segment::Index(2)];
    check.target = Some(Path { segments });
    let labels = [
        (CheckKind::Status, "status"),
        (CheckKind::Header, "headers.items[*].price[2]"),
        (CheckKind::Schema, "body.schema"),
        (CheckKind::Match, "body.match.items[*].price[2]"),
        (CheckKind::Equals, "body.equals"),
        (CheckKind::Size, "size"),
        (CheckKind::Timing, "timing.items[*].price[2]"),
        (CheckKind::Cel, "cel.items[*].price[2]"),
        (CheckKind::Custom, "custom"),
    ];
    for (kind, label) in labels {
        check.kind = kind;
        assert_eq!(check_label(&check), label, "{kind:?}");
    }
    check.kind = CheckKind::Match;
    check.target = Some(Path { segments: Vec::new() });
    assert_eq!(check_label(&check), "body.match");
}
