//! The `${fake.…}` generators by Faker name, and the named-generator kinds of the config (D174).

use std::collections::{BTreeSet, HashMap};
use std::fmt;

use serde_json::Value;
use vc_diagnostics::Code;

use crate::ValueType;
use crate::codes;
use crate::fake_fns as run;
use crate::pipe_fns::Chance;

/// What a generator does: given the reference's literal arguments, it draws a value.
pub type FakeFn = fn(&[Value], &FakeCx<'_>) -> Result<Value, String>;

/// Checks a reference's literal arguments before the run.
pub type ArgCheck = fn(&[Value]) -> Result<(), FakeError>;

/// What a generator draws from: the run's seeded random numbers and the time the run started.
#[derive(Clone, Copy)]
#[non_exhaustive]
pub struct FakeCx<'a> {
    /// The run's draws.
    pub chance: &'a dyn Chance,
    /// When the run started, in milliseconds since 1970; dates are relative to it.
    pub started_ms: u64,
}

impl fmt::Debug for FakeCx<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FakeCx").field("started_ms", &self.started_ms).finish_non_exhaustive()
    }
}

impl<'a> FakeCx<'a> {
    /// Draws from `chance`, with dates relative to `started_ms`.
    #[must_use]
    pub const fn new(chance: &'a dyn Chance, started_ms: u64) -> Self {
        Self { chance, started_ms }
    }
}

/// One generator: its argument check, the type it gives, and what it does.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct FakeDef {
    /// Checks the arguments before the run.
    pub check: ArgCheck,
    /// The type of value it gives.
    pub output: ValueType,
    /// What it does.
    pub generate: FakeFn,
}

impl FakeDef {
    /// A generator giving `output`, whose arguments `check` accepts.
    #[must_use]
    pub const fn new(check: ArgCheck, output: ValueType, generate: FakeFn) -> Self {
        Self { check, output, generate }
    }
}

/// A reference or named generator the compiler rejects, with the code it reports.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct FakeError {
    /// V2302 for the wrong arguments, V2309 for a pattern `fromRegExp` cannot make text for.
    pub code: Code,
    /// What is wrong, to follow the generator's name.
    pub message: String,
}

impl FakeError {
    /// A rejection with `code`.
    #[must_use]
    pub fn new(code: Code, message: &str) -> Self {
        Self { code, message: message.to_owned() }
    }
}

/// A named generator lowered to the built-in it calls and its arguments.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct FakeCall {
    /// The built-in generator, such as `number.int`.
    pub name: &'static str,
    /// Its arguments.
    pub args: Vec<Value>,
}

const fn text(check: ArgCheck, generate: FakeFn) -> FakeDef {
    FakeDef::new(check, ValueType::Text, generate)
}

const fn number(check: ArgCheck, generate: FakeFn) -> FakeDef {
    FakeDef::new(check, ValueType::Number, generate)
}

/// The built-in generators, by the syntax reference's Faker names.
const BUILTIN: &[(&str, FakeDef)] = &[
    ("person.fullName", text(run::no_args, run::full_name)),
    ("person.firstName", text(run::no_args, run::first_name)),
    ("person.jobTitle", text(run::no_args, run::job_title)),
    ("internet.email", text(run::no_args, run::email)),
    ("internet.userName", text(run::no_args, run::user_name)),
    ("phone.number", text(run::no_args, run::phone)),
    ("location.streetAddress", text(run::no_args, run::street_address)),
    ("location.city", text(run::no_args, run::city)),
    ("location.zipCode", text(run::no_args, run::zip_code)),
    ("location.country", text(run::no_args, run::country)),
    ("location.countryCode", text(run::no_args, run::country_code)),
    ("string.uuid", text(run::no_args, run::uuid)),
    ("string.alphanumeric", text(run::count_arg, run::alphanumeric)),
    ("string.fromRegExp", text(run::pattern_arg, run::from_reg_exp)),
    ("lorem.word", text(run::no_args, run::word)),
    ("lorem.words", text(run::count_arg, run::words)),
    ("lorem.sentence", text(run::no_args, run::sentence)),
    ("lorem.paragraphs", text(run::count_arg, run::paragraphs)),
    ("number.int", number(run::int_range, run::int)),
    ("number.float", number(run::number_range, run::float)),
    ("commerce.price", number(run::number_range, run::price)),
    ("date.past", text(run::no_args, run::past)),
    ("date.future", text(run::no_args, run::future)),
    ("date.recent", text(run::no_args, run::recent)),
    ("helpers.arrayElement", FakeDef::new(run::choices, ValueType::Any, run::array_element)),
];

