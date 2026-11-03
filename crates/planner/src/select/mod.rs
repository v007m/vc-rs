//! Selection: positional selectors joined with "or", then the flags filtering with "and".

mod kinds;
mod spec;
mod suggest;
mod tags;

use std::collections::BTreeMap;

use vc_model::{Program, TestId};

use self::kinds::Positional;
use self::spec::{AllOf, BoxSpec, Candidate, Changed, Grep, Spec};
use crate::{PlanError, Selection};

/// Which instances of a selected test to plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pick {
    All,
    /// 1-based instance numbers, each with the selector that asked for it.
    Ordinals(BTreeMap<u32, String>),
}

/// The selected tests and which of their instances to plan.
pub fn select(program: &Program, selection: &Selection) -> Result<BTreeMap<TestId, Pick>, PlanError> {
    let ids = selection.ids.iter().map(|text| kinds::read(text));
    let paths = selection.paths.iter().map(|text| Ok(kinds::read_path(text)));
    let positionals = ids.chain(paths).collect::<Result<Vec<_>, _>>()?;
    let filter = filters(selection)?;
    let candidates: Vec<_> = program.tests.values().map(|test| Candidate::new(program, test)).collect();
    for positional in &positionals {
        known(program, positional, &candidates)?;
    }
    let picked = candidates.iter().filter(|candidate| filter.matches(candidate));
    Ok(picked.filter_map(|candidate| pick(&positionals, candidate).map(|pick| (candidate.test.id, pick))).collect())
}

/// The flags, all of which must hold.
fn filters(selection: &Selection) -> Result<AllOf, PlanError> {
    let mut specs = selection.tags.iter().map(|expr| tags::parse(expr)).collect::<Result<Vec<BoxSpec>, _>>()?;
    if let Some(text) = &selection.grep {
        specs.push(Box::new(Grep::new(text)));
    }
    if let Some(changed) = &selection.changed {
        specs.push(Box::new(Changed(changed.clone())));
    }
    Ok(AllOf(specs))
}

/// Which instances of `candidate` the positional selectors pick; all when there are none.
fn pick(positionals: &[Positional], candidate: &Candidate<'_>) -> Option<Pick> {
    if positionals.is_empty() {
        return Some(Pick::All);
    }
    let mut ordinals = BTreeMap::new();
    for positional in positionals.iter().filter(|positional| positional.spec.matches(candidate)) {
        let Some(ordinal) = positional.ordinal else { return Some(Pick::All) };
        ordinals.entry(ordinal).or_insert_with(|| positional.text.clone());
    }
    (!ordinals.is_empty()).then_some(Pick::Ordinals(ordinals))
}

/// Fails when a positional selector matches no test and names no test file.
fn known(program: &Program, positional: &Positional, candidates: &[Candidate<'_>]) -> Result<(), PlanError> {
    let text = positional.text.as_str();
    let is_file = || program.test_files.keys().any(|file| program.str(file.sym()) == Some(text));
    if candidates.iter().any(|candidate| positional.spec.matches(candidate)) || is_file() {
        return Ok(());
    }
    let wanted = positional.ordinal.and_then(|_| text.rsplit_once(':')).map_or(text, |(test, _)| test);
    let files = candidates.iter().map(|candidate| candidate.file_id);
    let names = candidates.iter().map(|candidate| candidate.full_id).chain(files);
    Err(PlanError::UnknownSelector { selector: text.to_owned(), suggestions: suggest::closest(wanted, names) })
}
