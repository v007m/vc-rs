//! The seeded random source (D133): pure, so the native build and the playground draw the
//! same values for a seed.

use std::sync::atomic::{AtomicU64, Ordering};

use vc_interfaces::Random;

/// `SplitMix64`'s step: the state moves by this odd constant on every draw.
const GOLDEN: u64 = 0x9E37_79B9_7F4A_7C15;

/// A [`Random`] that gives the same draws for the same seed (`SplitMix64`, lock-free).
#[derive(Debug)]
pub struct SeededRandom {
    state: AtomicU64,
}

impl SeededRandom {
    /// A source seeded by `seed`, as `--seed` gives it.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { state: AtomicU64::new(seed) }
    }
}

impl Random for SeededRandom {
    fn next_u64(&self) -> u64 {
        let mut mixed = self.state.fetch_add(GOLDEN, Ordering::Relaxed).wrapping_add(GOLDEN);
        mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        mixed ^ (mixed >> 31)
    }
}

#[cfg(test)]
mod tests {
    use vc_interfaces::Random;
    use vc_testkit::random_suite;

    use super::SeededRandom;

    #[test]
    fn seeded_random_meets_the_random_contract() {
        random_suite(&mut |seed| Box::new(SeededRandom::new(seed)) as Box<dyn Random>);
    }

    #[test]
    fn seeded_random_gives_splitmix64() {
        let random = SeededRandom::new(1_234_567);
        assert_eq!(random.next_u64(), 6_457_827_717_110_365_317);
        assert_eq!(random.next_u64(), 3_203_168_211_198_807_973);
    }
}
