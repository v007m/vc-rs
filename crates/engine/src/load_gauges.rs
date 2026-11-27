//! The Hosts panel's figures as each second of a load run closes (D674–D677): the generator's
//! CPU and memory from the host probe, the transports' open connections and the late starts.

use std::sync::Arc;
use std::time::Duration;

use vc_interfaces::{Clock, HostProbe, TransportFactory};
use vc_model::HostStats;

/// Reads the run's host figures, keeping the last CPU reading to tell the use since.
pub struct Gauges {
    probe: Arc<dyn HostProbe>,
    transports: Arc<dyn TransportFactory>,
    clock: Arc<dyn Clock>,
    /// When the CPU clock was last read, in ms since the epoch, and what it read.
    last: Option<(u64, Duration)>,
}

impl Gauges {
    /// Gauges reading `probe` and `transports`, timed by `clock`, from a first CPU reading now.
    pub fn new(probe: Arc<dyn HostProbe>, transports: Arc<dyn TransportFactory>, clock: Arc<dyn Clock>) -> Self {
        let last = probe.sample().cpu.map(|cpu| (clock.now_ms(), cpu));
        Self { probe, transports, clock, last }
    }

    /// The figures now, with `late` iterations so far.
    pub fn stats(&mut self, late: u64) -> HostStats {
        let sample = self.probe.sample();
        let mut stats = HostStats::default();
        stats.cpu_percent = sample.cpu.and_then(|cpu| self.cpu_percent(cpu));
        stats.memory_bytes = sample.resident_bytes;
        stats.connections = self.transports.open_connections();
        stats.late = late;
        stats
    }

    /// The CPU used since the last reading, in percent of one core over the time between;
    /// `None` when no time has passed or nothing was read before.
    fn cpu_percent(&mut self, cpu: Duration) -> Option<u32> {
        let now = self.clock.now_ms();
        let (then, before) = self.last.replace((now, cpu))?;
        let wall = now.checked_sub(then).filter(|wall| *wall > 0)?;
        let used = cpu.saturating_sub(before).as_millis();
        u32::try_from(used.saturating_mul(100) / u128::from(wall)).ok()
    }
}

#[cfg(test)]
#[path = "load_gauges_tests.rs"]
mod tests;
