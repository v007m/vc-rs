//! The pure core of a stream call's `exchange` (D377, D378): which written item comes next, and
//! which message each `receive` entry paired with. The stream driver does the I/O and judges
//! each message against the waiting group's checks; the cursor only keeps the place.

use crate::pairing::Pairing;

/// One written `exchange` item, as the cursor sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    /// A `send`: the driver sends its rendered value.
    Send,
    /// A `receive` with this many entries: 1 for one check, the list's length for a group.
    Receive(usize),
}

/// What one message did to the exchange.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fed {
    /// It adds nothing to the `receive` that waits, or none waits.
    Skipped,
    /// It fits an entry of item `item`, whose group still waits for more.
    Held {
        /// The written item.
        item: usize,
    },
    /// It completed item `item`.
    Met {
        /// The written item.
        item: usize,
        /// The message each entry paired with, as its index among the messages fed, in written
        /// order.
        pairs: Vec<usize>,
    },
}

/// The `receive` still waiting when the stream ends or the time limit passes (V5130).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unmet {
    /// The written item.
    pub item: usize,
    /// Its entries with no message of their own yet.
    pub entries: Vec<usize>,
    /// The index of the first message fed since the last item was met.
    pub since: usize,
}

/// The place in one call's exchange. Messages are numbered in the order they are fed.
#[derive(Debug)]
pub struct ExchangeCursor {
    items: Vec<Item>,
    at: usize,
    fed: usize,
    since: usize,
    /// The messages kept for the waiting group, each with the entries it fits.
    kept: Vec<(usize, Vec<usize>)>,
}

impl ExchangeCursor {
    /// A cursor before the first of `items`.
    #[must_use]
    pub fn new(items: Vec<Item>) -> Self {
        let mut cursor = Self { items, at: 0, fed: 0, since: 0, kept: Vec::new() };
        cursor.pass_empty_groups();
        cursor
    }

    /// The `send` item to send now, if the exchange is at one; the cursor moves past it.
    pub fn next_send(&mut self) -> Option<usize> {
        let item = self.at;
        (self.items.get(item) == Some(&Item::Send)).then(|| {
            self.advance();
            item
        })
    }

    /// The `receive` item that waits for messages, if the exchange is at one.
    #[must_use]
    pub fn receiving(&self) -> Option<usize> {
        self.group().map(|_| self.at)
    }

    /// Feeds the next message, given the entries of the waiting group it fits.
    ///
    /// A message is kept while one entry it fits holds fewer messages than the group has
    /// entries: such an entry can always take one of those it holds, so later ones add nothing,
    /// and the pairing stays bounded on an endless stream.
    pub fn feed(&mut self, fits: &[usize]) -> Fed {
        let message = self.fed;
        self.fed += 1;
        let Some(size) = self.group() else { return Fed::Skipped };
        let fits: Vec<usize> = fits.iter().copied().filter(|&entry| entry < size).collect();
        if !fits.iter().any(|&entry| self.held_by(entry) < size) {
            return Fed::Skipped;
        }
        self.kept.push((message, fits));
        let item = self.at;
        let Some(pairs) = self.pairs(size).into_iter().collect::<Option<Vec<_>>>() else { return Fed::Held { item } };
        self.advance();
        self.since = self.fed;
        Fed::Met { item, pairs }
    }

    /// The `receive` still waiting, with its open entries, or `None` once none waits.
    #[must_use]
    pub fn unmet(&self) -> Option<Unmet> {
        let size = self.group()?;
        let pairs = self.pairs(size).into_iter().enumerate();
        let entries = pairs.filter_map(|(entry, message)| message.is_none().then_some(entry)).collect();
        Some(Unmet { item: self.at, entries, since: self.since })
    }

    /// Whether every item is done.
    #[must_use]
    pub const fn is_done(&self) -> bool {
        self.at >= self.items.len()
    }

    /// Whether the exchange has no `receive` with an entry, so the call reads until the server
    /// closes.
    #[must_use]
    pub fn reads_to_end(&self) -> bool {
        !self.items.iter().any(|item| matches!(item, Item::Receive(entries) if *entries > 0))
    }

    /// The waiting group's size.
    fn group(&self) -> Option<usize> {
        match self.items.get(self.at) {
            Some(&Item::Receive(size)) => Some(size),
            _ => None,
        }
    }

    /// How many kept messages fit `entry`.
    fn held_by(&self, entry: usize) -> usize {
        self.kept.iter().filter(|(_, fits)| fits.contains(&entry)).count()
    }

    /// The message each of the group's `size` entries is paired with, if any.
    fn pairs(&self, size: usize) -> Vec<Option<usize>> {
        let fits = (0..size).map(|entry| {
            let kept = self.kept.iter().enumerate();
            kept.filter_map(|(slot, (_, fits))| fits.contains(&entry).then_some(slot)).collect()
        });
        let slots = Pairing::new(fits.collect(), self.kept.len()).pairs();
        slots.into_iter().map(|slot| slot.and_then(|slot| self.kept.get(slot)).map(|(message, _)| *message)).collect()
    }

    fn advance(&mut self) {
        self.at += 1;
        self.kept.clear();
        self.pass_empty_groups();
    }

    /// Moves past `receive` groups with no entries: they wait for nothing. The exchange shape
    /// is to refuse an empty list, so this only guards the cursor.
    fn pass_empty_groups(&mut self) {
        while self.items.get(self.at) == Some(&Item::Receive(0)) {
            self.at += 1;
        }
    }
}

#[cfg(test)]
#[path = "exchange_tests.rs"]
mod tests;
