//! `${fake.…}` generators, a registry keyed by the generator's dotted name (D131).

use std::collections::HashMap;
use std::fmt;

use serde_json::Value;
use vc_interfaces::Random;

/// One generator: draws from the run's random source, given the reference's literal arguments.
pub type FakeFn = fn(&dyn Random, &[Value]) -> Result<Value, String>;

/// The fake-data generators, by dotted name (`string.uuid`).
#[derive(Clone, Default)]
pub struct FakeData {
    by_name: HashMap<String, FakeFn>,
}

impl fmt::Debug for FakeData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut names: Vec<_> = self.by_name.keys().collect();
        names.sort_unstable();
        f.debug_set().entries(names).finish()
    }
}

impl FakeData {
    /// The placeholder set until the full catalogue lands: `string.uuid` only.
    #[must_use]
    pub fn placeholder() -> Self {
        Self::default().with("string.uuid", |random, _| Ok(uuid(random)))
    }

    /// The set with `generate` registered under `name`, replacing any earlier one.
    #[must_use]
    pub fn with(mut self, name: &str, generate: FakeFn) -> Self {
        self.by_name.insert(name.to_owned(), generate);
        self
    }

    /// A value from the generator `name`.
    ///
    /// # Errors
    ///
    /// The generator's reason when it fails, or a message when no generator has that name.
    pub fn generate(&self, name: &str, random: &dyn Random, args: &[Value]) -> Result<Value, String> {
        let generate = self.by_name.get(name).ok_or_else(|| format!("`fake.{name}` is not supported yet"))?;
        generate(random, args)
    }
}

/// A version 4 UUID from two draws.
fn uuid(random: &dyn Random) -> Value {
    let mut bytes = [0_u8; 16];
    bytes[..8].copy_from_slice(&random.next_u64().to_be_bytes());
    bytes[8..].copy_from_slice(&random.next_u64().to_be_bytes());
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let nibbles = bytes.iter().flat_map(|byte| [byte >> 4, byte & 0x0f]);
    let hex: String = nibbles.filter_map(|nibble| char::from_digit(u32::from(nibble), 16)).collect();
    let part = |range: std::ops::Range<usize>| hex.get(range).unwrap_or_default().to_owned();
    Value::String([part(0..8), part(8..12), part(12..16), part(16..20), part(20..32)].join("-"))
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use vc_interfaces::Random;
    use vc_testkit::ScriptedRandom;

    use super::FakeData;

    #[test]
    fn uuids_are_version_4_from_two_draws() {
        let random = ScriptedRandom::new([0x0123_4567_89ab_cdef, 0xfedc_ba98_7654_3210]);
        let value = FakeData::placeholder().generate("string.uuid", &random, &[]).unwrap();
        assert_eq!(value, json!("01234567-89ab-4def-bedc-ba9876543210"));
        let zero = ScriptedRandom::new([0]);
        assert_eq!(
            FakeData::placeholder().generate("string.uuid", &zero, &[]).unwrap(),
            json!("00000000-0000-4000-8000-000000000000")
        );
    }

    #[test]
    fn unknown_generators_are_not_supported_and_registered_ones_run() {
        let random = ScriptedRandom::new([7]);
        let fakes = FakeData::placeholder();
        assert_eq!(
            fakes.generate("person.fullName", &random, &[]),
            Err("`fake.person.fullName` is not supported yet".into())
        );
        let draw = |random: &dyn Random, args: &[Value]| Ok(json!([random.next_u64(), args.len()]));
        let fakes = fakes.with("number.draw", draw);
        assert_eq!(fakes.generate("number.draw", &random, &[json!(1)]), Ok(json!([7, 1])));
        assert_eq!(format!("{fakes:?}"), r#"{"number.draw", "string.uuid"}"#);
    }
}
