use vc_interfaces::{Environment, Random};

/// Builds a source from a seed.
pub type MakeRandom<'a> = dyn FnMut(u64) -> Box<dyn Random> + 'a;

/// The contract every [`Random`] meets (D112). `make` builds a source from a seed.
///
/// # Panics
///
/// When the source breaks the contract.
pub fn random_suite(make: &mut MakeRandom<'_>) {
    let draws = |random: &dyn Random| (0..32).map(|_| random.next_u64()).collect::<Vec<_>>();
    let first = draws(make(7).as_ref());
    assert_eq!(first, draws(make(7).as_ref()), "the same seed gives the same draws");
    assert_ne!(first, draws(make(8).as_ref()), "another seed gives other draws");
    let mut distinct = first.clone();
    distinct.sort_unstable();
    distinct.dedup();
    assert!(distinct.len() > 16, "draws vary: {first:?}");
}

/// Builds an environment holding exactly these vars.
pub type MakeEnvironment<'a> = dyn FnMut(&[(&str, &str)]) -> Box<dyn Environment> + 'a;

/// The contract every [`Environment`] meets (D112). `make` builds one holding exactly the vars
/// given.
///
/// # Panics
///
/// When the environment breaks the contract.
pub fn environment_suite(make: &mut MakeEnvironment<'_>) {
    let environment = make(&[("API_HOST", "api.test"), ("EMPTY", ""), ("Mixed_case", "x")]);
    assert_eq!(environment.var("API_HOST").as_deref(), Some("api.test"));
    assert_eq!(environment.var("EMPTY").as_deref(), Some(""), "an empty value is still set");
    assert_eq!(environment.var("Mixed_case").as_deref(), Some("x"));
    assert_eq!(environment.var("api_host"), None, "names are case-sensitive");
    assert_eq!(environment.var("MISSING"), None);
    assert_eq!(environment.names(), ["API_HOST", "EMPTY", "Mixed_case"], "every name, sorted");
    assert_eq!(make(&[]).var("API_HOST"), None);
    assert_eq!(make(&[]).names(), Vec::<String>::new());
}
