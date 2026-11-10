use vc_model::SourceFileId;
use vc_parser::{Document, FormatRules, ParserError, Syntax};

/// Items of `apis` in the order `id`, `name`, `path`, `body`; other maps as written.
struct Rules {
    strip: bool,
}

impl FormatRules for Rules {
    fn key_order(&self, path: &[&str]) -> Option<Vec<&'static str>> {
        match path {
            [] => Some(vec!["name", "apis"]),
            ["apis", "-"] => Some(vec!["id", "name", "path", "body"]),
            _ => None,
        }
    }

    fn drops(&self, comment: &str) -> bool {
        self.strip && comment.starts_with("# made by")
    }
}

fn format(text: &str, strip: bool) -> Result<String, ParserError> {
    Document::parse(SourceFileId::new(0), Syntax::Yaml, text)?.format(&Rules { strip })
}

#[test]
fn moves_comments_with_their_items_and_keeps_line_comments() {
    let text =
        "# top\napis:\n  # first\n  - path: /a  # here\n    id: a\n\n  # made by vc\n  - {id: b, path: /b}\n# end\n";
    let tidy =
        "# top\napis:\n  # first\n  - id: a\n    path: /a  # here\n\n  # made by vc\n  - id: b\n    path: /b\n# end\n";
    assert_eq!(format(text, false), Ok(tidy.to_owned()));
    assert_eq!(format(tidy, false), Ok(tidy.to_owned()), "formatting twice changes nothing");
    let stripped = format(text, true).unwrap_or_default();
    assert!(!stripped.contains("made by") && stripped.contains("# first\n"), "{stripped}");
}

#[test]
fn keeps_data_maps_in_order_and_values_as_written() {
    let text = "apis:\n  - body: {z: 0x1F, a: ~, t: 'true', u: 'yes'}\n    id: a\n";
    let tidy = "apis:\n  - id: a\n    body:\n      z: 0x1F\n      a: ~\n      t: \"true\"\n      u: yes\n";
    assert_eq!(format(text, false), Ok(tidy.to_owned()));
}

#[test]
fn moves_a_block_scalar_with_its_key() {
    let text = "apis:\n- body: |\n    line one\n      two\n  id: a\n";
    let tidy = "apis:\n  - id: a\n    body: |\n      line one\n        two\n";
    assert_eq!(format(text, false), Ok(tidy.to_owned()));
}

#[test]
fn writes_lists_of_scalars_and_empty_collections_in_block_style() {
    let text = "name: x\ntags: [a, 'b c']\nnone: []\nmaps: {}\n";
    let tidy = "name: x\ntags:\n  - a\n  - b c\nnone: []\nmaps: {}\n";
    assert_eq!(format(text, false), Ok(tidy.to_owned()));
}

#[test]
fn refuses_to_lose_a_comment_inside_a_flow_collection() {
    let text = "name: x\ntags: [a,\n  # why\n  b]\n";
    assert!(matches!(format(text, false), Err(ParserError::Unformattable(_))));
}
