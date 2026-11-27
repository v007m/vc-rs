use std::time::Duration;

/// Reads what the `vc` process itself uses of the machine, for a load run's Hosts panel
/// (D676): the generator's own load, so a slow p99 can be told apart from a busy generator.
///
/// The system adapter reads the process's CPU clock and resident memory; tests give fixed
/// samples.
pub trait HostProbe: Send + Sync {
    /// The process's use of the machine now. A figure the platform does not tell is `None`.
    fn sample(&self) -> HostSample;
}

/// One reading of a [`HostProbe`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct HostSample {
    /// The CPU time the process has used since it started, on all its threads.
    pub cpu: Option<Duration>,
    /// The process's resident memory, in bytes.
    pub resident_bytes: Option<u64>,
}

impl HostSample {
    /// A sample of `cpu` time used and `resident_bytes` of memory.
    #[must_use]
    pub const fn new(cpu: Option<Duration>, resident_bytes: Option<u64>) -> Self {
        Self { cpu, resident_bytes }
    }
}
