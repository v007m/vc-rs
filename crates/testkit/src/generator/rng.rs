/// Service names, one folder each; a later round adds a letter suffix (`ordersa`).
const SERVICES: &[&str] = &[
    "accounts",
    "orders",
    "carts",
    "catalog",
    "payments",
    "invoices",
    "shipments",
    "reviews",
    "coupons",
    "inventory",
    "members",
    "sessions",
    "messages",
    "tickets",
    "reports",
    "alerts",
    "bookings",
    "courses",
    "devices",
    "venues",
    "uploads",
    "groups",
    "ledgers",
    "media",
    "notes",
    "plans",
    "quotes",
    "refunds",
    "rules",
    "stores",
    "tasks",
    "teams",
    "tokens",
    "vendors",
    "wallets",
    "webhooks",
];

/// Plain words for labels and search terms.
const WORDS: &[&str] = &[
    "amber", "birch", "cedar", "delta", "ember", "fjord", "garnet", "harbor", "indigo", "juniper", "kestrel", "lumen",
];

/// A small deterministic random source (SplitMix64): the same seed always gives the same project.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub const fn new(seed: u64) -> Self {
        Self(seed)
    }

    const fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in `0..n`; 0 when `n` is 0.
    pub fn below(&mut self, n: usize) -> usize {
        let Ok(n64) = u64::try_from(n) else { return 0 };
        if n64 == 0 {
            return 0;
        }
        usize::try_from(self.next_u64() % n64).unwrap_or(0)
    }

    /// A number in `low..=high`.
    pub fn between(&mut self, low: usize, high: usize) -> usize {
        low + self.below(high.saturating_sub(low) + 1)
    }

    /// One of the plain words.
    pub fn word(&mut self) -> &'static str {
        WORDS[self.below(WORDS.len())]
    }

    /// `count` distinct indexes below `n`, in increasing order.
    pub fn subset(&mut self, n: usize, count: usize) -> Vec<usize> {
        let mut all: Vec<usize> = (0..n).collect();
        for i in (1..all.len()).rev() {
            all.swap(i, self.below(i + 1));
        }
        all.truncate(count);
        all.sort_unstable();
        all
    }
}

/// The name of the `index`th service: lowercase letters only, so it is a valid file ID word.
pub fn service_name(index: usize) -> String {
    let base = SERVICES[index % SERVICES.len()];
    format!("{base}{}", letters(index / SERVICES.len()))
}

/// `0` is empty, `1` is `a`, `26` is `z`, `27` is `aa`.
fn letters(mut round: usize) -> String {
    let mut out = Vec::new();
    while round > 0 {
        round -= 1;
        out.push(b'a' + u8::try_from(round % 26).unwrap_or(0));
        round /= 26;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// `orders` as `Orders`.
pub fn title(name: &str) -> String {
    let mut chars = name.chars();
    chars.next().map(|first| first.to_ascii_uppercase().to_string() + chars.as_str()).unwrap_or_default()
}

/// Replaces each `%key%` in `template` with its value.
pub fn fill(template: &str, values: &[(&str, &str)]) -> String {
    values.iter().fold(template.to_owned(), |text, (key, value)| text.replace(&format!("%{key}%"), value))
}

#[cfg(test)]
mod tests {
    use super::{Rng, fill, letters, service_name, title};

    #[test]
    fn the_same_seed_gives_the_same_numbers() {
        let (mut a, mut b) = (Rng::new(7), Rng::new(7));
        let first: Vec<_> = (0..20).map(|_| a.below(1000)).collect();
        assert_eq!(first, (0..20).map(|_| b.below(1000)).collect::<Vec<_>>());
        assert_ne!(first, (0..20).map(|_| Rng::new(8).below(1000)).collect::<Vec<_>>());
    }

    #[test]
    fn numbers_stay_in_range() {
        let mut rng = Rng::new(1);
        assert_eq!(rng.below(0), 0);
        assert!((0..200).map(|_| rng.between(3, 5)).all(|n| (3..=5).contains(&n)));
        let subset = rng.subset(10, 4);
        assert_eq!(subset.len(), 4);
        assert!(subset.windows(2).all(|w| w[0] < w[1]) && subset.iter().all(|&i| i < 10));
    }

    #[test]
    fn names_are_lowercase_words() {
        assert_eq!([letters(0), letters(1), letters(26), letters(27)], ["", "a", "z", "aa"]);
        assert_eq!(service_name(0), "accounts");
        assert_eq!(service_name(36), "accountsa");
        assert!((0..2000).map(service_name).all(|n| n.chars().all(|c| c.is_ascii_lowercase())));
        assert_eq!(title("orders"), "Orders");
        assert_eq!(fill("%a%-%b%-%a%", &[("a", "x"), ("b", "y")]), "x-y-x");
    }
}
