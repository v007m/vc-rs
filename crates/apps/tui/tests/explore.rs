//! The Explore tab: it reads the project when first shown, lists the apis and tests by file,
//! explains the chosen one, skips a test with a typed reason or unskips it, and edits the chosen
//! one's file through the compile guard.

mod support;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use insta::assert_snapshot;
use support::{Outcome, engine, press, screen, tui};
use vc_engine::{Engine, EngineError, ExplainRequest, FileChange, SelectRequest, SkipRequest};
use vc_tui::{Ask, Tui, Update};

const SIZE: (u16, u16) = (100, 24);

/// What the terminal would send back for [`Ask::Explore`]: the apis, then the tests.
fn explored(engine: &Engine) -> Outcome<Update> {
    let report = engine.catalog(&SelectRequest::default())?;
    Ok(Update::Explored { items: report.items, notices: report.notices })
}

/// The UI on the Explore tab, the project read: `listsOrders`, then the users apis `list`
/// and `create`, then `listsUsers`, `createsUser` and the skipped `deletesUser`.
fn on_explore(engine: &Engine) -> Outcome<Tui> {
    let mut tui = tui();
    if press(&mut tui, &[KeyCode::Char('2')]) != Some(Ask::Explore) {
        return Err("the tab reads the project when first shown".into());
    }
    tui.update(&explored(engine)?);
    Ok(tui)
}

/// Types `text`, one key a character.
fn type_text(tui: &mut Tui, text: &str) -> Option<Ask> {
    text.chars().fold(None, |_, typed| tui.press(KeyEvent::new(KeyCode::Char(typed), KeyModifiers::NONE)))
}

/// The status line.
fn status(tui: &Tui) -> String {
    screen(tui, SIZE).lines().nth(1).unwrap_or_default().to_owned()
}

/// Presses `typed` with ctrl held.
fn ctrl(tui: &mut Tui, typed: char) -> Option<Ask> {
    tui.press(KeyEvent::new(KeyCode::Char(typed), KeyModifiers::CONTROL))
}

/// The UI editing the file `e` opens on `listsOrders`, given the engine's text of it; the path.
fn editing(engine: &Engine) -> Outcome<(Tui, String)> {
    let mut tui = on_explore(engine)?;
    let Some(Ask::Open(path)) = press(&mut tui, &[KeyCode::Char('e')]) else {
        return Err("e opens the chosen item's file".into());
    };
    let file = engine.read_file(&path)?;
    tui.update(&Update::Opened { path: path.clone(), text: file.text, digest: file.digest });
    Ok((tui, path))
}

#[test]
fn the_project_is_read_when_the_tab_is_first_shown() {
    let mut tui = tui();
    assert_eq!(press(&mut tui, &[KeyCode::Char('2')]), Some(Ask::Explore));
    assert_eq!(status(&tui), "reading the project…");
    assert_eq!(press(&mut tui, &[KeyCode::Char('r')]), None, "it is being read");
    tui.update(&explored(&engine().unwrap()).unwrap());
    assert_eq!(press(&mut tui, &[KeyCode::Char('1'), KeyCode::Char('2')]), None, "read already");
    assert_eq!(press(&mut tui, &[KeyCode::Char('r')]), Some(Ask::Explore));
    tui.update(&Update::ExploreFailed("users.api.yaml:3: bad indent\n  help: indent by two".to_owned()));
    assert_eq!(status(&tui), "error users.api.yaml:3: bad indent");
}

#[test]
fn the_apis_and_tests_show_by_file_each_folders_apis_first() {
    let tui = on_explore(&engine().unwrap()).unwrap();
    assert_snapshot!("explore_project", screen(&tui, SIZE));
}

#[test]
fn enter_explains_the_chosen_item_under_the_active_profiles() {
    let engine = engine().unwrap();
    let mut tui = on_explore(&engine).unwrap();
    let ask = press(&mut tui, &[KeyCode::Down, KeyCode::Down, KeyCode::Down, KeyCode::Enter]);
    let Some(Ask::Explain { id, profiles }) = ask else { panic!("enter explains: {ask:?}") };
    assert_eq!((id.as_str(), profiles.as_slice()), ("users.crud#listsUsers", ["staging".to_owned()].as_slice()));
    assert!(screen(&tui, SIZE).contains("explaining users.crud#listsUsers…"));
    tui.update(&Update::Explained { id: "users#list".to_owned(), text: "GET http://x/users".to_owned() });
    assert!(screen(&tui, SIZE).contains("explaining users.crud#listsUsers…"), "an answer to another ask is ignored");
    let report = engine.explain(&id, &ExplainRequest { profiles, ..ExplainRequest::default() }).unwrap();
    tui.update(&Update::Explained { id, text: report.text });
    assert_snapshot!("explore_explained", screen(&tui, SIZE));
}

