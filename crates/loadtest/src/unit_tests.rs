use serde_json::json;

use super::Unit;

#[test]
fn a_value_shows_in_its_unit_with_no_trailing_zeros() {
    assert_eq!(Unit::MS.show(45.4712), "45.471 ms");
    assert_eq!(Unit::MS.show(30.0), "30 ms");
    assert_eq!(Unit::SHARE.show(1.0), "100%");
    assert_eq!(Unit::SHARE.show(0.0125), "1.25%");
    assert_eq!(Unit::PER_SECOND.show(12.5), "12.5/s");
}

#[test]
fn a_limit_shows_each_number_in_the_unit() {
    assert_eq!(Unit::MS.limit(&json!({"lt": 30.0})), "lt 30 ms");
    assert_eq!(Unit::SHARE.limit(&json!({"lt": 0.01})), "lt 1%");
    assert_eq!(Unit::PER_SECOND.limit(&json!({"gte": 20, "lt": 50})), "gte 20/s, lt 50/s");
    assert_eq!(Unit::MS.limit(&json!({"between": [1, 5]})), "between 1 ms, 5 ms");
    assert_eq!(Unit::MS.limit(&json!("fast")), "\"fast\"");
}

#[test]
fn a_breach_gets_the_decimals_it_needs_not_to_read_as_its_limit() {
    assert_eq!(Unit::MS.breach(31.25, &json!({"lt": 30.0})), "31.25 ms (lt 30 ms)");
    assert_eq!(Unit::MS.breach(30.0004, &json!({"lte": 30.0})), "30.0004 ms (lte 30 ms)");
    assert_eq!(Unit::SHARE.breach(0.000_001, &json!({"lte": 0})), "0.0001% (lte 0%)");
    assert_eq!(Unit::PER_SECOND.breach(0.000_04, &json!({"between": [1, 5]})), "0.00004/s (between 1/s, 5/s)");
    assert_eq!(Unit::MS.breach(30.0, &json!({"lt": 30.0})), "30 ms (lt 30 ms)", "an equal actual is shown as it is");
    assert_eq!(Unit::SHARE.breach(1e-12, &json!({"lte": 0})), "0% (lte 0%)", "six decimals at most");
}

#[test]
fn a_json_value_keeps_the_units_places() {
    assert!((Unit::MS.round(47.711_000_000_000_006) - 47.711).abs() < f64::EPSILON);
    assert!((Unit::SHARE.round(0.123_456) - 0.1235).abs() < f64::EPSILON);
    assert_eq!(Unit::MS.name(), "ms");
    assert_eq!(Unit::SHARE.name(), "share");
    assert_eq!(Unit::PER_SECOND.name(), "perSecond");
}