/// Turns a named generator's settings into the built-in's arguments.
type Lower = fn(&Value) -> Option<Vec<Value>>;

/// The kinds a named generator in the config can be, and the built-in each one calls.
const KINDS: &[(&str, &str, Lower)] = &[
    ("fromRegExp", "string.fromRegExp", pattern),
    ("oneOf", "helpers.arrayElement", items),
    ("int", "number.int", int_range),
    ("lorem", "lorem.words", lorem),
];

/// `fromRegExp: "[A-Z]{3}"`.
fn pattern(settings: &Value) -> Option<Vec<Value>> {
    settings.is_string().then(|| vec![settings.clone()])
}

/// `oneOf: [PRIVATE, PUBLIC]`.
fn items(settings: &Value) -> Option<Vec<Value>> {
    settings.as_array().cloned()
}

/// `int: {min: 1, max: 5}`; a bound left out is `number.int`'s default.
fn int_range(settings: &Value) -> Option<Vec<Value>> {
    let bound = |key: &str, default: i64| settings.get(key).cloned().unwrap_or_else(|| Value::from(default));
    settings.is_object().then(|| vec![bound("min", 0), bound("max", run::MAX_SAFE_INTEGER)])
}

/// `lorem: {words: 500}`.
fn lorem(settings: &Value) -> Option<Vec<Value>> {
    settings.as_object().map(|lorem| lorem.get("words").cloned().into_iter().collect())
}

/// The fake-data generators by dotted name (`string.uuid`): the built-ins, plus any registered.
#[derive(Clone, Debug, Default)]
pub struct FakeRegistry {
    fakes: HashMap<String, FakeDef>,
}

impl FakeRegistry {
    /// A registry holding the built-in generators.
    #[must_use]
    pub fn builtin() -> Self {
        Self { fakes: BUILTIN.iter().map(|(name, def)| ((*name).to_owned(), *def)).collect() }
    }

    /// The registry with `def` under `name`, replacing any earlier one.
    #[must_use]
    pub fn with(mut self, name: &str, def: FakeDef) -> Self {
        self.fakes.insert(name.to_owned(), def);
        self
    }

    /// The generator `name`, if registered.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&FakeDef> {
        self.fakes.get(name)
    }

    /// Every generator's name, sorted.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.fakes.keys().map(String::as_str).collect();
        names.sort_unstable();
        names
    }

    /// The first words of the generators' names (`person`, `lorem`), which no named generator
    /// may take.
    #[must_use]
    pub fn modules(&self) -> BTreeSet<&str> {
        self.fakes.keys().filter_map(|name| name.split('.').next()).collect()
    }

    /// The type `name` gives with `args`, or why the arguments are wrong; `None` for an
    /// unknown name.
    #[must_use]
    pub fn check(&self, name: &str, args: &[Value]) -> Option<Result<ValueType, FakeError>> {
        let def = self.get(name)?;
        Some((def.check)(args).map(|()| def.output))
    }

    /// A value from the generator `name`.
    ///
    /// # Errors
    ///
    /// The generator's reason when it fails, or a message when no generator has that name.
    pub fn generate(&self, name: &str, args: &[Value], cx: &FakeCx<'_>) -> Result<Value, String> {
        let def = self.get(name).ok_or_else(|| format!("`fake.{name}` is not a generator"))?;
        (def.generate)(args, cx)
    }

    /// The kinds a named generator can be: `fromRegExp`, `oneOf`, `int` and `lorem`.
    #[must_use]
    pub fn kinds() -> Vec<&'static str> {
        KINDS.iter().map(|(kind, _, _)| *kind).collect()
    }

    /// A named generator of `kind` with `settings`, as the built-in call it makes.
    ///
    /// # Errors
    ///
    /// V2302 for an unknown kind or settings of the wrong shape, and the built-in's own
    /// argument check, such as V2309 for a pattern `fromRegExp` cannot make text for.
    pub fn named(&self, kind: &str, settings: &Value) -> Result<FakeCall, FakeError> {
        let found = KINDS.iter().find(|(known, _, _)| *known == kind);
        let Some((_, name, lower)) = found else {
            return Err(FakeError::new(codes::FAKE_ARGS, &format!("`{kind}` is not a generator kind")));
        };
        let args =
            lower(settings).ok_or_else(|| FakeError::new(codes::FAKE_ARGS, "has settings of the wrong shape"))?;
        match self.check(name, &args) {
            Some(Err(error)) => Err(error),
            _ => Ok(FakeCall { name, args }),
        }
    }
}
