//! Serde helpers for the JSON values a template holds (numbers and literal arguments), so a
//! compact binary format can store them (D155). A human-readable format such as JSON writes them
//! as they are; a binary one, which cannot read back a self-describing value, gets their JSON text.

use serde::de::{DeserializeOwned, Error as _};
use serde::ser::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Writes `value` as itself, or as its JSON text for a binary format.
pub fn out<T: Serialize, S: Serializer>(value: &T, serializer: S) -> Result<S::Ok, S::Error> {
    if serializer.is_human_readable() {
        return value.serialize(serializer);
    }
    let text = serde_json::to_string(value).map_err(S::Error::custom)?;
    serializer.serialize_str(&text)
}

/// Reads back what [`out`] wrote.
pub fn read<'de, T: DeserializeOwned, D: Deserializer<'de>>(deserializer: D) -> Result<T, D::Error> {
    if deserializer.is_human_readable() {
        return T::deserialize(deserializer);
    }
    let text = String::deserialize(deserializer)?;
    serde_json::from_str(&text).map_err(D::Error::custom)
}

#[cfg(test)]
mod tests {
    use crate::{Template, TemplateValue};

    #[test]
    fn json_keeps_numbers_as_they_are() {
        let number = TemplateValue::Number(serde_json::Number::from_f64(1.5).unwrap());
        assert_eq!(serde_json::to_string(&number).unwrap(), r#"{"Number":1.5}"#);
        let list = TemplateValue::List(vec![number, TemplateValue::String(Template::default())]);
        let text = serde_json::to_string(&list).unwrap();
        assert_eq!(serde_json::from_str::<TemplateValue>(&text).unwrap(), list);
    }
}
