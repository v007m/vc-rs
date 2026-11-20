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
