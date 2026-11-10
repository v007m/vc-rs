use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use vc_engine::{Changes, FileChange};

use super::draw;
use crate::edit_session::EditSession;
use crate::screen::Update;

const PATH: &str = "services/users/users.api.yaml";

fn change(path: &str, before: &str, after: &str) -> FileChange {
    FileChange { path: path.to_owned(), before: Some(before.to_owned()), after: after.to_owned(), moved_from: None }
}

/// The rows `session` draws on an 80 by 24 terminal.
fn rows(session: &EditSession) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|frame| draw(session, frame, frame.area())).unwrap();
    let buffer = terminal.backend().buffer();
    (0..24).map(|y| (0..80).map(|x| buffer[(x, y)].symbol()).collect::<String>()).collect()
}

#[test]
fn a_draft_shows_the_diff_of_every_file_w_would_write() {
    let (mut session, _) = EditSession::open(PATH);
    session.take(&Update::Opened { path: PATH.to_owned(), text: "a\n".to_owned(), digest: "d1".to_owned() });
    session.press(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    let files = vec![change(PATH, "a\n", "b\n"), change("services/users/crud.test.yaml", "c\n", "d\n")];
    session.take(&Update::Edited(Changes { files, ..Changes::default() }));
    let shown = rows(&session).join("\n");
    for header in ["+++ b/services/users/users.api.yaml", "+++ b/services/users/crud.test.yaml"] {
        assert!(shown.contains(header), "{header} is not shown:\n{shown}");
    }
}
