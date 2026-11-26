use std::time::Duration;

use vc_contracts::engine::v1 as wire;
use vc_engine::{Event, HostStats};

use super::load_tick;

#[test]
fn a_load_tick_carries_each_percentile_in_milliseconds() {
    let [p50, p90, p95, p99] = [2_000, 4_500, 6_000, 9_250].map(Duration::from_micros);
    let breaching = vec!["errorRate".to_owned(), "users.crud#getsCreatedUser: p90".to_owned()];
    let mut host = HostStats::default();
    (host.cpu_percent, host.memory_bytes, host.late) = (Some(130), Some(1 << 20), 4);
    let tick =
        load_tick(Event::LoadTick { second: 7, users: 5, rps: 9, p50, p90, p95, p99, errors: 3, breaching, host });
    assert_eq!((tick.second, tick.users, tick.rps, tick.errors), (7, 5, 9, 3));
    let late_iterations = 4;
    let host =
        wire::HostStats { cpu_percent: Some(130), memory_bytes: Some(1 << 20), connections: None, late_iterations };
    assert_eq!(tick.host, Some(host), "an untold figure stays unset");
    assert_eq!([tick.p50_ms, tick.p90_ms, tick.p95_ms, tick.p99_ms], [2.0, 4.5, 6.0, 9.25]);
    assert_eq!(tick.breaching, ["errorRate", "users.crud#getsCreatedUser: p90"]);
    assert_eq!(load_tick(Event::RunStarted { files: 1, instances: 1 }), wire::LoadTick::default());
}
