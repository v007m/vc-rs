//! The names an action saves: its own `save` and `provides`, its api's `save`, and for `test:`
//! everything the reused test saves.

use std::collections::{HashMap, HashSet};

use vc_parser::Node;

use super::project::{Item, Project, Source, TEST_BLOCKS, list, text};

/// The keys of a map node.
pub fn keys(node: Option<&Node>) -> Vec<String> {
    node.map(Node::entries).unwrap_or_default().iter().map(|entry| entry.key.clone()).collect()
}

/// The api an action or test calls.
pub fn called<'a>(project: &Project<'a>, (source, node): (&Source<'_>, &Node)) -> Option<Item<'a>> {
    project.apis.get(&source.resolve(text(node, "call")?)).copied()
}

/// The test a `test:` action reuses.
pub fn reused<'a>(project: &Project<'a>, (source, node): (&Source<'_>, &Node)) -> Option<(String, Item<'a>)> {
    let full = source.resolve(text(node, "test")?);
    project.tests.get(&full).map(|item| (full, *item))
}

/// Works out what actions save, remembering each reused test's names.
pub struct Saves<'p, 'a> {
    project: &'p Project<'a>,
    tests: HashMap<String, Vec<String>>,
    open: HashSet<String>,
}

impl<'p, 'a> Saves<'p, 'a> {
    /// No test worked out yet.
    pub fn new(project: &'p Project<'a>) -> Self {
        Self { project, tests: HashMap::new(), open: HashSet::new() }
    }

    /// The names `node`, an action or a test with `call`, saves.
    pub fn action(&mut self, source: &Source<'_>, node: &Node) -> Vec<String> {
        let mut names = keys(node.get("save"));
        names.extend(list(node, "provides").iter().filter_map(|name| name.as_str()).map(str::to_owned));
        if let Some(api) = called(self.project, (source, node)) {
            names.extend(keys(api.node.get("save")));
        }
        if let Some((full, item)) = reused(self.project, (source, node)) {
            names.extend(self.test(full, item));
        }
        names
    }

    /// Everything the test `full` saves; nothing while it is being worked out (a cycle).
    fn test(&mut self, full: String, item: Item<'a>) -> Vec<String> {
        if let Some(names) = self.tests.get(&full) {
            return names.clone();
        }
        if !self.open.insert(full.clone()) {
            return Vec::new();
        }
        let project = self.project;
        let source = project.source(item);
        let blocks = TEST_BLOCKS.iter().flat_map(|key| list(item.node, key));
        let mut names: Vec<String> = Vec::new();
        for action in blocks.chain([item.node]) {
            names.extend(self.action(source, action));
        }
        self.open.remove(&full);
        self.tests.insert(full, names.clone());
        names
    }
}
