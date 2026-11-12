//! `vc plan --json`: the plan with every ID resolved to its text, for scripts and agents.

use serde::Serialize;
use serde_json::Value;
use vc_model::{ActionId, InstanceId, InstanceLabels, Needs, Plan, PlannedAction, PlannedFile, Program};

/// The plan as JSON, each instance with the label the tree and the run give it.
#[must_use]
pub fn json(program: &Program, plan: &Plan) -> Value {
    let labels = InstanceLabels::new(plan);
    let view = PlanView {
        instance_count: plan.instance_count(),
        before_run: planned(program, &plan.before_run),
        files: plan.files.iter().map(|file| file_view(program, &labels, file)).collect(),
        after_run: after(program, &plan.after_run),
        skipped: plan
            .skipped
            .iter()
            .map(|skipped| SkippedView { test: super::name(program, skipped.test.sym()), reason: &skipped.reason })
            .collect(),
    };
    serde_json::to_value(view).unwrap_or(Value::Null)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PlanView<'p> {
    instance_count: usize,
    before_run: Vec<ActionView>,
    files: Vec<FileView<'p>>,
    after_run: Vec<ActionView>,
    skipped: Vec<SkippedView<'p>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FileView<'p> {
    id: &'p str,
    before_all: Vec<ActionView>,
    instances: Vec<InstanceView<'p>>,
    after_all: Vec<ActionView>,
}

#[derive(Serialize)]
struct ActionView {
    action: ActionId,
    label: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    needs: Vec<ActionId>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InstanceView<'p> {
    id: InstanceId,
    test: &'p str,
    label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    example_row: Option<u32>,
    repeat_index: u32,
    needs: Vec<ActionId>,
}

#[derive(Serialize)]
struct SkippedView<'p> {
    test: &'p str,
    reason: &'p str,
}

fn file_view<'p>(program: &'p Program, labels: &InstanceLabels, file: &'p PlannedFile) -> FileView<'p> {
    let instances = file.instances.iter().map(|instance| InstanceView {
        id: instance.id,
        test: super::name(program, instance.test.sym()),
        label: super::instance_label(program, labels, instance),
        example_row: instance.example_row,
        repeat_index: instance.repeat_index,
        needs: ids(&instance.needs),
    });
    FileView {
        id: super::name(program, file.id.sym()),
        before_all: planned(program, &file.before_all),
        instances: instances.collect(),
        after_all: after(program, &file.after_all),
    }
}

fn planned(program: &Program, actions: &[PlannedAction]) -> Vec<ActionView> {
    let view = |planned: &PlannedAction| ActionView {
        action: planned.action,
        label: super::action_label(program, planned.action),
        needs: ids(&planned.needs),
    };
    actions.iter().map(view).collect()
}

fn after(program: &Program, actions: &[ActionId]) -> Vec<ActionView> {
    let view = |action: &ActionId| ActionView {
        action: *action,
        label: super::action_label(program, *action),
        needs: Vec::new(),
    };
    actions.iter().map(view).collect()
}

fn ids(needs: &Needs) -> Vec<ActionId> {
    needs.iter().collect()
}
