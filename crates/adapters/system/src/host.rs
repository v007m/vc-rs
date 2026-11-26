use std::time::Duration;

use vc_interfaces::{HostProbe, HostSample};

/// What this process uses of the machine (D676): its CPU clock, read through `clock_gettime` on
/// Unix (none elsewhere), and its resident memory as the platform reports it.
#[derive(Clone, Copy, Debug, Default)]
#[expect(
    clippy::exhaustive_structs,
    reason = "unit adapter built by the engine builder, shared across the workspace; every user is in this repo (D160)"
)]
pub struct SystemHostProbe;

impl HostProbe for SystemHostProbe {
    fn sample(&self) -> HostSample {
        let resident = memory_stats::memory_stats().and_then(|stats| u64::try_from(stats.physical_mem).ok());
        HostSample::new(cpu_time(), resident)
    }
}

/// The CPU time the process has used, on all its threads.
#[cfg(unix)]
fn cpu_time() -> Option<Duration> {
    let time = rustix::time::clock_gettime(rustix::time::ClockId::ProcessCPUTime);
    let seconds = u64::try_from(time.tv_sec).ok()?;
    let nanos = u32::try_from(time.tv_nsec).ok()?;
    Some(Duration::new(seconds, nanos))
}

#[cfg(not(unix))]
const fn cpu_time() -> Option<Duration> {
    None
}

#[cfg(test)]
mod tests {
    use std::hint::black_box;

    use vc_interfaces::HostProbe;

    use super::SystemHostProbe;

    #[test]
    fn the_process_clock_moves_on_while_it_works_and_its_memory_is_resident() {
        let before = SystemHostProbe.sample();
        let mut sum = 0_u64;
        for step in 0..5_000_000_u64 {
            sum = black_box(sum.wrapping_add(step));
        }
        let after = SystemHostProbe.sample();
        assert!(after.resident_bytes.is_some_and(|bytes| bytes > 0), "{after:?}");
        if cfg!(unix) {
            assert!(after.cpu > before.cpu, "{before:?} then {after:?}, after summing to {sum}");
        }
    }
}
