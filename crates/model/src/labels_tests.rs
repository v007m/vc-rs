use super::{InstanceLabels, instance_marks};
use crate::{FileId, Instance, InstanceId, Needs, Plan, PlannedFile, StringTable, TestId};

/// A plan of `test`'s instances, one per `(row, repeat)`, its rows named by `names`.
fn plan(slots: &[(Option<u32>, u32)], names: &[(u32, &str)]) -> (Plan, TestId) {
    let mut table = StringTable::new();
    let file = FileId::new(table.intern("users.crud").unwrap());
    let test = TestId::new(table.intern("users.crud#rejectsBadEmail").unwrap());
    let instance = |(n, &(example_row, repeat_index)): (usize, _)| Instance {
        id: InstanceId::new(u32::try_from(n).unwrap()),
        test,
        example_row,
        repeat_index,
        needs: Needs::default(),
    };
    let instances = slots.iter().enumerate().map(instance).collect();
    let planned =
        PlannedFile { id: file, before_all: Vec::new(), instances, after_all: Vec::new(), skipped: Vec::new() };
    let row_names = names.iter().map(|&(row, name)| ((test, row), name.to_owned())).collect();
    (Plan { files: vec![planned], row_names, ..Plan::default() }, test)
}

fn suffixes(slots: &[(Option<u32>, u32)], names: &[(u32, &str)]) -> Vec<String> {
    let (plan, test) = plan(slots, names);
    let labels = InstanceLabels::new(&plan);
    (0..slots.len()).map(|n| labels.suffix(test, Some(InstanceId::new(u32::try_from(n).unwrap())))).collect()
}

#[test]
fn a_named_row_shows_its_name_and_an_unnamed_one_its_number() {
    let shown = suffixes(&[(Some(0), 0), (Some(1), 0)], &[(0, "missing @")]);
    assert_eq!(shown, [" › missing @", " [row 2]"]);
}

#[test]
fn a_name_two_rows_share_is_numbered_too() {
    let shown = suffixes(&[(Some(0), 0), (Some(1), 0)], &[(0, "Oslo"), (1, "Oslo")]);
    assert_eq!(shown, [" › Oslo [row 1]", " › Oslo [row 2]"]);
}

#[test]
fn every_instance_of_a_repeated_test_names_its_repeat() {
    let shown = suffixes(&[(Some(0), 0), (Some(0), 1), (None, 0)], &[(0, "Oslo")]);
    assert_eq!(shown, [" › Oslo [repeat 1]", " › Oslo [repeat 2]", " [repeat 1]"]);
}

#[test]
fn an_instance_the_plan_lacks_or_none_has_nothing_after_its_id() {
    let (plan, test) = plan(&[(None, 0)], &[]);
    let labels = InstanceLabels::new(&plan);
    assert_eq!(labels.suffix(test, Some(InstanceId::new(0))), "");
    assert_eq!(labels.suffix(test, Some(InstanceId::new(9))), "");
    assert_eq!(labels.suffix(test, None), "");
}

#[test]
fn marks_number_rows_and_repeats_from_one() {
    assert_eq!(instance_marks(Some(1), Some(2)), " [row 2, repeat 3]");
    assert_eq!(instance_marks(None, Some(0)), " [repeat 1]");
    assert_eq!(instance_marks(Some(0), None), " [row 1]");
    assert_eq!(instance_marks(None, None), "");
}

#[test]
fn the_first_repeat_picked_alone_still_names_its_repeat() {
    let (mut plan, test) = plan(&[(Some(1), 0)], &[]);
    plan.repeated.insert(test);
    let labels = InstanceLabels::new(&plan);
    assert_eq!(labels.suffix(test, Some(InstanceId::new(0))), " [row 2, repeat 1]");
}

#[test]
fn a_row_name_cannot_drive_the_terminal() {
    let shown = suffixes(&[(Some(0), 0)], &[(0, "\u{1b}]0;pwned\u{7}\u{1b}[2J \u{202e}evil\nnext")]);
    assert_eq!(shown, [" › \u{fffd}]0;pwned\u{fffd}\u{fffd}[2J \u{fffd}evil\u{fffd}next"]);
}
