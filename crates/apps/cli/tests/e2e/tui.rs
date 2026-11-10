//! `vc tui` needs a terminal: under a pipe it says so and exits 2 instead of drawing.

use crate::{CONFIG, Project, said};

#[test]
fn without_a_terminal_it_exits_2_and_says_why() {
    let output = Project::new(&[CONFIG]).vc(&["tui"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", said(&output));
    assert!(said(&output).contains("vc tui needs a terminal"), "{}", said(&output));
}
