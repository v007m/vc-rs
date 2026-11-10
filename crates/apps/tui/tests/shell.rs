//! The shell: the header and tab bar, the tabs the keys choose, the footer and the help,
//! rendered to a test backend.

mod support;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use insta::assert_snapshot;
use support::tui;
use vc_tui::{Tui, TuiOptions};

/// The shell's screen, 100 by 16.
fn screen(tui: &Tui) -> String {
    support::screen(tui, (100, 16))
}

fn press(tui: &mut Tui, codes: &[KeyCode]) {
    support::press(tui, codes);
}

#[test]
fn opens_on_the_run_tab_with_the_project_and_profile() {
    let tui = tui();
    assert_eq!(tui.tab(), "Run");
    assert_snapshot!("run", screen(&tui));
}

#[test]
fn digits_choose_tabs_and_tab_keys_cycle_them() {
    let mut tui = tui();
    press(&mut tui, &[KeyCode::Char('3')]);
    assert_eq!(tui.tab(), "History");
    assert_snapshot!("history", screen(&tui));
    press(&mut tui, &[KeyCode::Char('9'), KeyCode::Char('0')]);
    assert_eq!(tui.tab(), "History");
    press(&mut tui, &[KeyCode::Tab, KeyCode::Tab]);
    assert_eq!(tui.tab(), "Import");
    press(&mut tui, &[KeyCode::Tab]);
    assert_eq!(tui.tab(), "Run");
    press(&mut tui, &[KeyCode::BackTab]);
    assert_eq!(tui.tab(), "Import");
}

#[test]
fn the_help_opens_over_the_tab_and_closes() {
    let mut tui = tui();
    press(&mut tui, &[KeyCode::Char('?')]);
    assert_snapshot!("help", screen(&tui));
    press(&mut tui, &[KeyCode::Esc]);
    assert!(!screen(&tui).contains("Help"));
    press(&mut tui, &[KeyCode::Char('?'), KeyCode::Char('?')]);
    assert!(!screen(&tui).contains("Help"));
    press(&mut tui, &[KeyCode::Char('?'), KeyCode::Char('2')]);
    assert_eq!((tui.tab(), screen(&tui).contains("Help")), ("Explore", false));
}

#[test]
fn q_and_ctrl_c_quit_but_a_release_does_nothing() {
    let mut tui = tui();
    tui.press(KeyEvent::new_with_kind(KeyCode::Char('q'), KeyModifiers::NONE, KeyEventKind::Release));
    tui.press(KeyEvent::new_with_kind(KeyCode::Char('2'), KeyModifiers::NONE, KeyEventKind::Release));
    assert_eq!((tui.quitting(), tui.tab()), (false, "Run"));
    press(&mut tui, &[KeyCode::Char('q')]);
    assert!(tui.quitting());
    let mut tui = self::tui();
    tui.press(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert!(tui.quitting());
}

#[test]
fn no_profile_shows_the_default_one() {
    let tui = Tui::new(TuiOptions::new("shop", Vec::new()));
    let header = screen(&tui).lines().next().unwrap_or_default().to_owned();
    assert!(header.starts_with("◆ vibranium shop  profile default"), "{header}");
}
