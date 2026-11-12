//! The explain pages, `crates/diagnostics/explain/<code>.md`, built into the binary so
//! `vc explain V2003` works anywhere (D88, D180).

use super::Code;

/// Declares [`PAGES`]: each listed code with its page.
macro_rules! pages {
    ($($code:ident),* $(,)?) => {
        /// Every code that has an explain page, with the page's Markdown.
        const PAGES: &[(Code, &str)] =
            &[$((Code::$code, include_str!(concat!("../../explain/", stringify!($code), ".md"))),)*];
    };
}

pages! {
    V1001, V1002, V1003, V1004, V1005, V1006, V1007, V1008, V1009, V1010, V1011, V1012, V1013,
    V2001, V2002, V2003, V2004, V2005, V2006, V2007, V2008, V2101, V2102, V2103, V2105, V2106, V2107,
    V2201, V2205, V2210, V2211, V2301, V2302, V2303, V2304, V2305, V2306, V2308, V2309, V2310, V2311, V2312,
    V3101, V3102, V3103, V3104, V3201, V3202, V3203, V3204, V3205, V3206, V3207, V3208,
    V4001, V4002, V4003, V4004, V4005, V4006, V4007, V4008,
    V5101, V5102, V5103, V5104, V5105, V5106, V5107, V5108, V5109, V5110, V5111, V5112, V5113, V5114,
    V5115, V5116, V5117, V5118, V5119, V5120,
}

/// How far a page's example lines are indented in the plain-text rendering.
const INDENT: &str = "    ";

impl Code {
    /// The code's explain page as Markdown: a `# <code>: <title>` line, a paragraph, and a
    /// `## Wrong` and a `## Right` example. `None` for a code whose page is still to come.
    #[must_use]
    pub fn explanation(self) -> Option<&'static str> {
        PAGES.iter().find(|(code, _)| *code == self).map(|(_, page)| *page)
    }

    /// The explain page as plain text for a terminal: headings without their `#`s, examples
    /// indented without their fences. A code with no page gets its title line alone.
    #[must_use]
    pub fn explain(self) -> String {
        let Some(page) = self.explanation() else {
            return format!("{self}: {}\n", self.title());
        };
        let mut text = String::new();
        let mut fenced = false;
        for line in page.lines() {
            if line.starts_with("```") {
                fenced = !fenced;
            } else if fenced {
                text.push_str(INDENT);
                text.push_str(line);
                text.push('\n');
            } else {
                text.push_str(line.trim_start_matches('#').trim_start());
                text.push('\n');
            }
        }
        text
    }
}
