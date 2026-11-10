//! The Import tab: a form of what `vc import` takes, drafted with `d` into diffs of the files
//! it would create or change, written with `w` or discarded with `x`.

mod support;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use insta::assert_snapshot;
use support::{CURLS_URL, Outcome, importing, press, screen, tui};
use vc_engine::{Changes, Engine, ImportRequest};
use vc_tui::{Ask, Tui, Update};

const SIZE: (u16, u16) = (120, 30);

/// Types `text`, one key a character.
fn type_text(tui: &mut Tui, text: &str) {
    for typed in text.chars() {
        tui.press(KeyEvent::new(KeyCode::Char(typed), KeyModifiers::NONE));
    }
}

/// The status line.
fn status(tui: &Tui) -> String {
    screen(tui, SIZE).lines().nth(1).unwrap_or_default().to_owned()
}

/// The importer chosen.
fn importer(tui: &Tui) -> String {
    let shown = screen(tui, SIZE);
    let name = shown.split_once("‹ ").and_then(|(_, rest)| rest.split_once(" ›")).map(|(name, _)| name);
    name.unwrap_or_default().to_owned()
}

/// The UI on the Import tab, the curl importer chosen (back from openapi past http and har),
/// [`CURLS_URL`] typed as the source and `--tests` on.
fn filled() -> Tui {
    let mut tui = tui();
    press(&mut tui, &[KeyCode::Char('5'), KeyCode::Left, KeyCode::Left, KeyCode::Left]);
    press(&mut tui, &[KeyCode::Down, KeyCode::Enter]);
    type_text(&mut tui, CURLS_URL);
    press(&mut tui, &[KeyCode::Enter, KeyCode::Down, KeyCode::Down, KeyCode::Char(' ')]);
    tui
}

/// The draft of `request`, as the terminal would send it back.
async fn drafted(request: &ImportRequest) -> Outcome<Update> {
    Ok(Update::Drafted(importing()?.import(request).await?))
}

/// The changes an ask writes.
fn written(ask: Option<Ask>) -> Outcome<Changes> {
    match ask {
        Some(Ask::Write(changes)) => Ok(changes),
        other => Err(format!("w writes the draft: {other:?}").into()),
    }
}

#[test]
fn the_form_starts_empty_on_the_openapi_importer() {
    let mut tui = tui();
    assert_eq!(press(&mut tui, &[KeyCode::Char('5')]), None, "nothing is read to show it");
    assert_eq!(status(&tui), "choose the importer and the source, then d drafts the import");
    assert_snapshot!("import_form", screen(&tui, SIZE));
    assert_eq!(press(&mut tui, &[KeyCode::Char('d')]), None);
    assert_eq!(status(&tui), "error a source is needed: a URL, or a path inside the project");
    press(&mut tui, &[KeyCode::Right, KeyCode::Right]);
    assert_eq!(importer(&tui), "bruno", "→ steps round past the last importer to the first");
    press(&mut tui, &[KeyCode::Left]);
    assert_eq!(importer(&tui), "postman", "← steps back round to the last importer");
}

#[test]
fn the_importers_offered_are_the_engines() {
    let mut tui = tui();
    press(&mut tui, &[KeyCode::Char('5')]);
    let mut offered: Vec<String> = Engine::importers()
        .iter()
        .map(|_| {
            press(&mut tui, &[KeyCode::Right]);
            importer(&tui)
        })
        .collect();
    offered.sort();
    assert_eq!(offered, Engine::importers());
}

#[test]
fn the_source_is_typed_in_place_though_it_holds_q_and_digits() {
    let mut tui = tui();
    press(&mut tui, &[KeyCode::Char('5'), KeyCode::Down, KeyCode::Enter]);
    type_text(&mut tui, "specs/q1.yaml");
    assert_eq!((tui.quitting(), tui.tab()), (false, "Import"), "the keys went to the source");
    assert_eq!(status(&tui), "typing the source · enter keeps it, esc cancels");
    press(&mut tui, &[KeyCode::Enter, KeyCode::Down, KeyCode::Enter]);
    type_text(&mut tui, "dropped");
    press(&mut tui, &[KeyCode::Esc]);
    let ask = press(&mut tui, &[KeyCode::Char('d')]);
    let Some(Ask::Import(request)) = ask else { panic!("d drafts: {ask:?}") };
    let wanted = ImportRequest {
        kind: "openapi".to_owned(),
        source: "specs/q1.yaml".to_owned(),
        profiles: vec!["staging".to_owned()],
        ..ImportRequest::default()
    };
    assert_eq!(*request, wanted, "esc dropped the file ID");
    assert_eq!(status(&tui), "drafting the import of specs/q1.yaml…");
    assert_eq!(press(&mut tui, &[KeyCode::Char('d')]), None, "one draft at a time");
}

