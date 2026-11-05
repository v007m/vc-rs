//! Reads of saved values that a `function:` or `test:` action left undeclared (V5108, D186).

use std::cell::Cell;
use std::collections::BTreeMap;

use vc_diagnostics::Code;
use vc_model::{ActionId, Failure, Program};

use crate::{ActionKindTag, Frame};

/// The frame's undeclared saved values, each with the action that saved it and whether this
/// render has read it.
#[derive(Debug, Default)]
pub struct Reads {
    undeclared: BTreeMap<String, (ActionId, Cell<bool>)>,
    quiet: Cell<bool>,
}

impl Reads {
    /// The undeclared values `frame` holds, none read yet.
    pub fn of(frame: &Frame) -> Self {
        let undeclared = frame.origins.iter().map(|(name, origin)| (name.clone(), (*origin, Cell::new(false))));
        Self { undeclared: undeclared.collect(), quiet: Cell::new(false) }
    }

    /// Notes a read of the saved value `name`, unless reads are quiet.
    pub fn note(&self, name: &str) {
        if self.quiet.get() {
            return;
        }
        if let Some((_, read)) = self.undeclared.get(name) {
            read.set(true);
        }
    }

    /// Runs `read` without noting its reads: a hook's `vars` holds every var, read or not.
    pub fn quietly<T>(&self, read: impl FnOnce() -> T) -> T {
        let was = self.quiet.replace(true);
        let value = read();
        self.quiet.set(was);
        value
    }

    /// A V5108 warning for each undeclared value read, in name order, at the action that saved it.
    pub fn warnings(&self, program: &Program) -> Vec<Failure> {
        let read = self.undeclared.iter().filter(|(_, (_, read))| read.get());
        read.map(|(name, (origin, _))| warning(program, name, *origin)).collect()
    }
}

/// The V5108 warning for reading `name`, which `origin` saved without declaring it.
fn warning(program: &Program, name: &str, origin: ActionId) -> Failure {
    let saver = program.actions.get(usize::try_from(origin.index()).unwrap_or(usize::MAX));
    let kind = saver.map_or("function", |saver| ActionKindTag::of(&saver.kind).as_str());
    let message = format!(
        "`var.{name}` was saved by a `{kind}:` step that does not declare it; add `provides: [{name}]` to that step"
    );
    Failure::new(message).with_code(Code::V5108.to_string()).at(origin)
}
