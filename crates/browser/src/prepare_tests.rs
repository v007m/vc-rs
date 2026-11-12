use vc_executor::MAX_FRESH_SEED;

use super::fresh_seed;

#[test]
fn a_fresh_seed_fits_a_javascript_number_as_the_engine_s_does() {
    for _ in 0..64 {
        assert!(fresh_seed() <= MAX_FRESH_SEED, "2^53 - 1 at most");
    }
}
