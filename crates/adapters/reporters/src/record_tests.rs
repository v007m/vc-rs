use vc_interfaces::{Row, TestRow};
use vc_model::{Event, TestStatus};

use crate::testing::{self, Capture};

/// The scripted run with one exchange before every finished action.
fn with_exchanges(events: Vec<Event>) -> Vec<Event> {
    let mut out = Vec::new();
    for event in events {
        if let Event::ActionFinished { action, owner, .. } = &event {
            let exchange = Box::new(testing::exchange("{\"email\":\"x\"}", "{\"id\":1}"));
            out.push(Event::Exchanged { action: *action, owner: *owner, exchange });
        }
        out.push(event);
    }
    out
}

fn recorded() -> (Vec<Row>, crate::RunSnapshot) {
    let (capture, _captured) = Capture::new();
    let context = testing::context(capture);
    let events = with_exchanges(testing::script(&context.program(), context.plan()));
    testing::record(&context, &events)
}

/// A test row's ID, example row and repeat.
type Position<'a> = (&'a str, Option<u32>, u32);

fn tests(rows: &[Row]) -> Vec<&TestRow> {
    rows.iter().filter_map(|row| if let Row::Test(test) = row { Some(test) } else { None }).collect()
}

#[test]
fn gives_a_row_per_finished_test_with_its_checks() {
    let (rows, _) = recorded();
    let tests = tests(&rows);
    let ids: Vec<Position<'_>> = tests.iter().map(|t| (t.test_id.as_str(), t.example_row, t.repeat_index)).collect();
    assert_eq!(ids[1..3], [("users.crud#rejectsBadEmail", Some(0), 0), ("users.crud#rejectsBadEmail", Some(1), 0)]);
    assert_eq!(ids[4], ("users.crud#listsUsers", None, 1));
    let statuses: Vec<_> = tests.iter().map(|t| t.status).collect();
    assert_eq!(statuses[2], Some(TestStatus::Blocked));
    assert_eq!(tests[2].blocked_by.as_deref(), Some("seed"));
    assert_eq!(tests[1].message.as_deref(), Some("expected 400, got 201"));
    assert_eq!(tests[5].message.as_deref(), Some("rewrite pending"));
}

#[test]
fn keeps_each_check_with_its_source() {
    let (rows, _) = recorded();
    let tests = tests(&rows);
    let check = &tests[1].checks[1];
    assert_eq!(
        (check.kind.as_str(), check.path.as_deref(), check.passed),
        ("body.match", Some("errors[0].field"), false)
    );
    assert_eq!((check.expected.as_str(), check.file.as_str()), ("\"email\"", "services/users/crud.test.yaml"));
    assert!(check.line > 0 && check.origin.starts_with("test "), "{check:?}");
    assert!(tests[0].duration_ms > 0.0);
}

#[test]
fn keeps_bodies_only_where_history_bodies_says() {
    let (rows, _) = recorded();
    let Row::Exchange(seed) = &rows[0] else { panic!("the lifecycle exchange comes first: {rows:?}") };
    assert_eq!((seed.action.as_str(), seed.api_id.as_deref()), ("seed", Some("users#create")));
    assert_eq!(seed.request_body, None, "a passed lifecycle action keeps no bodies by default");
    let tests = tests(&rows);
    assert_eq!(tests[0].exchanges.len(), 1);
    assert_eq!(tests[0].exchanges[0].response_body, None, "a passed test keeps no bodies");
    assert_eq!(tests[1].exchanges[0].response_body.as_deref(), Some(&b"{\"id\":1}"[..]), "a failed one does");
    assert_eq!(tests[3].exchanges.len(), 2, "both attempts of the flaky test");
    assert_eq!((tests[2].exchanges.len(), tests[5].exchanges.len()), (0, 0), "never ran");
}

#[test]
fn snapshots_the_whole_run() {
    let (_, snapshot) = recorded();
    assert_eq!(snapshot.files, ["users.crud"]);
    assert_eq!(snapshot.tests.len(), 6);
    assert_eq!(snapshot.lifecycle.len(), 1);
    assert_eq!(snapshot.cleanups.len(), 1);
    let failed = &snapshot.tests[1];
    assert!(failed.detail.contains("errors[0].field"), "{}", failed.detail);
    assert_eq!(failed.exchanges[0].api.as_deref(), Some("users#create"));
    assert_eq!(snapshot.tests[0].detail, "");
    assert_eq!(snapshot.run().id, 7);
}

#[test]
fn keeps_the_config_s_project_for_the_reports() {
    let (capture, _captured) = Capture::new();
    let (program, plan) = testing::project();
    let mut named = (*program).clone();
    named.config.project = Some("shop".to_owned());
    let context = crate::ReporterContext::new(std::sync::Arc::new(named), plan, capture);
    let snapshot = crate::Recorder::new(&context).finish(crate::RunFacts::of(7, &testing::summary()));
    assert_eq!(snapshot.project(), "shop");
    assert_eq!(testing::snapshot().project(), "vibranium", "no `project`, the fallback");
}
