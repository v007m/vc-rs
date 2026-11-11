//! What every render of a run shares: the var definitions of each level with the active
//! profiles applied once, the pipes, the fake-data generators, and the random and environment
//! ports.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use indexmap::IndexMap;
use serde_json::{Map, Value};
use vc_interfaces::{Environment, Random, Secrets};
use vc_model::{ActionId, ApiId, FileId, ProfileId, Program, TemplateValue, TestId, VarDefs, VarName};
use vc_template::{FakeRegistry, PipeRegistry};

use crate::values::BodyFiles;

/// The var definitions of one level, by name, with the active profiles applied.
pub type Defined = HashMap<String, TemplateValue>;

/// What a run is given from outside the program: the active profiles, the `--set vars.*`
/// values, the random source (seeded by `--seed`), the environment snapshot and the fake-data
/// generators.
#[derive(Clone)]
#[non_exhaustive]
pub struct RunInputs {
    /// The active profiles, in order (later ones win); empty means the profiles marked
    /// `default: true`.
    pub profiles: Vec<ProfileId>,
    /// Config vars set from outside (`--set vars.region=eu`), layered over the config's vars
    /// and the profiles' vars; a file, api or test that defines the var still wins.
    pub vars: Defined,
    /// Draws for fake data and the `random` and `shuffle` pipes.
    pub random: Arc<dyn Random>,
    /// What `${env.X}` reads.
    pub environment: Arc<dyn Environment>,
    /// What `${fake.…}` generates.
    pub fakes: FakeRegistry,
    /// When the run started, in milliseconds since 1970; `date.past` and the other dates are
    /// relative to it, so a seeded run repeats.
    pub started_ms: u64,
    /// What `${secret.X}` reads; every value is masked in all output.
    pub secrets: Secrets,
    /// `${run.id}`: the run's number in the history; 0 when the history is not kept.
    pub run_id: u64,
    /// The bytes of the `$file` request bodies, read before the run (D272).
    pub bodies: BodyFiles,
}

impl RunInputs {
    /// Inputs with the default profiles, the built-in generators and a start at 1970.
    #[must_use]
    pub fn new(random: Arc<dyn Random>, environment: Arc<dyn Environment>) -> Self {
        let (profiles, vars, fakes) = (Vec::new(), Defined::new(), FakeRegistry::builtin());
        let (secrets, bodies) = (Secrets::new(), BodyFiles::default());
        Self { profiles, vars, random, environment, fakes, started_ms: 0, secrets, run_id: 0, bodies }
    }

    /// The same inputs with the run starting at `started_ms`, for the generated dates.
    #[must_use]
    pub fn with_started_at(self, started_ms: u64) -> Self {
        Self { started_ms, ..self }
    }

    /// The same inputs with `profiles` active, in order.
    #[must_use]
    pub fn with_profiles(self, profiles: Vec<ProfileId>) -> Self {
        Self { profiles, ..self }
    }

    /// The same inputs with `vars` layered over the config's vars.
    #[must_use]
    pub fn with_vars(self, vars: Defined) -> Self {
        Self { vars, ..self }
    }

    /// The same inputs as history run `run_id`.
    #[must_use]
    pub fn with_run_id(self, run_id: u64) -> Self {
        Self { run_id, ..self }
    }

    /// The same inputs with `secrets` for `${secret.X}`.
    #[must_use]
    pub fn with_secrets(self, secrets: Secrets) -> Self {
        Self { secrets, ..self }
    }

    /// The same inputs with the bytes of the `$file` bodies.
    #[must_use]
    pub fn with_bodies(self, bodies: BodyFiles) -> Self {
        Self { bodies, ..self }
    }
}

impl fmt::Debug for RunInputs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RunInputs")
            .field("profiles", &self.profiles)
            .field("run_id", &self.run_id)
            .finish_non_exhaustive()
    }
}

/// The run's values, built once when the run starts.
pub struct RunValues {
    active: Vec<ProfileId>,
    profiles: Vec<String>,
    config: Arc<Defined>,
    api_files: HashMap<FileId, Arc<Defined>>,
    apis: HashMap<ApiId, Arc<Defined>>,
    test_files: HashMap<FileId, Arc<Defined>>,
    tests: HashMap<TestId, Arc<Defined>>,
    actions: Vec<Arc<Defined>>,
    datasets: Arc<Value>,
    pipes: PipeRegistry,
    secrets: Arc<Value>,
    inputs: RunInputs,
}

impl fmt::Debug for RunValues {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RunValues").field("profiles", &self.profiles).finish_non_exhaustive()
    }
}

