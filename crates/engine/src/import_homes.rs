//! The api files a recording's hosts go to: each file whose `baseUrl` renders, under the
//! active profiles, to a full URL, by that URL's origin and path, with the method and path of
//! each of its apis so a recorded request can reuse one.

use vc_interfaces::ConfigEntry;
use vc_model::{ApiGroup, AuthRef, Method, Program};

use crate::explain::Explainer;
use crate::plan::{self, profile_flag};
use crate::shown::Shown;
use crate::{Engine, EngineError};

/// An api file of the project, where its base URL points.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Home {
    /// `scheme://host[:port]`, lower case.
    pub origin: String,
    /// The base URL's path, without a trailing `/`: `/v1`, or empty.
    pub prefix: String,
    pub file_id: String,
    /// The file's own auth, by name.
    pub auth: Option<String>,
    pub apis: Vec<Known>,
}

/// An api a file has: its ID in the file, method, path shape (`/carts/{}`) and the names of
/// its path's vars, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Known {
    pub id: String,
    pub method: Method,
    pub shape: String,
    pub vars: Vec<String>,
}

impl Engine {
    /// The api files a recording can go to under `profiles` (the default ones when empty);
    /// none when the project does not compile or its profiles are not known, since every host
    /// is then new.
    pub(crate) fn homes(&self, profiles: &[String]) -> Vec<Home> {
        self.rendered_homes(profiles).unwrap_or_default()
    }

    fn rendered_homes(&self, profiles: &[String]) -> Result<Vec<Home>, EngineError> {
        let flags: Vec<ConfigEntry> = profile_flag(profiles).into_iter().collect();
        let layered = self.layered(&[], &flags)?;
        let program = layered.program.as_ref();
        let (active, names) = plan::profiles(program, layered.settings.profiles.as_ref())?;
        let vars = Explainer { program, active: &active, names: &names, entries: &layered.entries }.config_vars();
        let shown = Shown { program, vars: &vars };
        Ok(program.api_groups.values().filter_map(|group| home(program, &shown, group)).collect())
    }
}

/// Where `group` points, when its base URL renders to a full URL.
fn home(program: &Program, shown: &Shown<'_>, group: &ApiGroup) -> Option<Home> {
    let (origin, prefix) = split_base(&shown.template(group.base_url.as_ref()?))?;
    let written = Shown { program, vars: &[] };
    let apis = program.apis.values().filter(|api| api.group == group.id).filter_map(|api| {
        let (_, id) = program.str(api.id.sym())?.split_once('#')?;
        let path = written.template(&api.path);
        Some(Known { id: id.to_owned(), method: api.method, shape: shape(&path), vars: path_vars(&path) })
    });
    let auth = match &group.auth {
        AuthRef::Use { id, .. } => program.str(id.sym()),
        AuthRef::Inherit | AuthRef::None => None,
    };
    Some(Home {
        origin,
        prefix,
        file_id: program.str(group.id.sym())?.to_owned(),
        auth: auth.map(str::to_owned),
        apis: apis.collect(),
    })
}

/// A rendered base URL as its lower-case origin and its path without a trailing `/`; none for
/// one that is not a full URL or still holds a template.
pub fn split_base(base: &str) -> Option<(String, String)> {
    let (scheme, rest) = base.split_once("://").filter(|_| !base.contains("${"))?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, path) = rest.split_at_checked(end)?;
    let path = path.split(['?', '#']).next().unwrap_or_default().trim_end_matches('/');
    (!authority.is_empty()).then(|| (format!("{scheme}://{authority}").to_ascii_lowercase(), path.to_owned()))
}

/// `path` with each `${…}` and `{…}` written as `{}`, so two paths that differ only in their
/// var names match.
pub fn shape(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    let mut depth = 0_usize;
    for c in path.chars() {
        match c {
            '{' => {
                if depth == 0 {
                    if out.ends_with('$') {
                        out.pop();
                    }
                    out.push_str("{}");
                }
                depth += 1;
            }
            '}' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// The names of the vars in `path`, in order: `cartId` for `${var.cartId}` or `{cartId}`.
pub fn path_vars(path: &str) -> Vec<String> {
    let names = path.split('{').skip(1).filter_map(|rest| {
        let inner = rest.split('}').next()?;
        let name = inner.strip_prefix("var.").unwrap_or(inner);
        let name: String = name.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        (!name.is_empty()).then_some(name)
    });
    names.collect()
}

#[cfg(test)]
mod tests {
    use super::{path_vars, shape, split_base};

    #[test]
    fn a_base_url_splits_into_its_origin_and_path() {
        let split = |base: &str| split_base(base);
        assert_eq!(split("HTTPS://API.Shop.test/v1/"), Some(("https://api.shop.test".to_owned(), "/v1".to_owned())));
        assert_eq!(split("http://localhost:7421"), Some(("http://localhost:7421".to_owned(), String::new())));
        assert_eq!(split("${var.shopUrl}/v1"), None);
        assert_eq!(split("/v1"), None);
    }

    #[test]
    fn paths_that_differ_only_in_var_names_have_one_shape() {
        assert_eq!(shape("/carts/${var.cartId}/items"), "/carts/{}/items");
        assert_eq!(shape("/carts/{id}/items"), "/carts/{}/items");
        assert_eq!(shape("/a/${var.x | upper}"), "/a/{}");
        assert_eq!(path_vars("/carts/${var.cartId}/items/{sku}"), ["cartId", "sku"]);
    }
}
