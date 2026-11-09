//! The commands that are designed but not built yet exit 2 and say so.

use crate::{CONFIG, Project};

#[test]
fn each_stub_exits_2_and_says_it_is_not_built() {
    let project = Project::new(&[CONFIG]);
    let commands: [&[&str]; 2] = [&["load", "checkout.peak#browses"], &["migrate", "v1", "old"]];
    for args in commands {
        let output = project.vc(args).assert().code(2).get_output().clone();
        let said = crate::said(&output);
        assert!(said.contains("is not built yet"), "{args:?}: {said}");
    }
}
