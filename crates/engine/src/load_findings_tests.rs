use serde_json::json;
use vc_interfaces::{FindingKind, FindingRow, Row};

use super::{rows, text};
use crate::{ErrorKind, LoadBreach, LoadReport};

fn report() -> LoadReport {
    let breach = LoadBreach {
        metric: "p95".into(),
        test: Some("users.crud#getsUser".into()),
        actual: 45.5,
        shown: "45.5 ms (lt 30 ms)".into(),
        ..LoadBreach::default()
    };
    let error = |test: Option<&str>, api: Option<&str>, count| ErrorKind {
        api: api.map(Into::into),
        test: test.map(Into::into),
        status: api.map(|_| 503),
        code: Some("V5101".into()),
        count,
    };
    let errors =
        vec![error(Some("a.b#lists"), Some("users#list"), 3), error(None, None, 1), error(Some("a.b#lists"), None, 2)];
    LoadReport { breaches: vec![breach], error_kinds: errors, ..LoadReport::default() }
}

fn found(report: &LoadReport) -> Vec<FindingRow> {
    rows(report).into_iter().filter_map(|row| if let Row::Finding(found) = row { Some(found) } else { None }).collect()
}

#[test]
fn a_report_gives_its_breaches_then_its_errors_with_the_lines_it_printed() {
    let found = found(&report());
    let kinds: Vec<FindingKind> = found.iter().map(|row| row.kind).collect();
    assert_eq!(kinds, [FindingKind::Breach, FindingKind::Error, FindingKind::Error, FindingKind::Error]);
    assert_eq!(found[0].shown, "breached users.crud#getsUser: p95 at 45.5 ms (lt 30 ms)");
    assert_eq!((found[0].metric.as_deref(), found[0].actual), (Some("p95"), Some(45.5)));
    assert_eq!(found[1].shown, "errors users#list 503 V5101 × 3");
    assert_eq!((found[1].api_id.as_deref(), found[1].status, found[1].count), (Some("users#list"), Some(503), Some(3)));
}

#[test]
fn errors_print_under_their_test_in_the_order_first_seen() {
    let shown = text(&found(&report()));
    let expected = "  breached users.crud#getsUser: p95 at 45.5 ms (lt 30 ms)\n  test a.b#lists\n    errors users#list \
                    503 V5101 × 3\n    errors V5101 × 2\n  test \n    errors V5101 × 1\n";
    assert_eq!(shown, expected);
    assert_eq!(text(&[]), "");
}

#[test]
fn a_finding_reads_as_json_with_nulls_for_what_it_lacks() {
    let breach = super::json(&found(&report())[0]);
    let expected = json!({"kind": "breach", "test": "users.crud#getsUser", "api": null, "metric": "p95",
        "status": null, "code": null, "count": null, "actual": 45.5,
        "shown": "breached users.crud#getsUser: p95 at 45.5 ms (lt 30 ms)"});
    assert_eq!(breach, expected);
}
