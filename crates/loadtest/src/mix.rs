//! The traffic mix: which test each iteration runs, picked by weight in constant time.

use vc_model::{MixEntry, TestId};

use crate::LoadError;

/// A precomputed alias table (Vose) over the mix's weights.
///
/// Built with integers only, so each test's share is exactly its weight over the total. A pick
/// takes one random `u64`: its high half chooses a column, its low half the coin that keeps the
/// column's test or switches to its alias. Tests of weight 0 are never picked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mix {
    tests: Vec<TestId>,
    keep: Vec<u64>,
    alias: Vec<usize>,
    total: u64,
}

impl Mix {
    /// The mix of `entries`.
    ///
    /// # Errors
    ///
    /// [`LoadError::EmptyMix`] when no entry has a weight above 0.
    pub fn new(entries: &[MixEntry]) -> Result<Self, LoadError> {
        let weighted: Vec<&MixEntry> = entries.iter().filter(|entry| entry.weight > 0).collect();
        let total: u64 = weighted.iter().map(|entry| u64::from(entry.weight)).sum();
        if total == 0 {
            return Err(LoadError::EmptyMix);
        }
        let columns = u64::try_from(weighted.len()).unwrap_or(u64::MAX);
        let scaled = weighted.iter().map(|entry| u128::from(entry.weight) * u128::from(columns)).collect();
        let (keep, alias) = table(scaled, u128::from(total));
        Ok(Self { tests: weighted.iter().map(|entry| entry.test).collect(), keep, alias, total })
    }

    /// The test the random number `random` picks.
    #[must_use]
    pub fn pick(&self, random: u64) -> TestId {
        let columns = u128::try_from(self.tests.len()).unwrap_or(u128::MAX);
        let column = usize::try_from((u128::from(random >> 32) * columns) >> 32).unwrap_or(0);
        let coin = (u128::from(random & 0xFFFF_FFFF) * u128::from(self.total)) >> 32;
        let chosen = if coin < u128::from(self.keep[column]) { column } else { self.alias[column] };
        // In bounds: the column is below the test count, an alias is a column and the mix has a test.
        self.tests[chosen]
    }

    /// The tests that can be picked, in the mix's order.
    #[must_use]
    pub fn tests(&self) -> &[TestId] {
        &self.tests
    }
}

/// Vose's alias method on `scaled` (each weight times the column count) against `total`, the
/// capacity of a column: for each column, how much of `total` keeps its own test, and the column
/// whose test the rest goes to.
fn table(mut scaled: Vec<u128>, total: u128) -> (Vec<u64>, Vec<usize>) {
    let mut keep = vec![total; scaled.len()];
    let mut alias: Vec<usize> = (0..scaled.len()).collect();
    let (mut small, mut large): (Vec<usize>, Vec<usize>) = (0..scaled.len()).partition(|&i| scaled[i] < total);
    while let (Some(&under), Some(&over)) = (small.last(), large.last()) {
        small.pop();
        large.pop();
        keep[under] = scaled[under];
        alias[under] = over;
        scaled[over] = scaled[over] + scaled[under] - total;
        if scaled[over] < total { small.push(over) } else { large.push(over) }
    }
    (keep.into_iter().map(|share| u64::try_from(share).unwrap_or(u64::MAX)).collect(), alias)
}

#[cfg(test)]
#[path = "mix_tests.rs"]
mod tests;
