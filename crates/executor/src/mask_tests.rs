use std::sync::Arc;

use serde_json::{Value, json};
use vc_model::{CelSource, Config, RedactRule, ScalarStyle};
use vc_template::{CelCompiler, Roots};

use super::{CelRule, MaskRule, MaskRuleMaker, MaskRules, Masking, Masks, RedactRuleKind, RegexRule};
use crate::MASKED;
use crate::fixture::span;

fn cel_program(text: &str) -> vc_model::CelProgram {
    let source = CelSource { text: text.to_owned(), span: span(), style: ScalarStyle::Plain };
    let mut strings = vc_model::StringTable::new();
    let roots = Roots::new().in_for_each();
    CelCompiler::new().check(&source, &roots, &mut strings).unwrap()
}

fn cel(text: &str) -> CelRule {
    CelRule::new(&cel_program(text))
}

const SSN: &str = r"\d{3}-\d{2}-\d{4}";

#[test]
fn secret_values_are_masked_inside_any_text_and_short_ones_are_left() {
    let masks = Masks::default().with_values(&["s3cr3t-token".to_owned(), "abc".to_owned(), "s3cr3t".to_owned()]);
    assert_eq!(
        masks.without_values("Bearer s3cr3t-token and s3cr3t, abc"),
        format!("Bearer {MASKED} and {MASKED}, abc")
    );
    assert_eq!(masks.without_values("nothing here"), "nothing here");
    assert!(!format!("{masks:?}").contains("s3cr3t"));
    assert_eq!(Masks::default().without_values("s3cr3t"), "s3cr3t");
}

#[test]
fn values_walk_every_string_and_hide_named_fields_in_any_case() {
    let masks = Masks::default().with_values(&["hunter22".to_owned()]).with_fields(&["Password".to_owned()]);
    let value = json!({"user": {"PASSWORD": "x", "note": ["is hunter22", 7]}, "n": 1});
    let masked = masks.value(value);
    assert_eq!(masked, json!({"user": {"PASSWORD": MASKED, "note": [format!("is {MASKED}"), 7]}, "n": 1}));
}

#[tokio::test]
async fn regex_rules_mask_matches_after_the_values_and_a_bad_pattern_masks_everything() {
    let masks = Masks::default().with_values(&["555-1234".to_owned()]);
    let rules = [MaskRules::standard().rule(&RedactRule::Regex(SSN.to_owned()))];
    let masking = Masking { masks: &masks, rules: &rules };
    assert_eq!(masking.text("ssn 123-45-6789, phone 555-1234").await, format!("ssn {MASKED}, phone {MASKED}"));
    assert_eq!(masking.text("$1 stays").await, "$1 stays");
    let value = masking.value(json!({"a": ["123-45-6789"], "n": 5})).await;
    assert_eq!(value, json!({"a": [MASKED], "n": 5}));
    let broken = RegexRule::new("(");
    assert_eq!(broken.text("anything".to_owned()).await, MASKED);
    assert_eq!(broken.value(json!({"a": "b"})).await, json!({"a": MASKED}));
    assert_eq!(RegexRule::new("x").text("abc".to_owned()).await, "abc");
}

#[tokio::test]
async fn cel_rules_mask_fields_they_hold_for_and_fail_closed() {
    let rules: Vec<Arc<dyn MaskRule>> = vec![Arc::new(cel("item.name == 'ssn' || item.value == 'top'"))];
    let masking = Masking { masks: &Masks::default(), rules: &rules };
    let value = json!({"ssn": "1", "plan": "top", "name": "ada", "deep": {"ssn": 2}});
    let masked = masking.value(value).await;
    assert_eq!(masked, json!({"ssn": MASKED, "plan": MASKED, "name": "ada", "deep": {"ssn": MASKED}}));
    assert_eq!(cel("item.name == 'x'").text("x".to_owned()).await, "x");
    let failing = cel("item.value.size() > 2");
    assert_eq!(failing.value(json!({"n": 5, "s": "a"})).await, json!({"n": MASKED, "s": "a"}));
    assert_eq!(CelRule(None).value(json!({"n": null})).await, json!({"n": MASKED}));
}

/// Remembers every text it was given, and masks nothing.
struct Seen(std::sync::mpsc::Sender<String>);

#[async_trait::async_trait]
impl MaskRule for Seen {
    async fn text(&self, text: String) -> String {
        self.0.send(text.clone()).ok();
        text
    }

    async fn value(&self, value: Value) -> Value {
        self.0.send(value.to_string()).ok();
        value
    }
}

#[tokio::test]
async fn a_rule_runs_after_the_built_in_masking_and_never_sees_a_secret() {
    let (sender, seen) = std::sync::mpsc::channel();
    let rules: Vec<Arc<dyn MaskRule>> = vec![Arc::new(Seen(sender))];
    let masks = Masks::default().with_values(&["s3cr3t-key".to_owned()]).with_fields(&["pw".to_owned()]);
    let masking = Masking { masks: &masks, rules: &rules };
    assert_eq!(masking.text("key s3cr3t-key").await, format!("key {MASKED}"));
    assert_eq!(masking.value(json!({"pw": "hunter22", "k": "s3cr3t-key"})).await, json!({"pw": MASKED, "k": MASKED}));
    let seen: Vec<String> = seen.try_iter().collect();
    assert_eq!(seen.len(), 2);
    assert!(seen.iter().all(|text| !text.contains("s3cr3t") && !text.contains("hunter22")), "{seen:?}");
}

#[tokio::test]
async fn the_registry_makes_each_kind_through_its_maker_and_masks_everything_without_one() {
    let standard = MaskRules::standard();
    assert_eq!(format!("{standard:?}"), "{Regex, Cel}");
    let config = Config {
        redact_rules: vec![RedactRule::Regex(SSN.to_owned()), RedactRule::Cel(cel_program("item.name == 'ssn'"))],
        ..Config::default()
    };
    let rules = standard.for_config(&config);
    let masking = Masking { masks: &Masks::default(), rules: &rules };
    assert_eq!(masking.value(json!({"ssn": 1, "id": "123-45-6789"})).await, json!({"ssn": MASKED, "id": MASKED}));
    let blind = MaskRules::default().rule(&RedactRule::Cel(cel_program("false")));
    assert_eq!((blind.text("a".to_owned()).await, blind.value(json!(1)).await), (MASKED.to_owned(), json!(MASKED)));
    let wrong_kind: MaskRuleMaker = Arc::new(|_| None);
    let refusing = MaskRules::standard().with(RedactRuleKind::Regex, wrong_kind);
    assert_eq!(refusing.rule(&RedactRule::Regex("x".to_owned())).text("abc".to_owned()).await, MASKED);
    assert_eq!(RedactRuleKind::of(&RedactRule::Cel(cel_program("true"))), RedactRuleKind::Cel);
}
