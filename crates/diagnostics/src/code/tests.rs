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
fn every_code_is_in_the_v1_to_v5_range_with_a_title() {
    for code in Code::ALL {
        assert!((1001..=5999).contains(&code.number()), "{code}");
        assert!(!code.title().is_empty(), "{code}");
    }
}

#[test]
fn table_has_every_category_and_the_round_15_to_17_codes() {
    let categories: BTreeSet<Category> = Code::ALL.iter().map(|code| code.category()).collect();
    assert_eq!(categories.len(), 5);
    for added in [
        "V2006", "V2007", "V2105", "V3205", "V3206", "V5108", "V5109", "V5110", "V5111", "V2106", "V2107", "V5114",
        "V1013", "V2008", "V5115", "V5116", "V5117", "V5118",
    ] {
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
    for text in ["V9999", "V02003", "V+2003", "V+203", "V0123", "V200", "2003", "v2003", "V", ""] {
        assert_eq!(text.parse::<Code>(), Err(DiagnosticsError::UnknownCode(text.to_owned())));
    }
}

#[test]
fn carries_title_and_default_severity() {
    assert_eq!(Code::V2003.title(), "Unknown api or test");
    assert_eq!(Code::V2003.default_severity(), Severity::Error);
    assert_eq!(Code::V3104.default_severity(), Severity::Warning);
    assert_eq!(Code::V5108.default_severity(), Severity::Warning, "D186: the run goes on");
    assert_eq!(Code::V5117.default_severity(), Severity::Warning, "an alert never changes the exit code");
}

#[test]
fn category_comes_from_the_first_digit() {
    let found = [Code::V1004, Code::V2003, Code::V3104, Code::V4001, Code::V5101].map(Code::category);
    let expected =
        [Category::Syntax, Category::Reference, Category::Lifecycle, Category::Convention, Category::Runtime];
    assert_eq!(found, expected);
}

#[test]
fn serializes_as_its_text() {
    assert_eq!(serde_json::to_string(&Code::V1004).unwrap(), r#""V1004""#);
}

#[test]
fn a_page_is_its_markdown_and_its_text_drops_the_marks() {
    let page = Code::V2003.explanation().unwrap();
    assert!(page.starts_with("# V2003: Unknown api or test\n"));
    let text = Code::V2003.explain();
    assert!(text.starts_with("V2003: Unknown api or test\n"), "{text}");
    assert!(text.contains("\nWrong\n") && text.contains("\nRight\n"), "{text}");
    assert!(text.contains("\n    # file: services/users/crud.test.yaml\n"), "examples are indented:\n{text}");
    assert!(!text.contains("```") && !text.contains("## "), "{text}");
}
