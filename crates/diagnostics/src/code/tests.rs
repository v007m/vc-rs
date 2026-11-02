use std::collections::BTreeSet;

use super::{Category, Code};
use crate::{DiagnosticsError, Severity};

#[test]
fn codes_are_unique_and_in_numeric_order() {
    let numbers: Vec<u16> = Code::ALL.iter().map(|code| code.number()).collect();
    let unique: BTreeSet<u16> = numbers.iter().copied().collect();
    assert_eq!(unique.len(), numbers.len());
    assert!(numbers.is_sorted());
}

#[test]
fn every_code_is_in_its_category_range() {
    for code in Code::ALL {
        assert_eq!(code.number() / 1000, code.category().digit(), "{code}");
        assert!((1001..=5999).contains(&code.number()), "{code}");
        assert!(!code.title().is_empty(), "{code}");
    }
}

#[test]
fn table_has_every_category_and_the_round_15_to_17_codes() {
    let categories: BTreeSet<Category> = Code::ALL.iter().map(|code| code.category()).collect();
    assert_eq!(categories.len(), 5);
    for added in ["V2006", "V2007", "V2105", "V3205", "V3206", "V5108", "V5109", "V5110", "V5111"] {
        assert!(added.parse::<Code>().is_ok(), "{added}");
    }
}

#[test]
fn prints_and_parses_as_v_number() {
    assert_eq!(Code::V2003.to_string(), "V2003");
    assert_eq!("V2003".parse::<Code>(), Ok(Code::V2003));
    for code in Code::ALL {
        assert_eq!(code.to_string().parse::<Code>().as_ref(), Ok(code));
    }
}

#[test]
fn rejects_unknown_or_malformed_codes() {
    for text in ["V9999", "V02003", "2003", "v2003", ""] {
        assert_eq!(text.parse::<Code>(), Err(DiagnosticsError::UnknownCode(text.to_owned())));
    }
}

#[test]
fn carries_title_and_default_severity() {
    assert_eq!(Code::V2003.title(), "Unknown api or test");
    assert_eq!(Code::V2003.default_severity(), Severity::Error);
    assert_eq!(Code::V3104.default_severity(), Severity::Warning);
    assert_eq!(Code::V4001.category(), Category::Convention);
}

#[test]
fn serializes_as_its_text() {
    assert_eq!(serde_json::to_string(&Code::V1004).unwrap(), r#""V1004""#);
}
