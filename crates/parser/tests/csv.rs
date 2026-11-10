use vc_diagnostics::{Code, Diagnostics};
use vc_model::SourceFileId;
use vc_parser::{Node, ScalarKind, Syntax};

fn csv(text: &str) -> (Option<Node>, Diagnostics) {
    vc_parser::parse(SourceFileId::new(3), Syntax::Csv, text)
}

/// Each row as `column=cell` pairs.
fn rows(root: &Node) -> Vec<Vec<String>> {
    let pairs = |row: &Node| {
        row.entries().iter().map(|e| format!("{}={}", e.key, e.value.as_str().unwrap_or_default())).collect()
    };
    root.items().iter().map(pairs).collect()
}

#[test]
fn reads_each_row_into_a_map_of_text_cells() {
    let (root, diagnostics) = csv("city,zip\nBerlin,10115\n\"Hamburg, Altona\",\"22\"\"765\"\r\n");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let root = root.unwrap();
    assert_eq!(rows(&root), [["city=Berlin", "zip=10115"], ["city=Hamburg, Altona", "zip=22\"765"]]);
    let cell = root.items()[0].get("zip").unwrap();
    assert_eq!(cell.as_scalar().unwrap().kind, ScalarKind::String);
    assert_eq!(root.span.file, SourceFileId::new(3));
}

#[test]
fn every_cell_spans_its_row_and_every_key_the_header() {
    let (root, _) = csv("name,town\nAda,Köln\nBo,Ulm");
    let root = root.unwrap();
    let place = |p: vc_model::Pos| (p.line, p.column, p.offset);
    let second = &root.items()[1];
    assert_eq!((place(second.span.start), place(second.span.end)), ((3, 1, 20), (3, 7, 26)));
    let first = &root.items()[0];
    assert_eq!((place(first.span.start), place(first.span.end)), ((2, 1, 10), (2, 9, 19)));
    assert_eq!(first.entries()[1].value.span, first.span);
    let key = first.entries()[1].key_span;
    assert_eq!((place(key.start), place(key.end)), ((1, 1, 0), (1, 10, 9)));
    assert_eq!(place(root.span.end), (3, 7, 26));
}

#[test]
fn an_empty_file_or_a_header_alone_has_no_rows() {
    for text in ["", "city,zip\n"] {
        let (root, diagnostics) = csv(text);
        assert!(diagnostics.is_empty(), "{text:?}");
        assert!(root.unwrap().items().is_empty(), "{text:?}");
    }
}

#[test]
fn a_row_with_another_number_of_cells_is_v1001_on_its_line() {
    let (root, diagnostics) = csv("city,zip\nBerlin,10115\nHamburg\nUlm,89073\n");
    assert!(root.is_none());
    let all: Vec<_> = diagnostics.iter().collect();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].code, Code::V1001);
    assert_eq!(all[0].message, "not valid CSV: this row has 1 cells, but the header has 2");
    let span = all[0].primary.span;
    assert_eq!((span.start.line, span.start.column, span.end.column), (3, 1, 8));
    assert!(all[0].help[0].contains("header row"));
}

#[test]
fn a_column_named_twice_is_v1006_and_its_first_cells_are_kept() {
    let (root, diagnostics) = csv("id,id,name\n1,2,Ada\n");
    let all: Vec<_> = diagnostics.iter().collect();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].code, Code::V1006);
    assert_eq!(all[0].message, "the column `id` is named twice");
    assert_eq!(rows(&root.unwrap()), [["id=1", "name=Ada"]]);
}
