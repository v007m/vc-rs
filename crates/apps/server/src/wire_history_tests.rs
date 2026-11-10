use vc_contracts::results::v1 as wire;
use vc_engine::{CheckRow, TestRow};

use super::test;

fn row(example_row: Option<u32>, repeat_index: u32) -> TestRow {
    TestRow { test_id: "users.crud#rejectsBadEmail".into(), example_row, repeat_index, ..TestRow::default() }
}

#[test]
fn a_label_marks_the_example_row_and_repeat_from_one() {
    assert_eq!(test(&row(None, 0)).label, "users.crud#rejectsBadEmail");
    assert_eq!(test(&row(Some(1), 0)).label, "users.crud#rejectsBadEmail [row 2]");
    assert_eq!(test(&row(Some(0), 2)).label, "users.crud#rejectsBadEmail [row 1, repeat 3]");
}

#[test]
fn a_stored_check_keeps_its_kind_target_and_line() {
    let check = CheckRow {
        kind: "body.match".into(),
        path: Some("items[*].price".into()),
        expected: "> 0".into(),
        file: "services/users/crud.test.yaml".into(),
        line: 9,
        ..CheckRow::default()
    };
    let found = test(&TestRow {
        checks: vec![check, CheckRow { kind: "later".into(), ..CheckRow::default() }],
        ..row(None, 0)
    });
    let [matched, unknown] = found.checks.as_slice() else { panic!("{found:?}") };
    assert_eq!((matched.kind(), matched.target.as_deref()), (wire::CheckKind::Match, Some("items[*].price")));
    assert_eq!(matched.span.as_ref().map(|span| span.start_line), Some(9));
    assert_eq!(unknown.kind(), wire::CheckKind::Unspecified);
}
