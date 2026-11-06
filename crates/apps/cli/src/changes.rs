//! Showing an authoring change before it is written, and asking for it (D88).

use std::io::{BufRead, Write};

use vc_engine::Changes;

/// Each changed file as a unified diff, on stdout.
pub fn preview(changes: &Changes) -> anyhow::Result<()> {
    let mut out = std::io::stdout().lock();
    for file in &changes.files {
        write!(out, "{}", file.diff())?;
    }
    out.flush()?;
    Ok(())
}

/// Asks `question` on stderr and reads the answer from stdin: yes only for `y` or `yes`, in
/// any case; no answer at all is no.
pub fn confirm(question: &str) -> anyhow::Result<bool> {
    eprint!("{question} [y/N] ");
    std::io::stderr().flush()?;
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer)?;
    Ok(is_yes(&answer))
}

/// Whether `answer` says yes.
fn is_yes(answer: &str) -> bool {
    let answer = answer.trim();
    answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes")
}

#[cfg(test)]
mod tests {
    use super::is_yes;

    #[test]
    fn only_y_or_yes_is_yes() {
        for yes in ["y\n", "Y", " yes \n", "YES"] {
            assert!(is_yes(yes), "{yes:?}");
        }
        for no in ["", "\n", "n", "no", "yep", "y es"] {
            assert!(!is_yes(no), "{no:?}");
        }
    }
}
