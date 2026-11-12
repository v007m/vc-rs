use std::time::Duration;

use prost::Message as _;

use crate::engine::v1::{RunEvent, RunReport, RunStarted, run_event};
use crate::results::v1 as wire;

#[test]
fn totals_and_statuses_take_their_wire_form() {
    let totals =
        vc_model::Totals { passed: 3, failed: 1, blocked: 0, flaky: 2, skipped: usize::MAX, allowed_failures: 4 };
    let sent = wire::Totals::from(totals);
    assert_eq!((sent.passed, sent.failed, sent.flaky, sent.skipped), (3, 1, 2, u32::MAX));
    assert_eq!(sent.allowed_failures, 4);
    assert_eq!(wire::TestStatus::from(vc_model::TestStatus::Flaky), wire::TestStatus::Flaky);
    let allowed = wire::TestStatus::from(vc_model::TestStatus::AllowedFailure);
    assert_eq!(allowed, wire::TestStatus::AllowedFailure);
    assert_eq!(wire::RunStatus::from(vc_model::RunStatus::Cancelled), wire::RunStatus::Cancelled);
    assert_eq!(wire::CheckKind::from(vc_model::CheckKind::Cel), wire::CheckKind::Cel);
    assert_eq!(wire::Protocol::from(vc_model::Protocol::Http2), wire::Protocol::Http2);
}

#[test]
fn timings_are_milliseconds_and_a_skipped_phase_is_absent() {
    let timing = vc_model::Timing {
        dns: None,
        connect: Some(Duration::from_micros(1_500)),
        total: Duration::from_millis(42),
        ..vc_model::Timing::default()
    };
    let sent = wire::Timing::from(timing);
    assert_eq!((sent.dns_ms, sent.connect_ms, sent.total_ms), (None, Some(1.5), 42.0));
}

#[test]
fn run_events_round_trip_in_binary() {
    let started = RunStarted { files: 2, instances: 5, run: Some(7), started_ms: 1_700_000_000_000 };
    let started = RunEvent { seq: 1, kind: Some(run_event::Kind::RunStarted(started)) };
    let report =
        RunEvent { seq: 9, kind: Some(run_event::Kind::Report(RunReport { exit: 1, ..RunReport::default() })) };
    for event in [started, report] {
        assert_eq!(RunEvent::decode(event.encode_to_vec().as_slice()).unwrap(), event);
    }
}

#[test]
fn every_status_keeps_its_proto_number() {
    let statuses = [
        (vc_model::TestStatus::Passed, 1),
        (vc_model::TestStatus::Failed, 2),
        (vc_model::TestStatus::Blocked, 3),
        (vc_model::TestStatus::Flaky, 4),
        (vc_model::TestStatus::Skipped, 5),
    ];
    for (status, number) in statuses {
        assert_eq!(wire::TestStatus::from(status) as i32, number);
    }
}
