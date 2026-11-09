//! The draft api file for an origin a recording or a collection calls: named after its host's
//! first label other than `www` and `api`, with the origin a base-URL var. A user and password
//! in the origin are never written.

use indexmap::IndexMap;
use serde_json::json;
use vc_interfaces::DraftGroup;

use crate::naming::{camel, file_id, free};
use crate::secret::DROPPED;

/// A new draft api file for `origin`, under a file ID none of `taken` has; a host with no word
/// to name it by gives `fallback`.
pub fn group(origin: &str, fallback: &str, taken: &[&str]) -> DraftGroup {
    let file_id = free(&file_id(origin, fallback), taken.len(), |id| taken.contains(&id));
    let var = format!("{}Url", camel(&file_id));
    DraftGroup {
        base_url: Some(format!("${{var.{var}}}")),
        config_vars: IndexMap::from([(var, json!(origin))]),
        file_id,
        ..DraftGroup::default()
    }
}

/// `origin` without a user and password, which only a note in `notes` mentions.
pub fn without_user(origin: &str, notes: &mut Vec<String>) -> String {
    let Some((scheme, authority)) = origin.split_once("://") else { return origin.to_owned() };
    let Some((_, host)) = authority.rsplit_once('@') else { return origin.to_owned() };
    notes.push(format!("the URL's user and password: {DROPPED}"));
    format!("{scheme}://{host}")
}
