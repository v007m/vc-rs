//! Text matching a pattern, for `fake.string.fromRegExp` and `fromRegExp` generators (D174).
//!
//! A bounded subset of regex syntax: literals, escapes, `.`, classes and ranges, groups and
//! alternation, and quantifiers. Anything else (lookarounds, backreferences, word boundaries,
//! Unicode classes) is refused with a reason, reported as V2309. An open repeat (`*`, `+`,
//! `{n,}`) adds at most [`OPEN_REPEAT`] copies, so every pattern makes text of bounded length.

use crate::pipe_fns::Chance;

/// The most copies an open repeat adds past its minimum.
pub const OPEN_REPEAT: u32 = 16;
/// The largest count a `{n,m}` may give.
const MAX_COUNT: u32 = 256;
/// The longest text a pattern may make.
const MAX_LENGTH: u64 = 10_000;
/// The deepest groups may nest.
const MAX_DEPTH: usize = 32;
/// What `.` and a negated class choose from: printable ASCII.
const PRINTABLE: (char, char) = (' ', '~');
const DIGITS: &[(char, char)] = &[('0', '9')];
const WORD: &[(char, char)] = &[('0', '9'), ('A', 'Z'), ('_', '_'), ('a', 'z')];
const SPACE: &[(char, char)] = &[(' ', ' ')];

/// Inclusive character ranges.
type Ranges = Vec<(char, char)>;

/// A pattern checked and ready to generate from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pattern {
    branches: Vec<Vec<Item>>,
}

/// One piece of a sequence and how often it repeats.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Item {
    piece: Piece,
    min: u32,
    max: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Piece {
    /// One character.
    Char(char),
    /// One character from these inclusive ranges.
    Set(Ranges),
    /// One of the branches.
    Group(Vec<Vec<Item>>),
}

impl Pattern {
    /// Checks `text`.
    ///
    /// # Errors
    ///
    /// What in the pattern is not supported or not valid, for a V2309 diagnostic.
    pub fn parse(text: &str) -> Result<Self, String> {
        let chars: Vec<char> = text.chars().collect();
        let mut parser = Parser { chars: &chars, at: 0 };
        parser.anchor('^');
        let branches = parser.branches(0)?;
        if parser.peek().is_some() {
            return Err("a `)` closes no group".to_owned());
        }
        let pattern = Self { branches };
        if longest(&pattern.branches) > MAX_LENGTH {
            return Err(format!("the pattern can make text longer than {MAX_LENGTH} characters"));
        }
        Ok(pattern)
    }

    /// Text the pattern matches, drawn from `chance`.
    #[must_use]
    pub fn generate(&self, chance: &dyn Chance) -> String {
        let mut out = String::new();
        branches(&self.branches, chance, &mut out);
        out
    }
}

/// The longest text one of `branches` can make.
fn longest(branches: &[Vec<Item>]) -> u64 {
    let sequence = |items: &Vec<Item>| {
        items.iter().fold(0_u64, |total, item| {
            let piece = match &item.piece {
                Piece::Group(inner) => longest(inner),
                Piece::Char(_) | Piece::Set(_) => 1,
            };
            total.saturating_add(piece.saturating_mul(u64::from(item.max)))
        })
    };
    branches.iter().map(sequence).max().unwrap_or(0)
}

fn branches(branches: &[Vec<Item>], chance: &dyn Chance, out: &mut String) {
    let Some(items) = branches.get(chance.below(branches.len())) else { return };
    for item in items {
        let spread = usize::try_from(item.max - item.min).unwrap_or(0);
        let count = item.min + u32::try_from(chance.below(spread + 1)).unwrap_or(0);
        for _ in 0..count {
            match &item.piece {
                Piece::Char(char) => out.push(*char),
                Piece::Set(ranges) => out.push(pick(ranges, chance)),
                Piece::Group(inner) => self::branches(inner, chance, out),
            }
        }
    }
}

