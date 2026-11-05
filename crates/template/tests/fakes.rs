#![cfg(test)]

use std::cell::Cell;

use proptest::prelude::{prop, prop_assert, prop_assert_eq, proptest};
use regex::Regex;
use serde_json::{Value, json};
use vc_diagnostics::Code;
use vc_template::{Chance, FakeCall, FakeCx, FakeDef, FakeRegistry, ValueType};

/// 2024-05-01T00:00:00Z.
const STARTED_MS: u64 = 1_714_521_600_000;

/// A splitmix64 stream from a seed.
struct Seeded(Cell<u64>);

impl Chance for Seeded {
    fn next_u64(&self) -> u64 {
        let state = self.0.get().wrapping_add(0x9e37_79b9_7f4a_7c15);
        self.0.set(state);
        let mixed = (state ^ (state >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        let mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        mixed ^ (mixed >> 31)
    }
}

/// The given draws, then zeros.
struct Scripted {
    draws: Vec<u64>,
    next: Cell<usize>,
}

impl Chance for Scripted {
    fn next_u64(&self) -> u64 {
        let at = self.next.get();
        self.next.set(at + 1);
        self.draws.get(at).copied().unwrap_or(0)
    }
}

fn scripted(draws: &[u64]) -> Scripted {
    Scripted { draws: draws.to_vec(), next: Cell::new(0) }
}

/// A generator, its arguments, the code it is rejected with and part of the message.
type Rejected = (&'static str, Vec<Value>, Code, &'static str);

fn draw(name: &str, args: &[Value], seed: u64) -> Value {
    let chance = Seeded(Cell::new(seed));
    FakeRegistry::builtin().generate(name, args, &FakeCx::new(&chance, STARTED_MS)).unwrap()
}

fn rejection(name: &str, args: &[Value]) -> (Code, String) {
    let error = FakeRegistry::builtin().check(name, args).unwrap().unwrap_err();
    (error.code, error.message)
}

#[test]
fn every_generator_without_arguments_gives_its_type() {
    let fakes = FakeRegistry::builtin();
    for name in fakes.names() {
        let Some(Ok(output)) = fakes.check(name, &[]) else { continue };
        let value = draw(name, &[], 7);
        let fits = match output {
            ValueType::Text => value.as_str().is_some_and(|text| !text.is_empty()),
            ValueType::Number => value.is_number(),
            _ => true,
        };
        assert!(fits, "{name}: {value}");
    }
    assert_eq!(fakes.check("helpers.arrayElement", &[json!(1)]), Some(Ok(ValueType::Any)));
    assert_eq!(fakes.check("person.age", &[]), None);
}

#[test]
fn the_registry_lists_names_and_modules_and_takes_more() {
    let fakes = FakeRegistry::builtin();
    let names = fakes.names();
    assert_eq!((names.len(), names.first().copied()), (25, Some("commerce.price")));
    let modules: Vec<&str> = fakes.modules().into_iter().collect();
    let expected = ["commerce", "date", "helpers", "internet", "location", "lorem", "number", "person", "phone"];
    assert_eq!(modules, [&expected[..], &["string"]].concat());
    let answer = |_: &[Value], _: &FakeCx<'_>| Ok(json!(42));
    let fakes = fakes.with("acme.answer", FakeDef::new(|_| Ok(()), ValueType::Number, answer));
    assert_eq!(draw_from(&fakes, "acme.answer"), Ok(json!(42)));
    assert_eq!(draw_from(&fakes, "acme.none"), Err("`fake.acme.none` is not a generator".to_owned()));
    assert!(format!("{:?}", FakeCx::new(&scripted(&[]), 5)).contains("started_ms: 5"));
}

fn draw_from(fakes: &FakeRegistry, name: &str) -> Result<Value, String> {
    fakes.generate(name, &[], &FakeCx::new(&scripted(&[]), 0))
}

#[test]
fn uuids_are_version_4_from_two_draws() {
    let fakes = FakeRegistry::builtin();
    let chance = scripted(&[0x0123_4567_89ab_cdef, 0xfedc_ba98_7654_3210]);
    let value = fakes.generate("string.uuid", &[], &FakeCx::new(&chance, 0)).unwrap();
    assert_eq!(value, json!("01234567-89ab-4def-bedc-ba9876543210"));
    let value = fakes.generate("string.uuid", &[], &FakeCx::new(&scripted(&[]), 0)).unwrap();
    assert_eq!(value, json!("00000000-0000-4000-8000-000000000000"));
}

#[test]
fn counts_set_how_much_text_is_made() {
    assert_eq!(draw("lorem.words", &[json!(5)], 1).as_str().unwrap().split(' ').count(), 5);
    assert_eq!(draw("lorem.words", &[], 1).as_str().unwrap().split(' ').count(), 3);
    assert_eq!(draw("lorem.paragraphs", &[json!(2)], 1).as_str().unwrap().lines().count(), 2);
    let code = draw("string.alphanumeric", &[json!(12)], 1);
    assert!(code.as_str().is_some_and(|code| code.len() == 12 && code.chars().all(|c| c.is_ascii_alphanumeric())));
    assert_eq!(draw("string.alphanumeric", &[], 1).as_str().map(str::len), Some(1));
    let fakes = FakeRegistry::builtin();
    let draws = scripted(&[0, 26, 61]);
    let cx = FakeCx::new(&draws, 0);
    assert_eq!(fakes.generate("string.alphanumeric", &[json!(3)], &cx), Ok(json!("Aa9")));
    let street = draw("location.streetAddress", &[], 3);
    assert!(street.as_str().is_some_and(|street| street.split_once(' ').is_some()), "{street}");
}

#[test]
fn numbers_stay_in_their_range() {
    let fakes = FakeRegistry::builtin();
    let int = |args: &[Value], draws: &[u64]| fakes.generate("number.int", args, &FakeCx::new(&scripted(draws), 0));
    assert_eq!(int(&[json!(1), json!(3)], &[0]), Ok(json!(1)));
    assert_eq!(int(&[json!(1), json!(3)], &[5]), Ok(json!(3)));
    assert_eq!(int(&[json!(-2), json!(-2)], &[9]), Ok(json!(-2)));
    assert_eq!(int(&[], &[u64::MAX]), Ok(json!(u64::MAX % 9_007_199_254_740_992)));
    assert_eq!(int(&[json!(3), json!(1)], &[]), Err("`min` is above `max`".to_owned()));
}

#[test]
fn floats_and_prices_stay_in_their_range() {
    let fakes = FakeRegistry::builtin();
    let float = |name: &str, args: &[Value], draw: u64| {
        fakes.generate(name, args, &FakeCx::new(&scripted(&[draw]), 0)).unwrap().as_f64().unwrap()
    };
    assert!((float("number.float", &[], 1 << 63) - 0.5).abs() < f64::EPSILON);
    assert!((float("number.float", &[json!(2), json!(4)], 1 << 62) - 2.5).abs() < f64::EPSILON);
    assert!((float("commerce.price", &[], 0) - 1.0).abs() < f64::EPSILON);
    assert!((float("commerce.price", &[json!(1), json!(2)], 0x5555_5555_0000_0000) - 1.33).abs() < 1e-9);
    let price = float("commerce.price", &[], u64::MAX);
    let cents = price * 100.0;
    assert!(price <= 1000.0 && (cents - cents.round()).abs() < 1e-6, "{price}");
}

#[test]
fn dates_are_rfc_3339_around_the_run_start() {
    let at = |name: &str, draw: u64| {
        let draws = scripted(&[draw]);
        let cx = FakeCx::new(&draws, STARTED_MS);
        FakeRegistry::builtin().generate(name, &[], &cx).unwrap()
    };
    assert_eq!(at("date.past", 0), json!("2024-04-30T23:59:59.999Z"));
    assert_eq!(at("date.future", 0), json!("2024-05-01T00:00:00.001Z"));
    assert_eq!(at("date.recent", 86_399_999), json!("2024-04-30T00:00:00Z"));
    assert_eq!(at("date.past", 365 * 86_400_000 - 1), json!("2023-05-02T00:00:00Z"));
    assert_eq!(at("date.future", 365 * 86_400_000 - 1), json!("2025-05-01T00:00:00Z"));
    let draws = scripted(&[]);
    let cx = FakeCx::new(&draws, u64::MAX);
    assert_eq!(FakeRegistry::builtin().generate("date.future", &[], &cx), Err("the date is out of range".to_owned()));
}

#[test]
fn array_element_picks_one_argument() {
    let fakes = FakeRegistry::builtin();
    let choices = [json!("PRIVATE"), json!("PUBLIC")];
    let pick = |draw: u64| fakes.generate("helpers.arrayElement", &choices, &FakeCx::new(&scripted(&[draw]), 0));
    assert_eq!((pick(0), pick(3)), (Ok(json!("PRIVATE")), Ok(json!("PUBLIC"))));
    let none = fakes.generate("helpers.arrayElement", &[], &FakeCx::new(&scripted(&[]), 0));
    assert_eq!(none, Err("there is nothing to choose from".to_owned()));
}

#[test]
fn wrong_arguments_are_v2302_and_bad_patterns_v2309() {
    let cases: [Rejected; 9] = [
        ("person.fullName", vec![json!(1)], Code::V2302, "takes no arguments"),
        ("lorem.words", vec![json!(-1)], Code::V2302, "from 0 to 10000"),
        ("lorem.words", vec![json!(10_001)], Code::V2302, "from 0 to 10000"),
        ("lorem.words", vec![json!(1), json!(2)], Code::V2302, "from 0 to 10000"),
        ("number.int", vec![json!(5), json!(1)], Code::V2302, "min then max"),
        ("number.int", vec![json!(1.5), json!(2)], Code::V2302, "two whole numbers"),
        ("number.float", vec![json!(2), json!(1)], Code::V2302, "two numbers"),
        ("string.fromRegExp", vec![json!(1)], Code::V2302, "takes one pattern"),
        ("string.fromRegExp", vec![json!("(?=a)b")], Code::V2309, "lookarounds"),
    ];
    for (name, args, code, reason) in cases {
        let (found, message) = rejection(name, &args);
        assert_eq!(found, code, "{name}");
        assert!(message.contains(reason), "{name}: {message}");
    }
    assert_eq!(rejection("helpers.arrayElement", &[]).0, Code::V2302);
    let fine: [(&str, Vec<Value>); 4] = [
        ("number.int", vec![json!(1), json!(1)]),
        ("number.float", vec![json!(0.5), json!(1)]),
        ("lorem.words", vec![json!(10_000)]),
        ("string.fromRegExp", vec![json!("[A-Z]{3}")]),
    ];
    assert!(fine.iter().all(|(name, args)| FakeRegistry::builtin().check(name, args).is_some_and(|it| it.is_ok())));
}

#[test]
fn named_generators_lower_to_builtin_calls() {
    let fakes = FakeRegistry::builtin();
    let lowered = |kind: &str, settings: Value| fakes.named(kind, &settings).map(|call| (call.name, call.args));
    assert_eq!(lowered("fromRegExp", json!("[a-z]{2}")), Ok(("string.fromRegExp", vec![json!("[a-z]{2}")])));
    assert_eq!(lowered("oneOf", json!(["A", 1])), Ok(("helpers.arrayElement", vec![json!("A"), json!(1)])));
    assert_eq!(lowered("int", json!({"min": 1, "max": 5})), Ok(("number.int", vec![json!(1), json!(5)])));
    assert_eq!(lowered("int", json!({})), Ok(("number.int", vec![json!(0), json!(9_007_199_254_740_991_i64)])));
    assert_eq!(lowered("lorem", json!({"words": 500})), Ok(("lorem.words", vec![json!(500)])));
    assert_eq!(lowered("lorem", json!({})), Ok(("lorem.words", vec![])));
    assert_eq!(FakeRegistry::kinds(), ["fromRegExp", "oneOf", "int", "lorem"]);
}

#[test]
fn named_generators_with_bad_settings_are_rejected() {
    let fakes = FakeRegistry::builtin();
    let rejected =
        |kind: &str, settings: Value| fakes.named(kind, &settings).map_err(|error| (error.code, error.message));
    let shape = |code| Err((code, "has settings of the wrong shape".to_owned()));
    assert_eq!(rejected("fromRegExp", json!(3)), shape(Code::V2302));
    assert_eq!(rejected("oneOf", json!("A")), shape(Code::V2302));
    assert_eq!(rejected("int", json!(3)), shape(Code::V2302));
    assert_eq!(rejected("lorem", json!([])), shape(Code::V2302));
    assert_eq!(rejected("pick", json!(1)), Err((Code::V2302, "`pick` is not a generator kind".to_owned())));
    assert_eq!(rejected("fromRegExp", json!("a\\b")).map_err(|error| error.0), Err(Code::V2309));
    assert_eq!(rejected("int", json!({"min": 9, "max": 1})).map_err(|error| error.0), Err(Code::V2302));
    assert_eq!(rejected("oneOf", json!([])).map_err(|error| error.0), Err(Code::V2302));
    let call: FakeCall = fakes.named("oneOf", &json!([1])).unwrap();
    assert_eq!(call.clone(), call);
}

#[test]
fn from_reg_exp_makes_matching_text() {
    let patterns = [
        "[A-Z]{3}-[0-9]{4}",
        "^[A-Z][a-z]{4,12} Team$",
        "(red|green|blue)-\\d{2,}",
        "(?:ab|cd)*x+y?z{0,2}",
        "[^,\\s]{5}\\.\\w\\W\\D\\S",
        "[\\d_a-c]{3}\\t\\n\\(\\)",
        "a*?b+?c??d{2}?",
        "[]a][-x][a-]",
        "^a|b$",
        "a$|b",
    ];
    for pattern in patterns {
        let full = Regex::new(&format!("^(?:{pattern})$")).unwrap();
        for seed in 0..50 {
            let text = draw("string.fromRegExp", &[json!(pattern)], seed);
            assert!(full.is_match(text.as_str().unwrap()), "{pattern}: {text}");
        }
    }
}

#[test]
fn unsupported_patterns_say_why() {
    let deep = "(".repeat(40);
    let cases = [
        ("a\\1", "`\\1` is not supported"),
        ("a\\b", "`\\b` is not supported"),
        ("(?<name>a)", "lookarounds and named groups"),
        ("(?!a)", "lookarounds and named groups"),
        ("*a", "`*` has nothing to repeat"),
        ("a{2", "is not closed"),
        ("a{x}", "must start a repeat"),
        ("a{3,1}", "is out of order"),
        ("a{257}", "repeat counts go up to 256"),
        ("a**", "a repeat cannot follow a repeat"),
        ("a++", "a repeat cannot follow a repeat"),
        ("(a", "a `(` is not closed"),
        ("a)", "a `)` closes no group"),
        ("[a", "a `[` is not closed"),
        ("[z-a]", "is out of order"),
        ("[a-\\d]", "cannot end in a class"),
        ("[[a-c]]", "nested classes are not supported"),
        ("[,-[a]]", "nested classes are not supported"),
        ("[a&&b]", "set operations"),
        ("[a--]", "set operations"),
        ("[^ -~]", "matches no printable character"),
        ("a^b", "only at the start or end"),
        ("a$b", "only at the start or end"),
        ("\\", "ends with `\\`"),
        ("(a{256}){256}", "longer than 10000"),
        (deep.as_str(), "nest deeper than 32"),
    ];
    for (pattern, reason) in cases {
        let (code, message) = rejection("string.fromRegExp", &[json!(pattern)]);
        assert_eq!(code, Code::V2309, "{pattern}");
        assert!(message.contains(reason), "{pattern}: {message}");
    }
}

/// Pieces of pattern syntax, put together at random.
const SYNTAX: [&str; 26] = [
    "a", "b", "0", "-", ",", "[", "]", "^", "(", ")", "(?:", "|", "*", "+", "?", "{2}", "{1,3}", "{2,}", ".", "\\d",
    "\\w", "\\s", "\\W", "\\.", "[a-c]", "[^a]",
];

proptest! {
    #[test]
    fn generators_repeat_for_a_seed(seed in 0_u64..u64::MAX, pick in 0_usize..25) {
        let fakes = FakeRegistry::builtin();
        let name = fakes.names()[pick];
        let args = match name {
            "helpers.arrayElement" => vec![json!("a"), json!("b"), json!("c")],
            "string.fromRegExp" => vec![json!("[a-f0-9]{8}")],
            _ => vec![],
        };
        prop_assert_eq!(draw(name, &args, seed), draw(name, &args, seed));
    }

    #[test]
    fn supported_patterns_make_text_the_regex_crate_matches(
        pieces in prop::collection::vec(prop::sample::select(&SYNTAX[..]), 0..10),
        seed in 0_u64..u64::MAX,
    ) {
        let pattern = pieces.concat();
        let checked = FakeRegistry::builtin().check("string.fromRegExp", &[json!(pattern)]);
        let full = Regex::new(&format!("^(?:{pattern})$"));
        if let (Some(Ok(_)), Ok(full)) = (checked, full) {
            let text = draw("string.fromRegExp", &[json!(pattern)], seed);
            prop_assert!(full.is_match(text.as_str().unwrap_or_default()), "{} made {}", pattern, text);
        }
    }
}
