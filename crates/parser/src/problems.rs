//! The parser's diagnostics, one constructor each.

use vc_diagnostics::{Code, Diagnostic};
use vc_model::SourceSpan;

/// What a scanner message means for Vibranium.
#[derive(Clone, Copy)]
enum ScanKind {
    /// The file's shape is wrong (V1002).
    Structure,
    /// An alias names an anchor that does not exist; aliases are refused anyway (V1002).
    Alias,
}

/// saphyr's errors carry only an English message, so the codes hang on these fragments of it.
/// Every entry has a test in `tests/parse.rs`, so a saphyr upgrade that rewords one fails a test
/// instead of silently changing a stable code. Anything not listed is V1001.
const SCAN_KINDS: &[(&str, ScanKind)] = &[
    ("mapping values are not allowed", ScanKind::Structure),
    ("did not find expected key", ScanKind::Structure),
    ("did not find expected '-' indicator", ScanKind::Structure),
    ("did not find expected <document start>", ScanKind::Structure),
    ("tabs disallowed", ScanKind::Structure),
    ("simple key expect", ScanKind::Structure),
    ("found unknown anchor", ScanKind::Alias),
];

const SYNTAX_HELP: &str =
    "quote values that contain `: ` or ` #` or start with a special character such as `*`, `&` or `!`";
const INDENT_HELP: &str = "check the indentation: a nested key needs more spaces than its parent, and siblings line up";

/// The scanner stopped: V1002 when the message is about structure or an alias, V1001 otherwise.
pub fn scan_error(at: SourceSpan, info: &str) -> Diagnostic {
    match SCAN_KINDS.iter().find(|(needle, _)| info.contains(needle)).map(|&(_, kind)| kind) {
        Some(ScanKind::Structure) => {
            Diagnostic::new(Code::V1002, at, info).label("the structure breaks here").help(INDENT_HELP)
        }
        Some(ScanKind::Alias) => alias(at),
        None => {
            Diagnostic::new(Code::V1001, at, format!("not valid YAML or JSON: {info}")).label("here").help(SYNTAX_HELP)
        }
    }
}

/// A key repeated in one map; the first one is kept.
pub fn duplicate_key(at: SourceSpan, first: SourceSpan, key: &str) -> Diagnostic {
    Diagnostic::new(Code::V1002, at, format!("duplicate key `{key}`"))
        .label("defined again here")
        .secondary(first, "first defined here")
        .help("a key may appear once per map; remove one of them or merge their values")
}

/// A key written as a list or map.
pub fn complex_key(at: SourceSpan) -> Diagnostic {
    Diagnostic::new(Code::V1002, at, "a map key must be plain text, not a list or a map")
        .label("not text")
        .help("use a text key such as `name:`")
}

/// An alias (`*name`): rejected, so values are never shared or expanded behind the reader.
pub fn alias(at: SourceSpan) -> Diagnostic {
    Diagnostic::new(Code::V1002, at, "YAML aliases (`*name`) are not supported")
        .label("alias")
        .help("write the value out, put shared values in `vars`, or reuse a payload with `$extend`")
}

/// A tag (`!name`, `!!str`) on a value.
pub fn tag(at: SourceSpan) -> Diagnostic {
    Diagnostic::new(Code::V1002, at, "YAML tags (`!name`, `!!str`) are not supported")
        .label("tagged value")
        .help("remove the tag; to keep a value as text, quote it")
}

/// A `---` starting a second document.
pub fn second_document(at: SourceSpan) -> Diagnostic {
    Diagnostic::new(Code::V1002, at, "a file holds one YAML document; this starts a second")
        .label("second document")
        .help("move the second document into its own file")
}

/// Nesting beyond the limit.
pub fn too_deep(at: SourceSpan, limit: usize) -> Diagnostic {
    Diagnostic::new(Code::V1002, at, format!("nested more than {limit} levels deep"))
        .label("too deep")
        .help("flatten the structure, or load deep data from a file with `$file`")
}