/// One character of `ranges`, every character equally likely.
fn pick(ranges: &[(char, char)], chance: &dyn Chance) -> char {
    let size = |(low, high): &(char, char)| usize::try_from(u32::from(*high) - u32::from(*low) + 1).unwrap_or(0);
    let mut left = chance.below(ranges.iter().map(size).sum());
    for range in ranges {
        if left < size(range) {
            let code = u32::from(range.0) + u32::try_from(left).unwrap_or(0);
            return char::from_u32(code).unwrap_or(range.0);
        }
        left -= size(range);
    }
    ranges.first().map_or(' ', |range| range.0)
}

/// The characters of printable ASCII not in `ranges`.
fn complement(ranges: &[(char, char)]) -> Ranges {
    let inside = |char: char| ranges.iter().any(|(low, high)| (*low..=*high).contains(&char));
    let mut out: Ranges = Vec::new();
    for char in (PRINTABLE.0..=PRINTABLE.1).filter(|char| !inside(*char)) {
        match out.last_mut() {
            Some(last) if u32::from(last.1) + 1 == u32::from(char) => last.1 = char,
            _ => out.push((char, char)),
        }
    }
    out
}

/// Reads a pattern left to right.
struct Parser<'c> {
    chars: &'c [char],
    at: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn next(&mut self) -> Option<char> {
        let char = self.peek()?;
        self.at += 1;
        Some(char)
    }

    fn eat(&mut self, char: char) -> bool {
        let found = self.peek() == Some(char);
        self.at += usize::from(found);
        found
    }

    /// Skips `^` at the start or `$` at the end: generated text always starts and ends there.
    fn anchor(&mut self, char: char) -> bool {
        let at_edge = char == '^' || self.chars.get(self.at + 1).is_none_or(|next| *next == ')' || *next == '|');
        at_edge && self.eat(char)
    }

    fn branches(&mut self, depth: usize) -> Result<Vec<Vec<Item>>, String> {
        let mut branches = vec![self.sequence(depth)?];
        while self.eat('|') {
            branches.push(self.sequence(depth)?);
        }
        Ok(branches)
    }

    fn sequence(&mut self, depth: usize) -> Result<Vec<Item>, String> {
        let mut items = Vec::new();
        while let Some(char) = self.peek().filter(|char| *char != '|' && *char != ')') {
            if self.anchor('$') {
                continue;
            }
            if char == '^' || char == '$' {
                return Err(format!("`{char}` is supported only at the start or end of the pattern"));
            }
            let piece = self.piece(depth)?;
            let (min, max) = self.quantifier()?;
            items.push(Item { piece, min, max });
        }
        Ok(items)
    }

    fn piece(&mut self, depth: usize) -> Result<Piece, String> {
        match self.next() {
            Some('(') => self.group(depth + 1),
            Some('[') => self.class(),
            Some('.') => Ok(Piece::Set(vec![PRINTABLE])),
            Some('\\') => self.escape().map(|(piece, _)| piece),
            Some(char @ ('*' | '+' | '?' | '{')) => Err(format!("`{char}` has nothing to repeat")),
            Some(char) => Ok(Piece::Char(char)),
            None => Err("the pattern ends early".to_owned()),
        }
    }

    fn group(&mut self, depth: usize) -> Result<Piece, String> {
        if depth > MAX_DEPTH {
            return Err(format!("groups nest deeper than {MAX_DEPTH}"));
        }
        if self.eat('?') && !self.eat(':') {
            return Err("lookarounds and named groups `(?…)` are not supported; use `(…)` or `(?:…)`".to_owned());
        }
        let branches = self.branches(depth)?;
        if !self.eat(')') {
            return Err("a `(` is not closed".to_owned());
        }
        Ok(Piece::Group(branches))
    }

    /// `[a-z0-9_]`, `[^,]`.
    fn class(&mut self) -> Result<Piece, String> {
        let negated = self.eat('^');
        let mut ranges = Vec::new();
        let mut first = true;
        loop {
            let char = self.next().ok_or_else(|| "a `[` is not closed".to_owned())?;
            if char == ']' && !first {
                break;
            }
            first = false;
            ranges.extend(self.class_member(char)?);
        }
        let ranges = if negated { complement(&ranges) } else { ranges };
        if ranges.is_empty() {
            return Err("the class matches no printable character".to_owned());
        }
        Ok(Piece::Set(ranges))
    }

    /// One member of a class: a character, a range `a-z` or an escape such as `\d`.
    fn class_member(&mut self, char: char) -> Result<Ranges, String> {
        let low = if char == '\\' {
            match self.escape()? {
                (Piece::Char(low), _) => low,
                (Piece::Set(ranges), _) => return Ok(ranges),
                (Piece::Group(_), _) => return Err("a class cannot hold a group".to_owned()),
            }
        } else {
            char
        };
        let ranged = self.peek() == Some('-') && self.chars.get(self.at + 1).is_some_and(|next| *next != ']');
        if !ranged {
            return Ok(vec![(low, low)]);
        }
        self.at += 1;
        let high = match self.next() {
            Some('\\') => self.escape()?.1.ok_or_else(|| "a range cannot end in a class such as `\\d`".to_owned())?,
            other => other.unwrap_or(low),
        };
        if high < low {
            return Err(format!("the range `{low}-{high}` is out of order"));
        }
        Ok(vec![(low, high)])
    }

    /// After `\`: the piece, and the character when it is one.
    fn escape(&mut self) -> Result<(Piece, Option<char>), String> {
        let char = self.next().ok_or_else(|| "the pattern ends with `\\`".to_owned())?;
        let set = |ranges: &[(char, char)]| Ok((Piece::Set(ranges.to_vec()), None));
        match char {
            'd' => set(DIGITS),
            'w' => set(WORD),
            's' => set(SPACE),
            'D' => set(&complement(DIGITS)),
            'W' => set(&complement(WORD)),
            'S' => set(&complement(SPACE)),
            'n' => Ok((Piece::Char('\n'), Some('\n'))),
            't' => Ok((Piece::Char('\t'), Some('\t'))),
            char if char.is_ascii_punctuation() || char == ' ' => Ok((Piece::Char(char), Some(char))),
            char => Err(format!("`\\{char}` is not supported; escape only punctuation, or use `\\d`, `\\w` or `\\s`")),
        }
    }

    /// The repeat after a piece: `(1, 1)` when there is none.
    fn quantifier(&mut self) -> Result<(u32, u32), String> {
        let bounds = match self.peek() {
            Some('*') => (0, OPEN_REPEAT),
            Some('+') => (1, OPEN_REPEAT),
            Some('?') => (0, 1),
            Some('{') => return self.counted(),
            _ => return Ok((1, 1)),
        };
        self.at += 1;
        self.lazy()?;
        Ok(bounds)
    }

    /// `{n}`, `{n,}` or `{n,m}`.
    fn counted(&mut self) -> Result<(u32, u32), String> {
        self.at += 1;
        let min = self.count()?.ok_or_else(|| "a `{` must start a repeat such as `{2,5}`".to_owned())?;
        let max = if self.eat(',') { self.count()?.unwrap_or(min + OPEN_REPEAT) } else { min };
        if !self.eat('}') {
            return Err("a repeat such as `{2,5}` is not closed".to_owned());
        }
        if max < min {
            return Err(format!("the repeat `{{{min},{max}}}` is out of order"));
        }
        self.lazy()?;
        Ok((min, max))
    }

    fn count(&mut self) -> Result<Option<u32>, String> {
        let start = self.at;
        while self.peek().is_some_and(|char| char.is_ascii_digit()) {
            self.at += 1;
        }
        let digits: String = self.chars.get(start..self.at).unwrap_or_default().iter().collect();
        if digits.is_empty() {
            return Ok(None);
        }
        let count = digits.parse::<u32>().ok().filter(|count| *count <= MAX_COUNT);
        count.map(Some).ok_or_else(|| format!("repeat counts go up to {MAX_COUNT}"))
    }

    /// A lazy `?` changes nothing for generation; a possessive `+` is not supported.
    fn lazy(&mut self) -> Result<(), String> {
        self.eat('?');
        if matches!(self.peek(), Some('*' | '+' | '{')) {
            return Err("a repeat cannot follow a repeat".to_owned());
        }
        Ok(())
    }
}
