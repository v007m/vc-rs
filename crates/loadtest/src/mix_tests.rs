use indexmap::IndexMap;
use vc_model::{MixEntry, StringTable, TestId};

use super::Mix;
use crate::LoadError;

fn ids(count: usize) -> Vec<TestId> {
    let mut strings = StringTable::new();
    (0..count).map(|i| TestId::new(strings.intern(&format!("t.f#t{i}")).unwrap())).collect()
}

fn entries(weights: &[u32]) -> (Vec<TestId>, Vec<MixEntry>) {
    let tests = ids(weights.len());
    let entries = tests
        .iter()
        .zip(weights)
        .map(|(&test, &weight)| MixEntry { test, weight, thresholds: IndexMap::new() })
        .collect();
    (tests, entries)
}

/// Each test's exact share of the table, in units of `total / columns`: what keeps it in its
/// own column plus what other columns hand to it.
fn shares(mix: &Mix) -> Vec<u128> {
    let mut shares = vec![0_u128; mix.tests.len()];
    for (column, (&keep, &alias)) in mix.keep.iter().zip(&mix.alias).enumerate() {
        shares[column] += u128::from(keep);
        shares[alias] += u128::from(mix.total - keep);
    }
    shares
}

#[test]
fn each_test_gets_exactly_its_weight_over_the_total() {
    for weights in [&[6, 3, 1][..], &[1], &[1, 1], &[5, 0, 7, 1, 1, 9], &[u32::MAX, 1, u32::MAX]] {
        let (_, entries) = entries(weights);
        let mix = Mix::new(&entries).unwrap();
        let columns = u128::try_from(mix.tests.len()).unwrap();
        let expected: Vec<u128> = weights.iter().filter(|&&w| w > 0).map(|&w| u128::from(w) * columns).collect();
        assert_eq!(shares(&mix), expected, "weights {weights:?}");
    }
}

#[test]
fn picks_follow_the_weights() {
    let (tests, entries) = entries(&[6, 3, 1]);
    let mix = Mix::new(&entries).unwrap();
    let mut counts = [0_u32; 3];
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    for _ in 0..100_000 {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        let picked = mix.pick(z ^ (z >> 31));
        counts[tests.iter().position(|&t| t == picked).unwrap()] += 1;
    }
    for (count, expected) in counts.iter().zip([60_000, 30_000, 10_000]) {
        assert!(count.abs_diff(expected) < 1_000, "{counts:?}");
    }
}

#[test]
fn a_zero_weight_test_is_never_picked() {
    let (tests, entries) = entries(&[0, 2]);
    let mix = Mix::new(&entries).unwrap();
    assert_eq!(mix.tests(), &tests[1..]);
    assert!([0, u64::MAX, 1 << 63, 0xFFFF_FFFF].iter().all(|&random| mix.pick(random) == tests[1]));
}

#[test]
fn extreme_random_numbers_stay_in_the_table() {
    let (tests, entries) = entries(&[1, 2, 3]);
    let mix = Mix::new(&entries).unwrap();
    assert_eq!(mix.pick(0), tests[0]);
    assert!(tests.contains(&mix.pick(u64::MAX)));
}

#[test]
fn a_mix_without_weight_is_refused() {
    assert_eq!(Mix::new(&entries(&[0, 0]).1), Err(LoadError::EmptyMix));
    assert_eq!(Mix::new(&[]), Err(LoadError::EmptyMix));
}
