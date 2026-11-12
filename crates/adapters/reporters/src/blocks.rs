//! The spec reporter's file blocks (D321): a file's heading and its tests together, so a run
//! that finishes tests of several files in turn still shows each file once.
//!
//! One file is live: its tests print as they finish. A test of another file waits in that
//! file's block. When the live file's last test finishes, every waiting file that has finished
//! prints whole, then the first unfinished one prints what it has and goes live. A serial run so
//! prints as it goes; whatever still waits when the run ends, cancelled or bailed, prints then.

use std::collections::HashMap;

use vc_model::{FileId, Plan, Program};

/// Each file's block, and which file prints live.
#[derive(Debug, Default)]
pub struct FileBlocks {
    /// Tests each file has still to finish, skipped ones included.
    remaining: HashMap<FileId, usize>,
    live: Option<FileId>,
    /// The other files' blocks, in the order their first test finished.
    waiting: Vec<Block>,
    /// Whether any block printed yet, so the next heading is parted from it.
    printed: bool,
}

/// A file's heading (empty for a file without a `name`) and its tests' text.
#[derive(Debug)]
struct Block {
    file: FileId,
    heading: String,
    body: String,
}

impl FileBlocks {
    /// The blocks of `plan`'s files, each expecting its instances and its skipped tests.
    pub fn new(program: &Program, plan: &Plan) -> Self {
        let mut remaining: HashMap<FileId, usize> = HashMap::new();
        for file in &plan.files {
            *remaining.entry(file.id).or_default() += file.instances.len();
        }
        for skipped in &plan.skipped {
            if let Some(test) = program.tests.get(&skipped.test) {
                *remaining.entry(test.file).or_default() += 1;
            }
        }
        Self { remaining, ..Self::default() }
    }

    /// What to print now that a test of `file` finished: its `text`, under `heading` when it
    /// starts the block, and any blocks that finishing it lets through; nothing while it waits.
    pub fn test(&mut self, file: FileId, heading: String, text: &str) -> String {
        let done = self.count_down(file);
        let mut out = match self.live {
            None => {
                self.live = Some(file);
                self.start(&heading)
            }
            Some(live) if live != file => {
                self.wait(file, heading, text);
                return String::new();
            }
            Some(_) => String::new(),
        };
        out.push_str(text);
        self.printed = true;
        if done {
            out.push_str(&self.advance());
        }
        out
    }

    /// Every block still waiting, as the run ends.
    pub fn rest(&mut self) -> String {
        self.live = None;
        std::mem::take(&mut self.waiting).iter().map(|block| self.flush(block)).collect()
    }

    /// Counts a test of `file` off; whether it was the file's last.
    fn count_down(&mut self, file: FileId) -> bool {
        let left = self.remaining.entry(file).or_default();
        *left = left.saturating_sub(1);
        *left == 0
    }

    fn wait(&mut self, file: FileId, heading: String, text: &str) {
        if let Some(block) = self.waiting.iter_mut().find(|block| block.file == file) {
            block.body.push_str(text);
        } else {
            self.waiting.push(Block { file, heading, body: text.to_owned() });
        }
    }

    /// The live file finished: the finished waiting blocks, then the first unfinished one,
    /// which goes live.
    fn advance(&mut self) -> String {
        self.live = None;
        let (finished, open): (Vec<Block>, Vec<Block>) = std::mem::take(&mut self.waiting)
            .into_iter()
            .partition(|block| self.remaining.get(&block.file).is_none_or(|left| *left == 0));
        let mut out: String = finished.iter().map(|block| self.flush(block)).collect();
        let mut open = open.into_iter();
        if let Some(next) = open.next() {
            self.live = Some(next.file);
            out.push_str(&self.flush(&next));
        }
        self.waiting = open.collect();
        out
    }

    fn flush(&mut self, block: &Block) -> String {
        let mut out = self.start(&block.heading);
        out.push_str(&block.body);
        self.printed = true;
        out
    }

    /// A heading, after a blank line when a block printed before it.
    fn start(&self, heading: &str) -> String {
        if heading.is_empty() || !self.printed { heading.to_owned() } else { format!("\n{heading}") }
    }
}

#[cfg(test)]
#[path = "blocks_tests.rs"]
mod tests;