impl RunValues {
    /// Applies the active profiles to every level of `program`, once.
    #[must_use]
    pub fn new(program: &Program, inputs: RunInputs) -> Self {
        let active = active(program, &inputs.profiles);
        let layer = |defs: &VarDefs| Arc::new(layered(program, defs, &active));
        Self {
            profiles: active.iter().filter_map(|profile| program.str(profile.sym()).map(str::to_owned)).collect(),
            config: Arc::new(config(program, &active, &inputs.vars)),
            api_files: program.api_groups.iter().map(|(id, group)| (*id, layer(&group.vars))).collect(),
            apis: program.apis.iter().map(|(id, api)| (*id, layer(&api.vars))).collect(),
            test_files: program.test_files.iter().map(|(id, file)| (*id, layer(&file.vars))).collect(),
            tests: program.tests.iter().map(|(id, test)| (*id, layer(&test.vars))).collect(),
            actions: program.actions.iter().map(|action| layer(&action.vars)).collect(),
            datasets: Arc::new(datasets(program)),
            pipes: PipeRegistry::builtin(),
            secrets: Arc::new(inputs.secrets.iter().map(|(name, secret)| (name, secret.expose())).collect()),
            active,
            inputs,
        }
    }

    /// The active profiles' names, in order, for `${run.profiles}`.
    #[must_use]
    pub fn profiles(&self) -> Value {
        Value::from(self.profiles.clone())
    }

    /// The active profiles' names, in order.
    #[must_use]
    pub fn profile_names(&self) -> &[String] {
        &self.profiles
    }

    /// The config's vars.
    #[must_use]
    pub fn config(&self) -> Arc<Defined> {
        Arc::clone(&self.config)
    }

    /// An api file's vars.
    #[must_use]
    pub fn api_file(&self, id: FileId) -> Option<Arc<Defined>> {
        self.api_files.get(&id).cloned()
    }

    /// An api's own vars.
    #[must_use]
    pub fn api(&self, id: ApiId) -> Option<Arc<Defined>> {
        self.apis.get(&id).cloned()
    }

    /// A test file's vars.
    #[must_use]
    pub fn test_file(&self, id: FileId) -> Option<Arc<Defined>> {
        self.test_files.get(&id).cloned()
    }

    /// A test's vars.
    #[must_use]
    pub fn test(&self, id: TestId) -> Option<Arc<Defined>> {
        self.tests.get(&id).cloned()
    }

    /// An action's vars.
    #[must_use]
    pub fn action(&self, id: ActionId) -> Option<Arc<Defined>> {
        self.actions.get(usize::try_from(id.index()).ok()?).cloned()
    }

    /// `defs` with the active profiles applied, for vars that are not on a level of the
    /// program (an auth definition's and its use site's).
    #[must_use]
    pub fn layer(&self, program: &Program, defs: &VarDefs) -> Defined {
        layered(program, defs, &self.active)
    }

    /// The `secret` root: every secret by name.
    #[must_use]
    pub fn secrets(&self) -> Arc<Value> {
        Arc::clone(&self.secrets)
    }

    /// The `dataset` root: each loaded dataset's rows, by ID (D173).
    #[must_use]
    pub fn datasets(&self) -> Arc<Value> {
        Arc::clone(&self.datasets)
    }

    /// The pipes templates apply.
    #[must_use]
    pub const fn pipes(&self) -> &PipeRegistry {
        &self.pipes
    }

    /// The random source, the environment and the fake data.
    #[must_use]
    pub const fn inputs(&self) -> &RunInputs {
        &self.inputs
    }
}

/// The profiles given, or the default ones when none are.
fn active(program: &Program, given: &[ProfileId]) -> Vec<ProfileId> {
    if !given.is_empty() {
        return given.to_vec();
    }
    program.profiles.values().filter(|profile| profile.default).map(|profile| profile.id).collect()
}

/// `defs`, with each active profile's overrides layered over them in order.
fn layered(program: &Program, defs: &VarDefs, active: &[ProfileId]) -> Defined {
    let mut values = named(program, &defs.values);
    for overrides in active.iter().filter_map(|profile| defs.by_profile.get(profile)) {
        values.extend(named(program, overrides));
    }
    values
}

/// The config's vars, with each active profile's `vars` layered over them in order, then the
/// vars set from outside.
fn config(program: &Program, active: &[ProfileId], set: &Defined) -> Defined {
    let mut values = layered(program, &program.config.vars, active);
    for profile in active.iter().filter_map(|profile| program.profiles.get(profile)) {
        values.extend(named(program, &profile.vars));
    }
    values.extend(set.iter().map(|(name, value)| (name.clone(), value.clone())));
    values
}

/// Each dataset's rows, nested by the dots of its ID: `users.seed` is `{users: {seed: rows}}`.
fn datasets(program: &Program) -> Value {
    let mut root = Map::new();
    for dataset in program.datasets.values() {
        let Some(id) = program.str(dataset.id.sym()) else { continue };
        nest(&mut root, id, Value::from(dataset.rows.clone()));
    }
    Value::Object(root)
}

/// Puts `value` at the dotted `path` of `map`, unless another dataset already holds that place.
fn nest(map: &mut Map<String, Value>, path: &str, value: Value) {
    match path.split_once('.') {
        None => {
            map.entry(path).or_insert(value);
        }
        Some((head, rest)) => {
            if let Value::Object(inner) = map.entry(head).or_insert_with(|| Value::Object(Map::new())) {
                nest(inner, rest, value);
            }
        }
    }
}

fn named(program: &Program, values: &IndexMap<VarName, TemplateValue>) -> Defined {
    let name = |var: &VarName| program.str(var.sym()).map(str::to_owned);
    values.iter().filter_map(|(var, value)| Some((name(var)?, value.clone()))).collect()
}
