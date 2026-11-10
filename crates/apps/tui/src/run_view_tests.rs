use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Widget;
use vc_engine::TestStatus;

use super::{detail_pane, file_row, first_shown, test_row};
use crate::run_model::{FileGroup, Finished};

/// A failed test whose ID, label and location carry an OSC title sequence and a bell.
fn hostile() -> Finished {
    Finished {
        test: "users.crud#a\u{1b}]0;owned\u{7}".to_owned(),
        label: "a\u{1b}]0;owned\u{7}".to_owned(),
        status: TestStatus::Failed,
        attempts: 1,
        location: Some("services/users/crud.test.yaml\u{1b}[2J:4".to_owned()),
        reason: None,
        checks: Vec::new(),
    }
}

fn text(line: &Line<'_>) -> String {
    line.spans.iter().map(|span| span.content.as_ref()).collect()
}

#[test]
fn a_test_or_file_named_with_control_characters_cannot_drive_the_terminal() {
    let test = hostile();
    let group = FileGroup { file: "users\u{1b}]0;x".to_owned(), tests: vec![test.clone()] };
    assert_eq!(text(&file_row(&group)), "▾ users·]0;x  ✕ 1 ");
    assert_eq!(text(&test_row(&test)), "  ✕ a·]0;owned·");
    let mut buffer = Buffer::empty(Rect::new(0, 0, 80, 6));
    detail_pane(Some(&test)).render(buffer.area, &mut buffer);
    let shown: String = buffer.content().iter().map(ratatui::buffer::Cell::symbol).collect();
    assert!(!shown.contains(['\u{1b}', '\u{7}']), "{shown}");
    for cleaned in ["a·]0;owned·", "users.crud#a·]0;owned·", "crud.test.yaml·[2J:4"] {
        assert!(shown.contains(cleaned), "{cleaned} in {shown}");
    }
}

#[test]
fn only_the_rows_in_view_are_drawn_the_chosen_one_last_when_it_is_below_the_first_screen() {
    assert_eq!(first_shown(None, 10), 0, "nothing chosen: the top");
    assert_eq!(first_shown(Some(9), 10), 0, "in view on the first screen");
    assert_eq!(first_shown(Some(10), 10), 1);
    assert_eq!(first_shown(Some(500), 20), 481, "the chosen row is the last of the 20 shown");
}
