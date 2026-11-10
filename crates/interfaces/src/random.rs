/// A source of random numbers for the `random` and `shuffle` pipes and the fake-data
/// generators, seeded by `--seed` so a run can be repeated (LLD "Vars, templates and CEL at run
/// time").
///
/// One source serves the whole run, shared by every task. With one task at a time the same
/// seed gives the same draws; with more, the order the tasks draw in may change them.
pub trait Random: Send + Sync {
    /// The next number, uniform over all of `u64`.
    fn next_u64(&self) -> u64;
}
