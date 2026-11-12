use std::collections::HashMap;

use vc_model::{FileId, StringTable};

use super::FileBlocks;

/// Files `a`, `b` and `c`, and blocks expecting `tests` of each.
fn blocks(tests: [usize; 3]) -> ([FileId; 3], FileBlocks) {
    let mut table = StringTable::new();
    let files = ["a", "b", "c"].map(|id| FileId::new(table.intern(id).unwrap()));
    let remaining = HashMap::from([(files[0], tests[0]), (files[1], tests[1]), (files[2], tests[2])]);
    (files, FileBlocks { remaining, ..FileBlocks::default() })
}

/// Each test's heading is its file's name and its text is `<file><n>`.
fn finish(blocks: &mut FileBlocks, file: FileId, name: &str, n: u8) -> String {
    blocks.test(file, format!("{name}\n"), &format!("{name}{n}\n"))
}

#[test]
fn a_waiting_files_tests_gather_under_its_one_heading() {
    let ([a, b, _], mut blocks) = blocks([2, 2, 0]);
    let mut out = finish(&mut blocks, a, "A", 1);
    out += &finish(&mut blocks, b, "B", 1);
    out += &finish(&mut blocks, b, "B", 2);
    out += &finish(&mut blocks, a, "A", 2);
    assert_eq!(out + &blocks.rest(), "A\nA1\nA2\n\nB\nB1\nB2\n");
}

#[test]
fn when_the_live_file_ends_finished_blocks_print_before_the_next_live_one() {
    let ([a, b, c], mut blocks) = blocks([2, 1, 2]);
    let mut out = finish(&mut blocks, a, "A", 1);
    out += &finish(&mut blocks, c, "C", 1);
    out += &finish(&mut blocks, b, "B", 1);
    out += &finish(&mut blocks, a, "A", 2);
    out += &finish(&mut blocks, c, "C", 2);
    assert_eq!(out + &blocks.rest(), "A\nA1\nA2\n\nB\nB1\n\nC\nC1\nC2\n");
}
