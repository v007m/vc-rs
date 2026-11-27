use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use vc_adapter_http::HttpTransports;
use vc_interfaces::{Clock, HostProbe, HostSample};
use vc_model::Date;

use super::Gauges;

/// A process that has used `cpu_ms` of CPU and holds 64 MiB.
#[derive(Default)]
struct Probe {
    cpu_ms: AtomicU64,
}

impl HostProbe for Probe {
    fn sample(&self) -> HostSample {
        HostSample::new(Some(Duration::from_millis(self.cpu_ms.load(Ordering::Relaxed))), Some(64 << 20))
    }
}

/// A clock at `now_ms`.
#[derive(Default)]
struct Ticking {
    now_ms: AtomicU64,
}

impl Clock for Ticking {
    fn today(&self) -> Date {
        Date::UNIX_EPOCH
    }

    fn now_ms(&self) -> u64 {
        self.now_ms.load(Ordering::Relaxed)
    }
}

/// A probe that tells nothing.
struct Blind;

impl HostProbe for Blind {
    fn sample(&self) -> HostSample {
        HostSample::default()
    }
}

#[test]
fn cpu_is_the_share_of_one_core_used_since_the_last_reading() {
    let (probe, clock) = (Arc::new(Probe::default()), Arc::new(Ticking::default()));
    let transports = Arc::new(HttpTransports::new());
    let mut gauges = Gauges::new(Arc::clone(&probe) as Arc<dyn HostProbe>, transports, Arc::clone(&clock) as _);
    probe.cpu_ms.store(250, Ordering::Relaxed);
    clock.now_ms.store(1_000, Ordering::Relaxed);
    let first = gauges.stats(3);
    assert_eq!(first.cpu_percent, Some(25));
    assert_eq!((first.memory_bytes, first.connections, first.late), (Some(64 << 20), Some(0), 3));
    probe.cpu_ms.store(2_250, Ordering::Relaxed);
    clock.now_ms.store(2_000, Ordering::Relaxed);
    assert_eq!(gauges.stats(3).cpu_percent, Some(200), "two cores busy for the second");
    assert_eq!(gauges.stats(4).cpu_percent, None, "no time has passed since the last reading");
}

#[test]
fn a_figure_nothing_tells_is_none() {
    let fixed = HttpTransports::fixed(Arc::new(vc_testkit::FakeTransport::scripted(Vec::new())));
    let mut gauges = Gauges::new(Arc::new(Blind), Arc::new(fixed), Arc::new(Ticking::default()));
    let stats = gauges.stats(0);
    assert_eq!((stats.cpu_percent, stats.memory_bytes, stats.connections), (None, None, None));
}