#[test]
fn t_skips_a_test_with_the_reason_typed_though_it_holds_q_and_digits() {
    let engine = engine().unwrap();
    let mut tui = on_explore(&engine).unwrap();
    assert_eq!(press(&mut tui, &[KeyCode::Down, KeyCode::Down, KeyCode::Down, KeyCode::Char('t')]), None);
    assert_eq!(type_text(&mut tui, "quota 2 ?"), None);
    assert_eq!((tui.quitting(), tui.tab()), (false, "Explore"), "the keys went to the reason");
    assert!(status(&tui).starts_with("skip users.crud#listsUsers because: quota 2 ?▏  enter skips"));
    let ask = press(&mut tui, &[KeyCode::Backspace, KeyCode::Enter]);
    let Some(Ask::Skip(request)) = ask else { panic!("enter skips: {ask:?}") };
    let wanted = SkipRequest { target: "users.crud#listsUsers".to_owned(), reason: "quota 2".to_owned(), until: None };
    assert_eq!(request, wanted);
    assert_eq!(status(&tui), "skipping users.crud#listsUsers…");
    let diff: String = engine.skip(&request).unwrap().files.iter().map(FileChange::diff).collect();
    assert!(diff.contains("+    skip: quota 2"), "{diff}");
    tui.update(&Update::Quarantined { target: request.target, skipped: true });
    assert_eq!(status(&tui), "users.crud#listsUsers skipped");
}

#[test]
fn the_test_skipped_is_the_one_chosen_when_t_was_pressed_though_a_re_read_moved_the_cursor() {
    let engine = engine().unwrap();
    let mut tui = on_explore(&engine).unwrap();
    press(&mut tui, &[KeyCode::Down, KeyCode::Down, KeyCode::Down, KeyCode::Char('t')]);
    type_text(&mut tui, "quota");
    let Update::Explored { mut items, notices } = explored(&engine).unwrap() else { panic!("the project") };
    items.retain(|item| !item.id.ends_with("#listsOrders"));
    tui.update(&Update::Explored { items, notices });
    assert!(status(&tui).starts_with("skip users.crud#listsUsers because: quota▏"), "{}", status(&tui));
    let ask = press(&mut tui, &[KeyCode::Enter]);
    let Some(Ask::Skip(request)) = ask else { panic!("enter skips: {ask:?}") };
    assert_eq!(request.target, "users.crud#listsUsers");
}

#[test]
fn c_shows_only_what_changed_in_the_last_week_until_pressed_again() {
    let engine = engine().unwrap();
    let mut tui = on_explore(&engine).unwrap();
    let week = std::time::Duration::from_hours(7 * 24);
    assert_eq!(press(&mut tui, &[KeyCode::Char('c')]), Some(Ask::ExploreChanged(week)));
    assert_eq!(press(&mut tui, &[KeyCode::Char('c')]), None, "it is being read");
    let Update::Explored { mut items, notices } = explored(&engine).unwrap() else { panic!("the project") };
    items.retain(|item| item.id.starts_with("orders.flow#"));
    tui.update(&Update::Explored { items, notices });
    assert!(
        status(&tui).starts_with("1 tests and 0 apis changed in the last 7 days · c shows all"),
        "{}",
        status(&tui)
    );
    assert_eq!(press(&mut tui, &[KeyCode::Char('r')]), Some(Ask::ExploreChanged(week)), "r keeps the filter");
    tui.update(&explored(&engine).unwrap());
    assert_eq!(press(&mut tui, &[KeyCode::Char('c')]), Some(Ask::Explore));
    tui.update(&explored(&engine).unwrap());
    assert!(!status(&tui).contains("changed in the last"), "{}", status(&tui));
}

#[test]
fn the_chosen_item_stays_chosen_when_a_reread_drops_one_above_it() {
    let engine = engine().unwrap();
    let mut tui = on_explore(&engine).unwrap();
    press(&mut tui, &[KeyCode::Down, KeyCode::Down, KeyCode::Down]);
    assert_eq!(press(&mut tui, &[KeyCode::Char('r')]), Some(Ask::Explore));
    let Update::Explored { mut items, notices } = explored(&engine).unwrap() else { panic!("the project") };
    items.retain(|item| !item.id.ends_with("#listsOrders"));
    tui.update(&Update::Explored { items, notices });
    let ask = press(&mut tui, &[KeyCode::Enter]);
    assert!(matches!(&ask, Some(Ask::Explain { id, .. }) if id == "users.crud#listsUsers"), "{ask:?}");
}

