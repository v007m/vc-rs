use super::Pairing;

/// Entries that each fit their own error and the next, the last only the first: every entry
/// is paired by one search that moves all the others along, as deep as there are entries.
fn chain(entries: usize) -> Vec<Vec<usize>> {
    let mut fits: Vec<_> = (0..entries - 1).map(|entry| vec![entry, entry + 1]).collect();
    fits.push(vec![0]);
    fits
}

#[test]
fn a_pairing_that_moves_every_entry_along_needs_no_recursion() {
    let entries = 5000;
    assert_eq!(Pairing::new(chain(entries), entries).unpaired(), Vec::<usize>::new());
    let mut stuck = chain(entries);
    stuck[entries - 2] = vec![entries - 2];
    assert_eq!(Pairing::new(stuck, entries).unpaired(), [entries - 1], "no path frees an error");
}

#[test]
fn an_entry_paired_along_a_path_keeps_its_error_from_the_entries_after_it() {
    let fits = vec![vec![0, 1], vec![0], vec![0]];
    assert_eq!(Pairing::new(fits, 3).unpaired(), [2], "the first two share errors 0 and 1, the third has none left");
}

#[test]
fn the_greedy_pass_takes_the_first_free_error_an_entry_fits() {
    let mut pairing = Pairing::new(vec![vec![1, 0], vec![1]], 2);
    assert!(pairing.free(0));
    assert_eq!(pairing.owner, [None, Some(0)]);
    assert!(!pairing.free(1), "error 1 is taken");
}

#[test]
fn a_search_tries_each_fit_once_and_resumes_after_the_last_one_tried() {
    let mut pairing = Pairing::new(vec![vec![0, 1, 2]], 3);
    pairing.search = 1;
    pairing.seen[1] = 1;
    let mut step = super::Step { entry: 0, next: 0, taken: 0 };
    assert_eq!((pairing.untried(&mut step), step.next), (Some(0), 1));
    assert_eq!((pairing.untried(&mut step), step.next), (Some(2), 3), "error 1 was tried already");
    assert_eq!(pairing.untried(&mut step), None);
}

#[test]
fn pairs_name_the_error_each_entry_took_after_a_path_moved_them() {
    assert_eq!(Pairing::new(vec![vec![0, 1], vec![0]], 2).pairs(), [Some(1), Some(0)]);
    assert_eq!(Pairing::new(vec![vec![0], vec![0], vec![]], 1).pairs(), [Some(0), None, None]);
    assert_eq!(Pairing::new(Vec::new(), 3).pairs(), Vec::<Option<usize>>::new());
}

#[test]
fn a_pushed_error_moves_an_earlier_pair_to_make_room() {
    let mut pairing = Pairing::new(vec![Vec::new(), Vec::new()], 0);
    pairing.push(&[0, 1]);
    assert_eq!((pairing.matched(), pairing.is_full()), (&[Some(0), None][..], false));
    pairing.push(&[0]);
    assert_eq!(pairing.matched(), [Some(1), Some(0)], "one search moves entry 0 to free error 0");
    pairing.push(&[0, 1, 7]);
    let fitting = (pairing.fitting(1), pairing.fitting(7));
    assert_eq!((pairing.matched(), pairing.is_full(), fitting), (&[Some(1), Some(0)][..], true, (Some(2), None)));
}

/// The fits of `entries` entries after each error in turn, each error fitting the entries its
/// mask names.
fn rebuilt(entries: usize, masks: &[Vec<bool>]) -> Vec<Vec<usize>> {
    (0..entries)
        .map(|entry| masks.iter().enumerate().filter_map(|(error, mask)| mask[entry].then_some(error)).collect())
        .collect()
}

proptest::proptest! {
    /// Pushing errors one at a time pairs as many entries as a full rebuild, and every pair is
    /// one of the entry's fits, one error to an entry.
    #[test]
    fn pushing_one_error_at_a_time_pairs_as_many_as_a_full_rebuild(
        masks in proptest::collection::vec(proptest::collection::vec(proptest::bool::ANY, 4), 0..10),
    ) {
        let mut pairing = Pairing::new(vec![Vec::new(); 4], 0);
        for (pushed, mask) in masks.iter().enumerate() {
            let fits: Vec<usize> = (0..4).filter(|&entry| mask[entry]).collect();
            pairing.push(&fits);
            let fits = rebuilt(4, &masks[..=pushed]);
            let full = Pairing::new(fits.clone(), pushed + 1).pairs();
            let matched = pairing.matched();
            let count = |pairs: &[Option<usize>]| pairs.iter().flatten().count();
            proptest::prop_assert_eq!(count(matched), count(&full), "after {} errors", pushed + 1);
            proptest::prop_assert_eq!(pairing.is_full(), count(matched) == 4, "full once every entry is paired");
            let errors: std::collections::BTreeSet<_> = matched.iter().flatten().collect();
            proptest::prop_assert_eq!(errors.len(), count(matched), "one error to an entry");
            for (entry, error) in matched.iter().enumerate() {
                proptest::prop_assert!(error.is_none_or(|error| fits[entry].contains(&error)), "entry {entry} fits its error");
            }
        }
    }
}
