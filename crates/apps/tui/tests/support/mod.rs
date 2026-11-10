//! What the TUI's tests share: drawing the UI to text, pressing keys, and a project the engine
//! runs with a fake transport, whose feed the tests hand to the UI.

#![allow(dead_code, reason = "each test file uses part of the support")]

use std::error::Error;
use std::path::PathBuf;
use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tokio::sync::mpsc;
use vc_engine::{Engine, EngineBuilder, RunFeed, RunRequest, RunWatch, interrupts};
use vc_testkit::{FakeTransport, FakeVcs, MemoryCompileCache, MemoryEnvironment, MemoryHistory, MemorySources, status};
use vc_tui::{Ask, Tui, TuiOptions, Update};

/// What a helper returns; the tests unwrap it.
pub type Outcome<T> = Result<T, Box<dyn Error + Send + Sync>>;

const CONFIG: &[u8] = b"syntaxVersion: 2\nprofiles:\n  - id: staging\n    default: true\nvars: {baseUrl: 'http://x'}\n";
const USERS: &[u8] = b"baseUrl: ${var.baseUrl}\napis:
  - id: list
    path: /users
  - id: create
    method: POST
    path: /users
    expect: {status: 201}
";
const CRUD: &[u8] = b"tests:
  - id: listsUsers
    call: users#list
  - id: createsUser
    call: users#create
  - id: deletesUser
    call: users#list
    skip: {reason: waits for DELETE}
";
const ORDERS: &[u8] = b"tests:\n  - id: listsOrders\n    call: users#list\n";
const PEAK: &[u8] = b"mix:
  - {test: users.crud#listsUsers, weight: 1}
stages:
  - {duration: 1s, users: 1}
thinkTime: 1s
thresholds: {errorRate: {lt: 0.5}}
";

/// Where [`importing`] serves [`CURLS`].
pub const CURLS_URL: &str = "https://specs.test/orders.curl";
/// Two curl commands, as copied from a browser's dev tools.
pub const CURLS: &str = "curl https://api.orders.test/v1/orders?status=open\n\ncurl -X POST https://api.orders.test/v1/orders -H 'content-type: application/json' --data-raw '{\"sku\":\"a1\"}'\n";

/// The UI of the project `shop` under the profile `staging`.
pub fn tui() -> Tui {
    Tui::new(TuiOptions::new("shop", vec!["staging".to_owned()]))
}

/// Presses each key in turn, without modifiers; what the last one asked for.
pub fn press(tui: &mut Tui, codes: &[KeyCode]) -> Option<Ask> {
    codes.iter().fold(None, |_, code| tui.press(KeyEvent::new(*code, KeyModifiers::NONE)))
}

/// The screen as text, one line per row, trailing spaces dropped.
pub fn screen(tui: &Tui, (width, height): (u16, u16)) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| tui.draw(frame)).unwrap();
    let buffer = terminal.backend().buffer();
    let rows = (0..height).map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect::<String>());
    rows.map(|row| row.trim_end().to_owned()).collect::<Vec<_>>().join("\n")
}

/// An engine over a project whose `users#create` expects 201 and gets 200, every request
/// answered 200 by a fake transport.
pub fn engine() -> Outcome<Engine> {
    engine_over(&Arc::new(MemoryHistory::new()), 200)
}

/// An engine over the project that stores its runs in `history` and answers every request
/// with `code`.
pub fn engine_over(history: &Arc<MemoryHistory>, code: u16) -> Outcome<Engine> {
    engine_with(history, FakeTransport::new(move |_, _| Ok(status(code))))
}

/// An engine over the project whose transport serves [`CURLS`] at [`CURLS_URL`] and
/// answers anything else 404.
pub fn importing() -> Outcome<Engine> {
    engine_with(
        &Arc::new(MemoryHistory::new()),
        FakeTransport::new(|request, _| {
            let found = request.uri.path() == "/orders.curl";
            let mut response = status(if found { 200 } else { 404 });
            response.body = (if found { CURLS } else { "" }).into();
            Ok(response)
        }),
    )
}

/// An engine over the project that stores its runs in `history` and sends through `transport`.
fn engine_with(history: &Arc<MemoryHistory>, transport: FakeTransport) -> Outcome<Engine> {
    let files: [(&str, &[u8]); 5] = [
        ("vibranium.config.yaml", CONFIG),
        ("services/users/users.api.yaml", USERS),
        ("services/users/crud.test.yaml", CRUD),
        ("services/orders/flow.test.yaml", ORDERS),
        ("load/users/peak.load.yaml", PEAK),
    ];
    let engine = EngineBuilder::new(PathBuf::from("/project"))
        .sources(Arc::new(MemorySources::new(&files)))
        .compile_cache(Arc::new(MemoryCompileCache::new()))
        .environment(Arc::new(MemoryEnvironment::new(&[])))
        .vcs(Arc::new(FakeVcs::default()))
        .transport(Arc::new(transport))
        .history(history.clone(), history.clone())
        .build()?;
    Ok(engine)
}

/// The run `ask` asks for.
pub fn run_of(ask: Option<Ask>) -> Outcome<RunRequest> {
    match ask {
        Some(Ask::Run(request)) => Ok(*request),
        other => Err(format!("a run was asked for: {other:?}").into()),
    }
}

/// Runs what `ask` asks for, one instance at a time so the order is fixed, and hands the
/// feed to `tui` as the terminal would.
pub async fn carry(tui: &mut Tui, ask: Option<Ask>) -> Outcome<()> {
    carry_on(&engine()?, tui, ask).await
}

/// [`carry`] on `engine`.
pub async fn carry_on(engine: &Engine, tui: &mut Tui, ask: Option<Ask>) -> Outcome<()> {
    for item in feed_of(engine, ask).await? {
        tui.update(&Update::Run(item));
    }
    Ok(())
}

/// The feed of the run `ask` asks for on `engine`, one instance at a time, up to its `Done`.
pub async fn feed_of(engine: &Engine, ask: Option<Ask>) -> Outcome<Vec<RunFeed>> {
    let (sender, mut feed) = mpsc::channel(1_024);
    let request = RunRequest { watch: Some(RunWatch::new(sender)), concurrency: Some(1), ..run_of(ask)? };
    engine.run(&request, interrupts().1).await?;
    drop(request);
    let mut items = Vec::new();
    while let Some(item) = feed.recv().await {
        let done = matches!(item, RunFeed::Done(_));
        items.push(item);
        if done {
            return Ok(items);
        }
    }
    Err("the feed ended without Done".into())
}