#[test]
fn a_blank_reason_is_refused_esc_drops_it_and_ctrl_c_still_quits() {
    let mut tui = on_explore(&engine().unwrap()).unwrap();
    assert_eq!(press(&mut tui, &[KeyCode::Char('t'), KeyCode::Char(' '), KeyCode::Enter]), None);
    assert!(status(&tui).ends_with("a skip needs a reason, for the report"), "{}", status(&tui));
    press(&mut tui, &[KeyCode::Esc, KeyCode::Char('3')]);
    assert_eq!(tui.tab(), "History", "esc dropped the reason, so the digits choose tabs again");
    press(&mut tui, &[KeyCode::Char('2'), KeyCode::Char('t')]);
    tui.press(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert!(tui.quitting());
}

#[test]
fn t_unskips_a_skipped_test_and_refuses_an_api() {
    let engine = engine().unwrap();
    let mut tui = on_explore(&engine).unwrap();
    assert_eq!(press(&mut tui, &[KeyCode::Down, KeyCode::Char('t')]), None);
    assert_eq!(status(&tui), "error users#list is an api: t skips a test");
    let ask = press(&mut tui, &[KeyCode::Down, KeyCode::Down, KeyCode::Down, KeyCode::Down, KeyCode::Char('t')]);
    assert_eq!(ask, Some(Ask::Unskip("users.crud#deletesUser".to_owned())));
    assert!(!engine.unskip("users.crud#deletesUser").unwrap().is_empty());
    tui.update(&Update::SkipFailed("crud.test.yaml could not be written: read-only".to_owned()));
    assert_eq!(status(&tui), "error crud.test.yaml could not be written: read-only");
}

#[test]
fn what_did_not_stop_the_reading_is_shown_on_the_status_line() {
    let mut tui = on_explore(&engine().unwrap()).unwrap();
    let Update::Explored { items, .. } = explored(&engine().unwrap()).unwrap() else { panic!("the project") };
    let notices = vec!["plugin `audit` is not trusted\n  help: trust it first".to_owned(), "other".to_owned()];
    tui.update(&Update::Explored { items, notices });
    let status = screen(&tui, (140, 24)).lines().nth(1).unwrap_or_default().to_owned();
    assert!(status.ends_with("  warning: plugin `audit` is not trusted (+1 more)"), "{status}");
}

#[test]
fn a_failure_clears_only_what_it_answers() {
    let mut tui = on_explore(&engine().unwrap()).unwrap();
    press(&mut tui, &[KeyCode::Down, KeyCode::Down, KeyCode::Down, KeyCode::Enter]);
    let explaining = "explaining users.crud#listsUsers…";
    tui.update(&Update::SkipFailed("crud.test.yaml could not be written: read-only".to_owned()));
    tui.update(&Update::ExploreFailed("users.api.yaml:3: bad indent".to_owned()));
    let failed = |id: &str| Update::ExplainFailed { id: id.to_owned(), reason: "no such test\n  help: x".to_owned() };
    tui.update(&failed("users#list"));
    assert!(screen(&tui, SIZE).contains(explaining), "a read, skip or other explain failure leaves it");
    tui.update(&failed("users.crud#listsUsers"));
    assert!(!screen(&tui, SIZE).contains(explaining));
    assert_eq!(status(&tui), "error no such test");
}

#[test]
fn the_project_is_read_again_when_shown_after_the_import_tab_wrote_files() {
    let engine = engine().unwrap();
    let mut tui = on_explore(&engine).unwrap();
    press(&mut tui, &[KeyCode::Char('5')]);
    tui.update(&Update::Written(vec!["Import orders".to_owned()]));
    assert_eq!(press(&mut tui, &[KeyCode::Char('2')]), Some(Ask::Explore), "the catalog shown is stale");
    tui.update(&explored(&engine).unwrap());
    assert_eq!(press(&mut tui, &[KeyCode::Char('1'), KeyCode::Char('2')]), None, "read again already");
}

#[test]
fn pgdn_and_pgup_scroll_a_long_explanation() {
    let mut tui = on_explore(&engine().unwrap()).unwrap();
    let ask = press(&mut tui, &[KeyCode::Down, KeyCode::Down, KeyCode::Down, KeyCode::Enter]);
    let Some(Ask::Explain { id, .. }) = ask else { panic!("enter explains: {ask:?}") };
    let text = (0..40).map(|at| format!("step {at:02}")).collect::<Vec<_>>().join("\n");
    tui.update(&Update::Explained { id, text });
    assert!(screen(&tui, SIZE).contains("step 00"));
    press(&mut tui, &[KeyCode::PageDown]);
    let scrolled = screen(&tui, SIZE);
    assert!(!scrolled.contains("step 00") && scrolled.contains("step 10"), "{scrolled}");
    press(&mut tui, &[KeyCode::PageUp]);
    assert!(screen(&tui, SIZE).contains("step 00"));
}

#[test]
fn pgdn_reaches_the_end_of_an_explanation_whose_long_lines_wrap() {
    let mut tui = on_explore(&engine().unwrap()).unwrap();
    let ask = press(&mut tui, &[KeyCode::Down, KeyCode::Down, KeyCode::Down, KeyCode::Enter]);
    let Some(Ask::Explain { id, .. }) = ask else { panic!("enter explains: {ask:?}") };
    let line = |at: usize| (0..80).map(|word| format!("w{at}x{word:02}")).collect::<Vec<_>>().join(" ");
    tui.update(&Update::Explained { id, text: (0..3).map(line).collect::<Vec<_>>().join("\n") });
    assert!(!screen(&tui, SIZE).contains("w2x79"), "three lines that wrap past the pane");
    press(&mut tui, &[KeyCode::PageDown, KeyCode::PageDown, KeyCode::PageDown, KeyCode::PageDown]);
    let scrolled = screen(&tui, SIZE);
    assert!(scrolled.contains("w2x79") && !scrolled.contains("w0x00"), "{scrolled}");
}

#[test]
fn e_edits_the_chosen_file_ctrl_s_drafts_it_through_the_guard_and_w_writes_the_draft() {
    let engine = engine().unwrap();
    let (mut tui, path) = editing(&engine).unwrap();
    assert_eq!(path, "services/orders/flow.test.yaml");
    assert!(screen(&tui, SIZE).contains(&format!("Edit {path}")), "the file is open");
    press(&mut tui, &[KeyCode::Down, KeyCode::End]);
    assert_eq!(type_text(&mut tui, "Again"), None, "the keys go to the text");
    let ask = ctrl(&mut tui, 's');
    let Some(Ask::Edit(request)) = ask else { panic!("ctrl-s drafts: {ask:?}") };
    assert!(request.text.contains("id: listsOrdersAgain") && request.expected_digest.is_some(), "{request:?}");
    let changes = engine.edit_file(&request).unwrap();
    tui.update(&Update::Edited(changes.clone()));
    assert!(screen(&tui, SIZE).contains("+  - id: listsOrdersAgain"), "the draft's diff is shown");
    assert_eq!(press(&mut tui, &[KeyCode::Char('w')]), Some(Ask::Save(changes.clone())));
    tui.update(&Update::Saved(changes.titles));
    assert_eq!(status(&tui), format!("wrote {path}"));
    press(&mut tui, &[KeyCode::Char('3')]);
    assert_eq!(tui.tab(), "History", "the editor closed, so the digits choose tabs again");
}

#[test]
fn an_edit_that_adds_an_error_is_refused_with_it_and_the_text_kept_to_fix() {
    let engine = engine().unwrap();
    let (mut tui, _) = editing(&engine).unwrap();
    press(&mut tui, &[KeyCode::Down, KeyCode::Down, KeyCode::End]);
    type_text(&mut tui, "x");
    let Some(Ask::Edit(request)) = ctrl(&mut tui, 's') else { panic!("ctrl-s drafts") };
    let refused = engine.edit_file(&request).unwrap_err();
    assert!(matches!(&refused, EngineError::Edit { .. }), "{refused:?}");
    tui.update(&Update::EditFailed(refused.to_string()));
    let shown = screen(&tui, SIZE);
    assert!(shown.contains("Refused") && shown.contains("nothing was written"), "{shown}");
    assert!(shown.contains("users#listx"), "the text stays, to fix: {shown}");
    press(&mut tui, &[KeyCode::Backspace]);
    assert!(matches!(ctrl(&mut tui, 's'), Some(Ask::Edit(_))), "fixed, it drafts again");
}

#[test]
fn esc_drops_a_changed_text_only_once_confirmed_and_an_unchanged_draft_is_not_written() {
    let engine = engine().unwrap();
    let (mut tui, path) = editing(&engine).unwrap();
    let Some(Ask::Edit(request)) = ctrl(&mut tui, 's') else { panic!("ctrl-s drafts") };
    tui.update(&Update::Edited(engine.edit_file(&request).unwrap()));
    assert_eq!(status(&tui), "nothing changed · esc goes back to the text");
    assert_eq!(press(&mut tui, &[KeyCode::Char('w')]), None, "nothing to write");
    press(&mut tui, &[KeyCode::Esc]);
    type_text(&mut tui, "# ");
    press(&mut tui, &[KeyCode::Esc]);
    assert!(status(&tui).starts_with(&format!("drop the changes to {path}?")), "{}", status(&tui));
    press(&mut tui, &[KeyCode::Char('n')]);
    assert!(screen(&tui, SIZE).contains(&format!("Edit {path} *")), "any key but y keeps the edit");
    press(&mut tui, &[KeyCode::Esc, KeyCode::Char('y')]);
    assert!(!screen(&tui, SIZE).contains(&format!("Edit {path}")), "y drops it");
}
