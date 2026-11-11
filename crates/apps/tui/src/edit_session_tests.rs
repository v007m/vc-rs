use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use vc_engine::{Changes, SaveFile};

use super::{EditSession, Edited, Stage};
use crate::screen::{Ask, Update};

const PATH: &str = "services/users/users.api.yaml";

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn save() -> KeyEvent {
    KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)
}

/// An edit of `PATH`, opened with the text `ab`.
fn opened() -> EditSession {
    let (mut session, ask) = EditSession::open(PATH);
    assert_eq!(ask, Ask::Open(PATH.to_owned()));
    let update = Update::Opened { path: PATH.to_owned(), text: "ab".to_owned(), digest: "d1".to_owned() };
    assert_eq!(session.take(&update), Edited::Going);
    session
}

#[test]
fn ctrl_s_drafts_the_text_with_the_digest_it_was_opened_with() {
    let mut session = opened();
    assert_eq!(*session.stage(), Stage::Text);
    session.press(key(KeyCode::Char('x')));
    let asked = session.press(save());
    let request = SaveFile { path: PATH.to_owned(), text: "xab".to_owned(), expected_digest: Some("d1".to_owned()) };
    assert_eq!(asked, Edited::Asks(Ask::Edit(request)));
    assert_eq!(*session.stage(), Stage::Drafting);
}

#[test]
fn a_refused_draft_goes_back_to_the_text_with_the_refusal_shown() {
    let mut session = opened();
    session.press(key(KeyCode::Char('x')));
    session.press(save());
    assert_eq!(session.take(&Update::EditFailed("V2001 unknown api\nhelp: add it".to_owned())), Edited::Going);
    assert_eq!((session.stage(), session.refusal()), (&Stage::Text, Some("V2001 unknown api\nhelp: add it")));
    assert_eq!(session.area().text(), "xab", "the text is kept");
    session.press(save());
    assert_eq!(session.refusal(), None, "a new draft clears it");
}

#[test]
fn a_draft_that_changes_nothing_is_not_written_and_esc_goes_back_to_the_text() {
    let mut session = opened();
    session.press(save());
    assert_eq!(session.take(&Update::Edited(Changes::default())), Edited::Going);
    assert_eq!(*session.stage(), Stage::Drafted(Changes::default()));
    assert_eq!(session.press(key(KeyCode::Char('w'))), Edited::Going);
    assert_eq!(session.take(&Update::Saved(Vec::new())), Edited::Going, "nothing was asked to be written");
    session.press(key(KeyCode::Esc));
    assert_eq!(*session.stage(), Stage::Text);
}

#[test]
fn esc_asks_before_dropping_changed_text_and_not_otherwise() {
    let mut session = opened();
    assert_eq!(session.press(key(KeyCode::Esc)), Edited::Closed(None), "nothing changed");
    let mut session = opened();
    session.press(key(KeyCode::Char('x')));
    assert_eq!(session.press(key(KeyCode::Esc)), Edited::Going);
    assert!(session.confirming());
    assert_eq!(session.press(key(KeyCode::Char('n'))), Edited::Going, "kept");
    assert_eq!(session.area().text(), "xab");
    session.press(key(KeyCode::Esc));
    assert_eq!(session.press(key(KeyCode::Char('y'))), Edited::Closed(None));
}

#[test]
fn a_file_that_cannot_be_opened_fails_the_edit() {
    let (mut session, _) = EditSession::open(PATH);
    assert_eq!(session.take(&Update::EditFailed("no file".to_owned())), Edited::Dropped("no file".to_owned()));
    let (mut session, _) = EditSession::open(PATH);
    assert_eq!(session.press(key(KeyCode::Esc)), Edited::Closed(None));
}
