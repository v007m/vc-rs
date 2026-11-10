use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use std::path::PathBuf;
use std::sync::Arc;

use ratatui::backend::TestBackend;
use vc_engine::{Changes, EngineBuilder};
use vc_testkit::{FakeVcs, MemoryCompileCache, MemoryEnvironment, MemorySources};

use super::draw;
use crate::edit_session::EditSession;
use crate::screen::Update;

const PATH: &str = "services/users/users.api.yaml";

/// The draft renaming `users#list` to `users#listUsers`: it changes the api file and the test
/// file calling it.
fn renamed() -> Changes {
    let files: [(&str, &[u8]); 3] = [
        ("vibranium.config.yaml", b"syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n"),
        (PATH, b"baseUrl: http://x\napis:\n  - id: list\n    path: /users\n"),
        ("services/users/crud.test.yaml", b"tests:\n  - id: listsUsers\n    call: users#list\n"),
    ];
    let engine = EngineBuilder::new(PathBuf::from("/project"))
        .sources(Arc::new(MemorySources::new(&files)))
        .compile_cache(Arc::new(MemoryCompileCache::new()))
        .environment(Arc::new(MemoryEnvironment::new(&[])))
        .vcs(Arc::new(FakeVcs::default()))
        .build();
    engine.unwrap().rename("users#list", "users#listUsers").unwrap()
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
    let changes = renamed();
    assert_eq!(changes.files.len(), 2, "{changes:?}");
    session.take(&Update::Edited(changes));
    let shown = rows(&session).join("\n");
    for header in ["+++ b/services/users/users.api.yaml", "+++ b/services/users/crud.test.yaml"] {
        assert!(shown.contains(header), "{header} is not shown:\n{shown}");
    }
}
