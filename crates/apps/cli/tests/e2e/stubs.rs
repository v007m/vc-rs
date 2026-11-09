//! The command that is designed but not built yet exits 2 and says so.

use crate::{CONFIG, Project};

#[test]
fn migrate_v1_exits_2_and_says_it_is_not_built() {
    let project = Project::new(&[CONFIG]);
    let output = project.vc(&["migrate", "v1", "old"]).assert().code(2).get_output().clone();
    let said = crate::said(&output);
    assert!(said.contains("is not built yet"), "{said}");
}
