use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::TextArea;

/// `area` after each of `codes`.
fn pressed(mut area: TextArea, codes: &[KeyCode]) -> TextArea {
    for code in codes {
        area.press(KeyEvent::new(*code, KeyModifiers::NONE));
    }
    area
}

#[test]
fn the_text_comes_back_as_it_was_given_its_last_line_end_kept() {
    let text = "name: users\r\nbase: http://x\n";
    assert_eq!(TextArea::new(text).text(), text);
    assert_eq!(TextArea::new("").lines(), [String::new()]);
}

#[test]
fn typing_enter_and_backspace_edit_at_the_cursor() {
    let area = pressed(TextArea::new("ab\ncd"), &[KeyCode::Right, KeyCode::Char('é'), KeyCode::Enter, KeyCode::Tab]);
    assert_eq!((area.text().as_str(), area.cursor()), ("aé\n  b\ncd", (1, 2)));
    let area = pressed(area, &[KeyCode::Home, KeyCode::Backspace, KeyCode::Backspace]);
    assert_eq!((area.text().as_str(), area.cursor()), ("a  b\ncd", (0, 1)));
}

#[test]
fn delete_joins_the_next_line_at_a_lines_end() {
    let area = pressed(TextArea::new("ab\ncd"), &[KeyCode::End, KeyCode::Delete, KeyCode::Delete]);
    assert_eq!((area.text().as_str(), area.cursor()), ("abd", (0, 2)));
}

#[test]
fn the_cursor_stays_on_the_text_and_wraps_at_line_ends() {
    let area = pressed(TextArea::new("abc\nd"), &[KeyCode::End, KeyCode::Down]);
    assert_eq!(area.cursor(), (1, 1), "kept on the shorter line");
    let area = pressed(area, &[KeyCode::Right, KeyCode::Down, KeyCode::PageDown]);
    assert_eq!(area.cursor(), (1, 1), "nothing after the last character");
    let area = pressed(area, &[KeyCode::Home, KeyCode::Left]);
    assert_eq!(area.cursor(), (0, 3), "left from a line's start goes to the end of the one before");
    let area = pressed(area, &[KeyCode::Right, KeyCode::PageUp, KeyCode::Up, KeyCode::Left]);
    assert_eq!(area.cursor(), (0, 0));
}

#[test]
fn a_control_character_is_not_typed_but_altgr_is() {
    let mut area = TextArea::new("");
    area.press(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    area.press(KeyEvent::new(KeyCode::Char('@'), KeyModifiers::CONTROL | KeyModifiers::ALT));
    assert_eq!(area.text(), "@");
}
