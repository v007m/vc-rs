//! A one-to-one pairing of entries with the values they fit, as many as can be: `expect.errors`
//! lists pair each entry with its own error this way (D362), and a stream's `receive` group
//! each entry with its own message (D378).

/// A one-to-one pairing of entries with errors (Kuhn's augmenting paths, searched with an
/// explicit stack): each entry's fitting errors, the entry each error is paired with, the
/// search each error was last tried in, and how many steps one search may take.
///
/// A search tries each error at most once and pops at most once more than it pushes, so it
/// takes at most twice as many steps as there are fits, plus one; its path never holds more
/// entries than there are. Both bounds are enforced, so a search always ends in bounded memory.
pub struct Pairing {
    fits: Vec<Vec<usize>>,
    owner: Vec<Option<usize>>,
    seen: Vec<usize>,
    search: usize,
    budget: usize,
}

/// One entry on a search's path: the next of its fits to try, and the error it would take.
struct Step {
    entry: usize,
    next: usize,
    taken: usize,
}

impl Pairing {
    pub fn new(fits: Vec<Vec<usize>>, errors: usize) -> Self {
        let budget = fits.iter().map(Vec::len).sum::<usize>().saturating_mul(2).saturating_add(1);
        Self { fits, owner: vec![None; errors], seen: vec![0; errors], search: 0, budget }
    }

    /// The entries left unpaired, in order.
    pub fn unpaired(self) -> Vec<usize> {
        let pairs = self.pairs().into_iter().enumerate();
        pairs.filter_map(|(entry, error)| error.is_none().then_some(entry)).collect()
    }

    /// The error each entry is paired with, if any, after each takes a free error it fits where
    /// it can and the rest search for a path that frees one.
    pub fn pairs(mut self) -> Vec<Option<usize>> {
        let left: Vec<_> = (0..self.fits.len()).filter(|&entry| !self.free(entry)).collect();
        for entry in left {
            self.paired(entry);
        }
        let mut pairs = vec![None; self.fits.len()];
        for (error, owner) in self.owner.iter().enumerate() {
            if let Some(&entry) = owner.as_ref() {
                pairs[entry] = Some(error);
            }
        }
        pairs
    }

    /// Pairs `entry` with the first free error it fits, if there is one.
    fn free(&mut self, entry: usize) -> bool {
        let Some(&error) = self.fits[entry].iter().find(|&&error| self.owner[error].is_none()) else { return false };
        self.owner[error] = Some(entry);
        true
    }

    /// Pairs `entry` with an error, moving entries along a path of paired errors to free one.
    /// A search that outgrows its bounds leaves the entry unpaired.
    fn paired(&mut self, entry: usize) -> bool {
        self.search += 1;
        let mut path = Vec::with_capacity(self.fits.len());
        path.push(Step { entry, next: 0, taken: 0 });
        for _ in 0..self.budget {
            let Some(step) = path.last_mut() else { return false };
            let Some(error) = self.untried(step) else {
                path.pop();
                continue;
            };
            step.taken = error;
            let Some(owner) = self.owner[error] else {
                self.shift(&path);
                return true;
            };
            if path.len() == self.fits.len() {
                return false;
            }
            path.push(Step { entry: owner, next: 0, taken: 0 });
        }
        false
    }

    /// The next error `step`'s entry fits that this search has not tried, marked tried.
    fn untried(&mut self, step: &mut Step) -> Option<usize> {
        let mut rest = self.fits[step.entry][step.next..].iter().copied().enumerate();
        let (skipped, error) = rest.find(|&(_, error)| self.seen[error] != self.search)?;
        step.next += skipped + 1;
        self.seen[error] = self.search;
        Some(error)
    }

    /// Gives each entry on `path` the error it would take.
    fn shift(&mut self, path: &[Step]) {
        for step in path {
            self.owner[step.taken] = Some(step.entry);
        }
    }
}

#[cfg(test)]
#[path = "pairing_tests.rs"]
mod tests;
