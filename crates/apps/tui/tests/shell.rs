//! The shell: the header and tab bar, the tabs the keys choose, the footer and the help,
//! rendered to a test backend.

mod support;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use insta::assert_snapshot;
use support::tui;
use vc_tui::{Ask, Tui, TuiOptions, Update};

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
}

#[test]
fn the_help_keeps_every_key_but_its_own_and_quit_from_the_tab_under_it() {
    let mut tui = tui();
    let ask = support::press(&mut tui, &[KeyCode::Char('?'), KeyCode::Char('2'), KeyCode::Tab, KeyCode::Char('a')]);
    assert_eq!((ask, tui.tab(), screen(&tui).contains("Help")), (None, "Run", true));
    press(&mut tui, &[KeyCode::Esc, KeyCode::Char('2')]);
    assert_eq!((tui.tab(), screen(&tui).contains("Help")), ("Explore", false));
    press(&mut tui, &[KeyCode::Char('?'), KeyCode::Char('q')]);
    assert!(tui.quitting(), "q quits over the help");
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
fn q_with_ctrl_or_alt_held_does_not_quit() {
    let mut tui = tui();
    tui.press(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL));
    tui.press(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::ALT));
    assert!(!tui.quitting());
}

#[test]
fn q_during_a_run_asks_to_be_pressed_again() {
    let mut tui = tui();
    assert!(matches!(support::press(&mut tui, &[KeyCode::Char('a')]), Some(Ask::Run(_))));
    press(&mut tui, &[KeyCode::Char('q')]);
    assert!(!tui.quitting());
    assert!(screen(&tui).contains("press q again to stop it and quit"), "{}", screen(&tui));
    press(&mut tui, &[KeyCode::Char('f'), KeyCode::Char('q')]);
    assert!(!tui.quitting(), "another key in between stays");
    press(&mut tui, &[KeyCode::Char('q')]);
    assert!(tui.quitting());
    let mut tui = self::tui();
    support::press(&mut tui, &[KeyCode::Char('a'), KeyCode::Char('q')]);
    tui.update(&Update::RunFailed("the project has errors".to_owned()));
    assert!(!screen(&tui).contains("press q again"), "the run is over");
    press(&mut tui, &[KeyCode::Char('q')]);
    assert!(tui.quitting(), "with nothing in flight q quits at once");
}

#[test]
fn no_profile_shows_the_default_one() {
    let tui = Tui::new(TuiOptions::new("shop", Vec::new()));
    let header = screen(&tui).lines().next().unwrap_or_default().to_owned();
    assert!(header.starts_with("◆ vibranium shop  profile default"), "{header}");
}

#[test]
fn a_project_or_profile_name_cannot_drive_the_terminal() {
    let tui = Tui::new(TuiOptions::new("shop\u{1b}[2J", vec!["ci\u{7}".to_owned()]));
    let header = screen(&tui).lines().next().unwrap_or_default().to_owned();
    assert!(header.starts_with("◆ vibranium shop·[2J  profile ci·"), "{header}");
}
