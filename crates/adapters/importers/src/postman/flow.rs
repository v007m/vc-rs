//! The collection as one test (`--flow`): a step per request, in collection order. A step saves
//! what its test script saves, and a later step that calls an api with a var of that name sends
//! the saved value, so the test replays with fresh data.

use indexmap::IndexMap;
use serde_json::json;
use vc_interfaces::{DraftFlow, DraftStep};

use super::draft::Requested;
use crate::naming::flow_id;

/// The flow `id` (`<fileId>#<testId>`; a bare test ID goes in `<fallback>.recorded`) calling
/// each request with what its script saves.
pub fn flow(
    id: &str,
    fallback: &str,
    steps: impl IntoIterator<Item = (Requested, IndexMap<String, String>)>,
) -> DraftFlow {
    let (file_id, test_id) = flow_id(id, fallback);
    let mut saved: Vec<String> = Vec::new();
    let mut drafted = Vec::new();
    for (requested, save) in steps {
        let vars = requested.vars.into_iter().filter(|var| saved.contains(var));
        let vars = vars.map(|var| {
            let template = json!(format!("${{var.{var}}}"));
            (var, template)
        });
        let vars = vars.collect();
        saved.extend(save.keys().cloned());
        drafted.push(DraftStep { call: requested.call, vars, save });
    }
    DraftFlow { file_id, test_id, steps: drafted }
}
