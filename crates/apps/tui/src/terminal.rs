//! The terminal: raw mode on the alternate screen while the UI runs, restored however it ends.
//! Keys are read on a blocking task and sent over a channel, so the loop never waits on the
//! keyboard while it could be drawing.

use std::io::{self, Stdout};
use std::time::Duration;

use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tokio::sync::mpsc;

use crate::{Tui, TuiError, TuiOptions};

/// How long the key reader waits for a key before it checks whether the UI has quit.
const POLL: Duration = Duration::from_millis(100);
/// Key events the reader may get ahead of the UI by.
const QUEUE: usize = 64;

/// Runs the UI in this terminal until a key quits it. Call it from within a Tokio runtime.
///
/// # Errors
///
/// [`TuiError::Terminal`] when the terminal cannot be set up or drawn on, and
/// [`TuiError::Input`] when the keyboard cannot be read.
pub async fn run(options: TuiOptions) -> Result<(), TuiError> {
    let mut screen = Screen::open()?;
    let (sender, mut events) = mpsc::channel(QUEUE);
    let reader = tokio::task::spawn_blocking(move || read_keys(&sender));
    let mut tui = Tui::new(options);
    let drawn = loop {
        if let Err(error) = screen.terminal.draw(|frame| tui.draw(frame)) {
            break Err(TuiError::Terminal(error));
        }
        match events.recv().await {
            Some(Event::Key(key)) => tui.press(key),
            Some(_) => {}
            None => break Ok(()),
        }
        if tui.quitting() {
            break Ok(());
        }
    };
    drop(events);
    let read = reader.await.map_err(|error| TuiError::Input { reason: error.to_string() })?;
    drawn.and_then(|()| read.map_err(|error| TuiError::Input { reason: error.to_string() }))
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

/// The terminal in raw mode on the alternate screen; dropping it restores the terminal.
struct Screen {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl Screen {
    fn open() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut screen = Self::restoring(Terminal::new(CrosstermBackend::new(io::stdout())))?;
        execute!(screen.terminal.backend_mut(), EnterAlternateScreen)?;
        screen.terminal.clear()?;
        Ok(screen)
    }

    /// `terminal` wrapped so that raw mode ends even when it could not be made.
    fn restoring(terminal: io::Result<Terminal<CrosstermBackend<Stdout>>>) -> io::Result<Self> {
        terminal.map(|terminal| Self { terminal }).inspect_err(|_| {
            let _ = disable_raw_mode();
        })
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = disable_raw_mode();
        let _ = self.terminal.show_cursor();
    }
}
