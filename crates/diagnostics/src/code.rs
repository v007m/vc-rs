use std::fmt;
use std::str::FromStr;

use serde::{Serialize, Serializer};

use crate::{DiagnosticsError, Severity};

mod table;
#[cfg(test)]
mod tests;

/// The area a diagnostic code belongs to; the first digit of the code (D88).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Category {
    /// V1xxx: the file cannot be read as the kind it claims to be.
    Syntax,
    /// V2xxx: references and types.
    Reference,
    /// V3xxx: lifecycle rules.
    Lifecycle,
    /// V4xxx: naming and layout conventions.
    Convention,
    /// V5xxx: results at run time.
    Runtime,
}

/// A stable diagnostic code such as V2003, with its title and default severity.
///
/// Every code is an associated const (`Code::V2003`), listed in [`Code::ALL`]; a code is never
/// reused for a different problem. Codes order by number and print as `V2003`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Code {
    number: u16,
    severity: Severity,
    title: &'static str,
}

impl Code {
    /// The number after the `V`: 2003 for V2003.
    #[must_use]
    pub const fn number(self) -> u16 {
        self.number
    }

    /// The area the code belongs to, from its first digit (every code is in V1001 to V5999).
    #[must_use]
    pub const fn category(self) -> Category {
        match self.number / 1000 {
            1 => Category::Syntax,
            2 => Category::Reference,
            3 => Category::Lifecycle,
            4 => Category::Convention,
            _ => Category::Runtime,
        }
    }

    /// The severity a diagnostic with this code has unless something overrides it.
    #[must_use]
    pub const fn default_severity(self) -> Severity {
        self.severity
    }

    /// A one-line summary of the problem: "Unknown api or test".
    #[must_use]
    pub const fn title(self) -> &'static str {
        self.title
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "V{}", self.number)
    }
}

impl FromStr for Code {
    type Err = DiagnosticsError;

    /// Looks a code up by its text: `V` and exactly four ASCII digits, `"V2003"`.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let digits = text.strip_prefix('V').filter(|d| d.len() == 4 && d.bytes().all(|b| b.is_ascii_digit()));
        let number = digits.and_then(|d| d.parse::<u16>().ok());
        let found = number.and_then(|n| Self::ALL.iter().copied().find(|code| code.number == n));
        found.ok_or_else(|| DiagnosticsError::UnknownCode(text.to_owned()))
    }
}

impl Serialize for Code {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}
