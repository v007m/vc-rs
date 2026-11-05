use std::sync::Arc;

use serde_json::{Value, json};
use vc_model::{CelSource, RedactRule, ScalarStyle};
use vc_template::{CelCompiler, Roots};

use super::{CelRule, MaskRule, Masking, Masks, RegexRule, rule};
use crate::MASKED;
use crate::fixture::span;

fn cel(text: &str) -> CelRule {
    let source = CelSource { text: text.to_owned(), span: span(), style: ScalarStyle::Plain };
    let mut strings = vc_model::StringTable::new();
    let roots = Roots::new().in_for_each();
    CelRule::new(&CelCompiler::new().check(&source, &roots, &mut strings).unwrap())
}

fn masking<R>(masks: &Masks, rules: &[Arc<dyn MaskRule>], use_it: impl FnOnce(&Masking<'_>) -> R) -> R {
    use_it(&Masking { masks, rules })
}

#[test]
fn secret_values_are_masked_inside_any_text_and_short_ones_are_left() {
    let masks = Masks::default().with_values(&["s3cr3t-token".to_owned(), "abc".to_owned(), "s3cr3t".to_owned()]);
    masking(&masks, &[], |masking| {
        assert_eq!(masking.text("Bearer s3cr3t-token and s3cr3t, abc"), format!("Bearer {MASKED} and {MASKED}, abc"));
        assert_eq!(masking.text("nothing here"), "nothing here");
    });
    assert!(!format!("{masks:?}").contains("s3cr3t"));
    masking(&Masks::default(), &[], |masking| assert_eq!(masking.text("s3cr3t"), "s3cr3t"));
}

#[test]
fn values_walk_every_string_and_hide_named_fields_in_any_case() {
    let masks = Masks::default().with_values(&["hunter22".to_owned()]).with_fields(&["Password".to_owned()]);
    let value = json!({"user": {"PASSWORD": "x", "note": ["is hunter22", 7]}, "n": 1});
    let masked = masking(&masks, &[], |masking| masking.value(value));
    assert_eq!(masked, json!({"user": {"PASSWORD": MASKED, "note": [format!("is {MASKED}"), 7]}, "n": 1}));
}

#[test]
fn regex_rules_mask_matches_after_the_values_and_a_bad_pattern_masks_everything() {
    let masks = Masks::default().with_values(&["555-1234".to_owned()]);
    let rules: Vec<Arc<dyn MaskRule>> = vec![rule(&RedactRule::Regex(r"\d{3}-\d{2}-\d{4}".to_owned()))];
    masking(&masks, &rules, |masking| {
        assert_eq!(masking.text("ssn 123-45-6789, phone 555-1234"), format!("ssn {MASKED}, phone {MASKED}"));
        assert_eq!(masking.text("$1 stays"), "$1 stays");
    });
    let broken = RegexRule::new("(");
    assert_eq!(broken.text("anything"), Some(MASKED.to_owned()));
    assert!(!broken.field("a", &json!("b")));
    assert_eq!(RegexRule::new("x").text("abc"), None);
}

#[test]
fn cel_rules_mask_fields_they_hold_for_and_fail_closed() {
    let rules: Vec<Arc<dyn MaskRule>> = vec![Arc::new(cel("item.name == 'ssn' || item.value == 'top'"))];
    let value = json!({"ssn": "1", "plan": "top", "name": "ada"});
    let masked = masking(&Masks::default(), &rules, |masking| masking.value(value));
    assert_eq!(masked, json!({"ssn": MASKED, "plan": MASKED, "name": "ada"}));
    assert_eq!(cel("item.name == 'x'").text("x"), None);
    let failing = cel("item.value.size() > 2");
    assert!(failing.field("n", &json!(5)));
    assert!(!failing.field("n", &json!("a")));
    assert!(CelRule(None).field("n", &Value::Null));
}
