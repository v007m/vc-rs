use serde_json::json;
use vc_contracts::engine::v1 as wire;
use vc_engine::{ErrorKind, LoadBreach, LoadEnding, LoadReport};

use super::load_report;

#[test]
fn a_breached_run_keeps_its_breaches_and_error_kinds_on_the_wire() {
    let mut breach = LoadBreach::default();
    (breach.metric, breach.actual, breach.unit) = ("p95".into(), 45.5, "ms".into());
    (breach.limit, breach.shown) = (json!({"lt": 30.0}), "45.5 ms (lt 30 ms)".into());
    let mut kind = ErrorKind::default();
    (kind.api, kind.status, kind.code, kind.count) = (Some("users#list".into()), Some(503), Some("V5101".into()), 4);
    let mut report = LoadReport::default();
    (report.exit, report.ending, report.breaches, report.error_kinds) =
        (3, LoadEnding::Aborted, vec![breach], vec![kind]);
    let wire = load_report(report);
    assert_eq!((wire.exit, wire.ending()), (3, wire::LoadEnding::Aborted));
    let [breach] = wire.breaches.as_slice() else { panic!("{wire:?}") };
    assert_eq!((breach.limit_json.as_str(), breach.shown.as_str()), (r#"{"lt":30.0}"#, "45.5 ms (lt 30 ms)"));
    let [kind] = wire.error_kinds.as_slice() else { panic!("{wire:?}") };
    assert_eq!((kind.status, kind.code.as_deref(), kind.count), (Some(503), Some("V5101"), 4));
}
