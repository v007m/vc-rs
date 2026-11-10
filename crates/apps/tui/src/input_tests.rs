use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{Input, Typed};

#[test]
fn alt_gr_characters_are_typed_and_ctrl_or_alt_ones_are_not() {
    let mut input = Input::new("me");
    let key = |typed, modifiers| KeyEvent::new(KeyCode::Char(typed), modifiers);
    assert_eq!(input.press(key('@', KeyModifiers::CONTROL | KeyModifiers::ALT)), Typed::Editing);
    input.press(key('x', KeyModifiers::CONTROL));
    input.press(key('y', KeyModifiers::ALT));
    input.press(key('Z', KeyModifiers::SHIFT));
    input.press(key('€', KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT));
    assert_eq!(input.text(), "me@Z€");
}
