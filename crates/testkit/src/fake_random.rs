use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use vc_interfaces::{Environment, Random};

/// A [`Random`] that gives `draws` in order, then starts over; `0` forever when empty.
#[derive(Debug, Default)]
pub struct ScriptedRandom {
    draws: Vec<u64>,
    next: AtomicUsize,
}

impl ScriptedRandom {
    /// A source giving `draws` in order, over and over.
    #[must_use]
    pub fn new(draws: impl Into<Vec<u64>>) -> Self {
        Self { draws: draws.into(), next: AtomicUsize::new(0) }
    }

    /// A source with draws spread from `seed`, for tests that need a plausible sequence.
    #[must_use]
    pub fn seeded(seed: u64) -> Self {
        let step = 0x9E37_79B9_7F4A_7C15_u64;
        Self::new((1..=64).map(|n: u64| seed.wrapping_add(n).wrapping_mul(step)).collect::<Vec<_>>())
    }
}

impl Random for ScriptedRandom {
    fn next_u64(&self) -> u64 {
        let at = self.next.fetch_add(1, Ordering::Relaxed);
        self.draws.get(at.checked_rem(self.draws.len()).unwrap_or(0)).copied().unwrap_or(0)
    }
}

/// An [`Environment`] holding the vars it was given.
#[derive(Clone, Debug, Default)]
pub struct MemoryEnvironment {
    vars: HashMap<String, String>,
}

impl MemoryEnvironment {
    /// An environment holding exactly `vars`.
    #[must_use]
    pub fn new(vars: &[(&str, &str)]) -> Self {
        Self { vars: vars.iter().map(|(name, value)| ((*name).to_owned(), (*value).to_owned())).collect() }
    }
}

impl Environment for MemoryEnvironment {
    fn var(&self, name: &str) -> Option<String> {
        self.vars.get(name).cloned()
    }

    fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.vars.keys().cloned().collect();
        names.sort_unstable();
        names
    }
}

#[cfg(test)]
mod tests {
    use vc_interfaces::{Environment, Random};

    use super::{MemoryEnvironment, ScriptedRandom};
    use crate::{environment_suite, random_suite};

    #[test]
    fn scripted_random_meets_the_random_contract() {
        random_suite(&mut |seed| Box::new(ScriptedRandom::seeded(seed)) as Box<dyn Random>);
    }

    #[test]
    fn scripted_random_repeats_its_draws() {
        let random = ScriptedRandom::new([3, 9]);
        let draws: Vec<_> = (0..5).map(|_| random.next_u64()).collect();
        assert_eq!(draws, [3, 9, 3, 9, 3]);
        assert_eq!(ScriptedRandom::default().next_u64(), 0);
    }

    #[test]
    fn memory_environment_meets_the_environment_contract() {
        environment_suite(&mut |vars| Box::new(MemoryEnvironment::new(vars)) as Box<dyn Environment>);
    }
}
