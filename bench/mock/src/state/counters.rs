use std::collections::HashMap;

/// Call counts per key, for the scripted `/flaky` and `/poll/{id}` routes.
#[derive(Default)]
pub struct Counters {
    calls: HashMap<String, u64>,
}

impl Counters {
    /// Records one more call for `key` and returns how many calls it has had, this one included.
    pub fn bump(&mut self, key: String) -> u64 {
        let calls = self.calls.entry(key).or_default();
        *calls += 1;
        *calls
    }
}
