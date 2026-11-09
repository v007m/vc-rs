//! `vc demo stop` with no demo running is not an error and names the port (D175, D188).

use crate::{CONFIG, Project};

#[test]
fn stopping_no_demo_exits_0_and_names_the_port() {
    let project = Project::new(&[CONFIG]);
    project.vc(&["demo", "stop", "--port", "7432"]).assert().code(0).stdout("no demo running on port 7432\n");
}
