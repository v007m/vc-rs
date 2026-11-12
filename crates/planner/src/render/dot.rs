//! `vc plan --dot`: every lifecycle action and test instance as a Graphviz node, each file a
//! cluster. A dashed edge runs from each node to those that run after it (`beforeRun`, then each
//! file's `beforeAll`, `beforeEach`, instances, `afterEach` and `afterAll`, then `afterRun`); a
//! solid edge runs from a node to each shared action whose saved values it reads (D122).

use std::fmt::Write as _;

use vc_model::{ActionId, Instance, InstanceLabels, Needs, Plan, PlannedFile, Program};

/// Nodes that run together, after every node of the stage before.
type Stage = Vec<String>;

/// The graph being written.
struct Graph<'p> {
    program: &'p Program,
    labels: InstanceLabels,
    out: String,
}

/// The plan as a Graphviz `digraph`.
#[must_use]
pub fn dot(program: &Program, plan: &Plan) -> String {
    let mut graph =
        Graph { program, labels: InstanceLabels::new(plan), out: String::from("digraph plan {\n  rankdir=LR;\n") };
    let before = graph.actions("beforeRun", plan.before_run.iter().map(|planned| planned.action), "  ");
    for planned in &plan.before_run {
        graph.needs(&node_id(planned.action), &planned.needs);
    }
    graph.order(&before);
    let files: Vec<Vec<Stage>> = plan.files.iter().enumerate().map(|(index, file)| graph.file(index, file)).collect();
    let after = graph.actions("afterRun", plan.after_run.iter().copied(), "  ");
    graph.order(&after);
    for stages in files {
        let run: Vec<Stage> = before.last().cloned().into_iter().chain(stages).chain(after.first().cloned()).collect();
        graph.order(&run);
    }
    graph.out.push_str("}\n");
    graph.out
}

impl Graph<'_> {
    /// One file's cluster and its `needs` edges; its stages, in the order they run, for the
    /// caller to order between `beforeRun` and `afterRun`.
    fn file(&mut self, index: usize, file: &PlannedFile) -> Vec<Stage> {
        let label = quote(super::name(self.program, file.id.sym()));
        let _ = writeln!(self.out, "  subgraph cluster_{index} {{\n    label={label};");
        let program = self.program;
        let (before_each, after_each) = program.test_files.get(&file.id).map_or((&[][..], &[][..]), |test_file| {
            (test_file.lifecycle.before_each.as_slice(), test_file.lifecycle.after_each.as_slice())
        });
        let mut stages = self.actions("beforeAll", file.before_all.iter().map(|planned| planned.action), "    ");
        stages.extend(self.actions("beforeEach", before_each.iter().copied(), "    "));
        stages.push(file.instances.iter().map(|instance| self.instance(instance)).collect());
        stages.extend(self.actions("afterEach", after_each.iter().copied(), "    "));
        stages.extend(self.actions("afterAll", file.after_all.iter().copied(), "    "));
        self.out.push_str("  }\n");
        for planned in &file.before_all {
            self.needs(&node_id(planned.action), &planned.needs);
        }
        for instance in &file.instances {
            self.needs(&format!("i{}", instance.id.index()), &instance.needs);
        }
        stages.retain(|stage| !stage.is_empty());
        stages
    }

    /// An instance's node, written; its ID.
    fn instance(&mut self, instance: &Instance) -> String {
        let id = format!("i{}", instance.id.index());
        let label = super::instance_label(self.program, &self.labels, instance);
        let _ = writeln!(self.out, "    {id} [label={}];", quote(&label));
        id
    }

    /// A node for each action of the `block`, written at `indent`; one stage each, in order.
    fn actions(&mut self, block: &str, actions: impl Iterator<Item = ActionId>, indent: &str) -> Vec<Stage> {
        let mut stages = Vec::new();
        for action in actions {
            let id = node_id(action);
            let label = format!("{block} {}", super::action_label(self.program, action));
            let _ = writeln!(self.out, "{indent}{id} [label={}];", quote(&label));
            stages.push(vec![id]);
        }
        stages
    }

    /// A dashed edge from each node of a stage to each of the next one that has nodes.
    fn order(&mut self, stages: &[Stage]) {
        let filled: Vec<&Stage> = stages.iter().filter(|stage| !stage.is_empty()).collect();
        for pair in filled.windows(2) {
            let [earlier, later] = pair else { continue };
            for (from, to) in earlier.iter().flat_map(|from| later.iter().map(move |to| (from, to))) {
                let _ = writeln!(self.out, "  {from} -> {to} [style=dashed];");
            }
        }
    }

    /// A solid edge from `from` to each shared action it needs.
    fn needs(&mut self, from: &str, needs: &Needs) {
        for action in needs.iter() {
            let _ = writeln!(self.out, "  {from} -> {};", node_id(action));
        }
    }
}

fn node_id(action: ActionId) -> String {
    format!("a{}", action.index())
}

fn quote(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}