#[test]
fn editing_a_row_again_starts_from_what_it_holds() {
    let mut tui = tui();
    press(&mut tui, &[KeyCode::Char('5'), KeyCode::Down, KeyCode::Enter]);
    type_text(&mut tui, "specs/q1.yml");
    press(&mut tui, &[KeyCode::Enter, KeyCode::Enter, KeyCode::Backspace, KeyCode::Backspace]);
    type_text(&mut tui, "aml");
    press(&mut tui, &[KeyCode::Enter, KeyCode::Down, KeyCode::Enter]);
    type_text(&mut tui, "shop");
    press(&mut tui, &[KeyCode::Enter, KeyCode::Enter]);
    type_text(&mut tui, "Api");
    let ask = press(&mut tui, &[KeyCode::Enter, KeyCode::Char('d')]);
    let Some(Ask::Import(request)) = ask else { panic!("d drafts: {ask:?}") };
    assert_eq!((request.source.as_str(), request.file_id.as_deref()), ("specs/q1.yaml", Some("shopApi")));
}

#[tokio::test]
async fn d_drafts_the_files_as_diffs_and_w_writes_them() {
    let mut tui = filled();
    let ask = press(&mut tui, &[KeyCode::Char('d')]);
    let Some(Ask::Import(request)) = ask else { panic!("d drafts: {ask:?}") };
    assert_eq!((request.kind.as_str(), request.source.as_str(), request.tests), ("curl", CURLS_URL, true));
    let draft = drafted(&request).await.unwrap();
    tui.update(&draft);
    assert_eq!(status(&tui), "2 files drafted · n shows the next diff, w writes them, x discards");
    assert_snapshot!("import_drafted", screen(&tui, SIZE));
    press(&mut tui, &[KeyCode::Char('n')]);
    assert!(screen(&tui, SIZE).contains("+++ b/vibranium.config.yaml"), "n shows the next file's diff");
    let changes = written(press(&mut tui, &[KeyCode::Char('w')])).unwrap();
    let Update::Drafted(draft) = draft else { panic!("a draft") };
    assert_eq!(changes, draft, "what was shown is what is written");
    assert_eq!(status(&tui), "writing 2 files…");
    tui.update(&Update::Written(changes.titles));
    assert!(status(&tui).starts_with("wrote: Import "), "{}", status(&tui));
}

#[tokio::test]
async fn x_discards_the_draft_and_a_refusal_shows_its_help() {
    let mut tui = filled();
    let Some(Ask::Import(request)) = press(&mut tui, &[KeyCode::Char('d')]) else { panic!("d drafts") };
    tui.update(&drafted(&request).await.unwrap());
    press(&mut tui, &[KeyCode::Char('x')]);
    assert_eq!(status(&tui), "choose the importer and the source, then d drafts the import");
    assert_eq!(press(&mut tui, &[KeyCode::Char('w')]), None, "nothing to write");
    let gone = ImportRequest { source: "https://specs.test/gone.curl".to_owned(), ..*request };
    let refused = importing().unwrap().import(&gone).await.unwrap_err();
    press(&mut tui, &[KeyCode::Char('d')]);
    let help = refused.help().unwrap_or_default();
    tui.update(&Update::ImportFailed(format!("{refused}\nhelp: {help}")));
    assert!(status(&tui).starts_with("error "), "{}", status(&tui));
    assert!(screen(&tui, SIZE).contains("help: give a URL, or a path inside the project"));
}

#[tokio::test]
async fn a_failed_write_or_redraft_keeps_the_draft_and_says_why_on_the_status_line() {
    let mut tui = filled();
    let Some(Ask::Import(request)) = press(&mut tui, &[KeyCode::Char('d')]) else { panic!("d drafts") };
    tui.update(&drafted(&request).await.unwrap());
    let shown = status(&tui);
    tui.update(&Update::ImportFailed("nothing asked for this".to_owned()));
    assert_eq!(status(&tui), shown, "a failure nothing asked for is ignored");
    written(press(&mut tui, &[KeyCode::Char('w')])).unwrap();
    tui.update(&Update::ImportFailed("disk full\nhelp: free some space".to_owned()));
    assert_eq!(status(&tui), "error disk full");
    assert!(screen(&tui, SIZE).contains("+++ b/"), "the draft is still shown");
    written(press(&mut tui, &[KeyCode::Char('w')])).unwrap();
    tui.update(&Update::ImportFailed("disk full".to_owned()));
    assert!(matches!(press(&mut tui, &[KeyCode::Char('d')]), Some(Ask::Import(_))), "d drafts again");
    tui.update(&Update::ImportFailed("the source moved\nhelp: x".to_owned()));
    assert_eq!(status(&tui), "error the source moved");
    assert!(written(press(&mut tui, &[KeyCode::Char('w')])).is_ok(), "the draft before is kept");
}

#[tokio::test]
async fn pgdn_and_pgup_scroll_the_diff_shown() {
    let mut tui = filled();
    let Some(Ask::Import(request)) = press(&mut tui, &[KeyCode::Char('d')]) else { panic!("d drafts") };
    tui.update(&drafted(&request).await.unwrap());
    let top = screen(&tui, SIZE);
    press(&mut tui, &[KeyCode::PageDown]);
    assert_ne!(screen(&tui, SIZE), top, "pgdn scrolls the diff on");
    press(&mut tui, &[KeyCode::PageUp]);
    assert_eq!(screen(&tui, SIZE), top, "pgup scrolls it back");
}
