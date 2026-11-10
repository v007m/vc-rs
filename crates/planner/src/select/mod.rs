//! Selection: positional selectors joined with "or", then the flags filtering with "and". Tests
//! are selected for a plan; apis and definition files for `vc list`.

mod kinds;
mod spec;
mod suggest;
mod tags;

pub use suggest::closest;

use std::collections::BTreeMap;

use vc_model::{ApiId, FileId, Program, SourceFileId, TestId};

use self::kinds::Positional;
use self::spec::{AllOf, BoxSpec, Candidate, Changed, Grep, Item, Spec};
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
    let positionals = positionals(selection)?;
    let filter = filters(selection)?;
    let candidates: Vec<_> = program.tests.values().map(|test| Candidate::new(program, test)).collect();
    let known = Known { files: names(program, program.test_files.keys()), items: "test or test file" };
    known.check(&positionals, &candidates)?;
    let picked = candidates.iter().filter(|candidate| filter.matches(candidate));
    let test = |candidate: &Candidate<'_>| if let Item::Test(test) = candidate.item { Some(test.id) } else { None };
    Ok(picked.filter_map(|candidate| Some((test(candidate)?, pick(&positionals, candidate)?))).collect())
}

/// The apis `selection` picks, in project order. Apis have no tags, so a tag never holds.
///
/// # Errors
///
/// A [`PlanError`] when a selector cannot be read or matches no api and names no api file.
pub fn select_apis(program: &Program, selection: &Selection) -> Result<Vec<ApiId>, PlanError> {
    let candidates: Vec<_> = program.apis.values().map(|api| Candidate::api(program, api)).collect();
    let known = Known { files: names(program, program.api_groups.keys()), items: "api or api file" };
    let api = |candidate: &&Candidate<'_>| if let Item::Api(api) = candidate.item { Some(api.id) } else { None };
    Ok(chosen(selection, &candidates, &known)?.iter().filter_map(api).collect())
}

/// The definition files `selection` picks, in project order; a file's tags are a test file's.
///
/// # Errors
///
/// A [`PlanError`] when a selector cannot be read or matches no definition file.
pub fn select_files(program: &Program, selection: &Selection) -> Result<Vec<SourceFileId>, PlanError> {
    let all = (0..).map(SourceFileId::new).map_while(|id| program.files.get(id).map(|file| (id, file)));
    let ids: Vec<(SourceFileId, String)> = all.filter_map(|(id, file)| Some((id, file.path.file_id()?))).collect();
    let candidates: Vec<_> = ids.iter().map(|(id, file_id)| Candidate::file(program, *id, file_id)).collect();
    let known = Known { files: Vec::new(), items: "definition file" };
    let file = |candidate: &&Candidate<'_>| if let Item::File(id) = candidate.item { Some(id) } else { None };
    Ok(chosen(selection, &candidates, &known)?.iter().filter_map(file).collect())
}

/// The candidates that the positional selectors pick and every flag holds for.
fn chosen<'c, 'p>(
    selection: &Selection,
    candidates: &'c [Candidate<'p>],
    known: &Known<'_>,
) -> Result<Vec<&'c Candidate<'p>>, PlanError> {
    let positionals = positionals(selection)?;
    let filter = filters(selection)?;
    known.check(&positionals, candidates)?;
    let picked = |candidate: &&Candidate<'_>| filter.matches(candidate) && pick(&positionals, candidate).is_some();
    Ok(candidates.iter().filter(picked).collect())
}

/// The IDs, then the paths, read.
fn positionals(selection: &Selection) -> Result<Vec<Positional>, PlanError> {
    let ids = selection.ids.iter().map(|text| kinds::read(text));
    let paths = selection.paths.iter().map(|text| Ok(kinds::read_path(text)));
    ids.chain(paths).collect()
}

fn names<'p>(program: &'p Program, files: impl Iterator<Item = &'p FileId>) -> Vec<&'p str> {
    files.filter_map(|file| program.str(file.sym())).collect()
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

/// What a positional selector may name besides a candidate: the files that hold none, and what
/// the candidates are, for the error.
struct Known<'p> {
    files: Vec<&'p str>,
    items: &'static str,
}

impl Known<'_> {
    fn check(&self, positionals: &[Positional], candidates: &[Candidate<'_>]) -> Result<(), PlanError> {
        positionals.iter().try_for_each(|positional| self.one(positional, candidates))
    }

    /// Fails when `positional` matches no candidate and names no file.
    fn one(&self, positional: &Positional, candidates: &[Candidate<'_>]) -> Result<(), PlanError> {
        let text = positional.text.as_str();
        if candidates.iter().any(|candidate| positional.spec.matches(candidate)) || self.files.contains(&text) {
            return Ok(());
        }
        let wanted = positional.ordinal.and_then(|_| text.rsplit_once(':')).map_or(text, |(test, _)| test);
        let files = candidates.iter().map(|candidate| candidate.file_id);
        let names = candidates.iter().map(|candidate| candidate.full_id).chain(files);
        let suggestions = suggest::closest(wanted, names);
        Err(PlanError::UnknownSelector { selector: text.to_owned(), items: self.items, suggestions })
    }
}
