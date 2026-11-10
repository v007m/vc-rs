//! The terminal: raw mode on the alternate screen while the UI runs, restored however it ends.
//! Keys are read on a blocking task and sent over a channel, and what the engine sends back
//! arrives on another, so the loop never waits on the keyboard or the engine while it could
//! be drawing.

use std::io::{self, Stdout};
use std::panic::{self, PanicHookInfo};
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tokio::sync::mpsc::{self, Receiver};
use vc_engine::Engine;

use crate::driver::Driver;
use crate::screen::Update;
use crate::{Tui, TuiError, TuiOptions, view};

/// How long the key reader waits for a key before it checks whether the UI has quit.
const POLL: Duration = Duration::from_millis(100);
/// Key events, or updates, the senders may get ahead of the UI by.
const QUEUE: usize = 64;

/// Runs the UI over `engine` in this terminal until a key quits it, ending a run still in
/// progress first. Call it from within a Tokio runtime.
///
/// # Errors
///
/// [`TuiError::Terminal`] when the terminal cannot be set up or drawn on, and
/// [`TuiError::Input`] when the keyboard cannot be read.
pub async fn run(engine: Engine, options: TuiOptions) -> Result<(), TuiError> {
    let mut raw = Raw::open()?;
    let (sender, keys) = mpsc::channel(QUEUE);
    let reader = tokio::task::spawn_blocking(move || read_keys(&sender));
    let (updates_sender, updates) = mpsc::channel(QUEUE);
    let driver = Driver::new(engine, options.base.clone(), updates_sender);
    let mut session = Session { tui: Tui::new(options), keys, updates, driver };
    let drawn = loop {
        if let Err(error) = raw.terminal.draw(|frame| session.tui.draw(frame)) {
            break Err(TuiError::Terminal(error));
        }
        if !session.next().await {
            break Ok(());
        }
    };
    let drawn = drawn.and_then(|()| draw_stopping(&mut raw, &session));
    session.close().await;
    let read = reader.await.map_err(|error| TuiError::Input { reason: error.to_string() })?;
    drawn.and_then(|()| read.map_err(|error| TuiError::Input { reason: error.to_string() }))
}

/// When a run or load run is still in progress, shows that quitting is ending it, as that can
/// take a few seconds.
fn draw_stopping(raw: &mut Raw, session: &Session) -> Result<(), TuiError> {
    if session.driver.busy() {
        raw.terminal.draw(|frame| view::draw_stopping(&session.tui, frame)).map_err(TuiError::Terminal)?;
    }
    Ok(())
}

/// The UI, where its keys and updates come from, and the engine side.
struct Session {
    tui: Tui,
    keys: Receiver<Event>,
    updates: Receiver<Update>,
    driver: Driver,
}

impl Session {
    /// Waits for the next key or update and applies it; `false` once the UI is to stop.
    async fn next(&mut self) -> bool {
        tokio::select! {
            key = self.keys.recv() => match key {
                Some(Event::Key(key)) => {
                    if let Some(ask) = self.tui.press(key) {
                        self.driver.carry(ask);
                    }
                }
                Some(_) => {}
                None => return false,
            },
            Some(update) = self.updates.recv() => self.tui.update(&update),
        }
        drain(&mut self.tui, &mut self.updates);
        !self.tui.quitting()
    }

    /// Stops listening to the keyboard and the engine, then ends the run in progress.
    async fn close(self) {
        drop((self.keys, self.updates));
        self.driver.finish().await;
    }
}

/// Applies the updates already waiting, up to a queue's worth, so one frame is drawn for a
/// burst of them rather than one each: drawing after every one lets the engine get ahead.
fn drain(tui: &mut Tui, updates: &mut Receiver<Update>) {
    for _ in 0..QUEUE {
        match updates.try_recv() {
            Ok(update) => tui.update(&update),
            Err(_) => break,
        }
    }
}

/// Sends each terminal event until the UI stops listening.
fn read_keys(sender: &mpsc::Sender<Event>) -> io::Result<()> {
    while !sender.is_closed() {
        if event::poll(POLL)? && sender.blocking_send(event::read()?).is_err() {
            break;
        }
    }
    Ok(())
}

/// The terminal in raw mode on the alternate screen; dropping it restores the terminal, and
/// so does a panic while it is open, before the panic is printed.
struct Raw {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    _hook: PanicRestore,
}

impl Raw {
    fn open() -> io::Result<Self> {
        enable_raw_mode()?;
        let hook = PanicRestore::install(restore);
        let mut raw = Self::restoring(Terminal::new(CrosstermBackend::new(io::stdout())), hook)?;
        execute!(raw.terminal.backend_mut(), EnterAlternateScreen)?;
        raw.terminal.clear()?;
        Ok(raw)
    }

    /// `terminal` wrapped so that raw mode ends even when it could not be made.
    fn restoring(terminal: io::Result<Terminal<CrosstermBackend<Stdout>>>, hook: PanicRestore) -> io::Result<Self> {
        terminal.map(|terminal| Self { terminal, _hook: hook }).inspect_err(|_| {
            let _ = disable_raw_mode();
        })
    }
}

impl Drop for Raw {
    fn drop(&mut self) {
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = disable_raw_mode();
        let _ = self.terminal.show_cursor();
    }
}

/// Leaves the alternate screen and raw mode, showing the cursor, for a panic's message.
fn restore() {
    let _ = execute!(io::stdout(), LeaveAlternateScreen, crossterm::cursor::Show);
    let _ = disable_raw_mode();
}

/// A panic hook, shared so that the one put in front of it can call it and it can be put back.
type Hook = Arc<dyn Fn(&PanicHookInfo<'_>) + Send + Sync + 'static>;

/// The panic hook that runs `restore` when the thread installing it panics, then the hook it
/// replaced, which prints the panic; dropping it puts the replaced hook back. A panic on
/// another thread, as in an engine task, leaves the terminal to the UI, which hears of it as
/// a failed update.
struct PanicRestore {
    previous: Hook,
}

impl PanicRestore {
    fn install(restore: impl Fn() + Send + Sync + 'static) -> Self {
        let previous: Hook = Arc::from(panic::take_hook());
        let chained = Arc::clone(&previous);
        let ui = std::thread::current().id();
        panic::set_hook(Box::new(move |info| {
            if std::thread::current().id() == ui {
                restore();
            }
            chained(info);
        }));
        Self { previous }
    }
}

impl Drop for PanicRestore {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }
        let previous = Arc::clone(&self.previous);
        panic::set_hook(Box::new(move |info| previous(info)));
    }
}

#[cfg(test)]
#[path = "terminal_tests.rs"]
mod tests;
