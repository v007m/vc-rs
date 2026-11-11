//! The items a generated test needs before its call: for each `{param}` in an operation's
//! path, the `POST` on the collection above it that creates one, read back from its response
//! into a var named after the resource (`userId`), which the test passes to the parameter.

use indexmap::IndexMap;
use serde_json::{Value, json};
use vc_interfaces::{DraftApi, DraftCleanup};
use vc_model::Method;

use super::draft::{Reader, Route};
use super::values::fields;
use crate::naming::{camel, free, singular};

/// The property of a created item that names it when none has the path parameter's name.
const ID: &str = "id";

/// What a route's tests need before the call.
pub enum Needs {
    /// The creators to call first, outermost first; none for a path without parameters.
    Made(Vec<Link>),
    /// No api creates the item one parameter names: the skip reason.
    Missing(String),
}

impl Needs {
    /// The creators to call first; none when one is missing.
    pub fn links(&self) -> &[Link] {
        match self {
            Self::Made(links) => links,
            Self::Missing(_) => &[],
        }
    }

    fn links_mut(&mut self) -> &mut [Link] {
        match self {
            Self::Made(links) => links,
            Self::Missing(_) => &mut [],
        }
    }
}

/// A creator a test calls first: it saves the var a path parameter reads.
pub struct Link {
    /// The creator, as an index into the routes.
    pub creator: usize,
    /// The creator's api ID.
    pub id: String,
    /// The item's own path, `<collection>/{<param>}`.
    pub item: String,
    /// The var the parameter reads.
    pub var: String,
    /// The var the creator saves the item's ID in, unique in the file (`userId`).
    pub saved: String,
    /// The property of the creator's response body that holds the item's ID.
    pub property: String,
}

impl Reader<'_> {
    /// What `route`'s tests need: a creator in `routes` for each parameter of its path, from
    /// the last parameter out.
    pub fn needs(&self, routes: &[Route<'_>], route: &Route<'_>) -> Needs {
        let mut links = Vec::new();
        let mut path = route.path.as_str();
        while let Some((collection, param)) = last_param(path) {
            let Some(link) = self.creator(routes, collection, param) else {
                return Needs::Missing(missing(collection, param));
            };
            links.push(link);
            path = collection;
        }
        links.reverse();
        Needs::Made(links)
    }

    /// The `POST` on `collection` whose success response has a property named `param`, or
    /// [`ID`].
    fn creator(&self, routes: &[Route<'_>], collection: &str, param: &str) -> Option<Link> {
        let mut posts = routes.iter().enumerate();
        let (creator, route) = posts.find(|(_, route)| route.method == Method::Post && route.path == collection)?;
        let (_, schema) = self.success_schema(route)?;
        let (properties, _) = fields(self.doc, schema)?;
        let property = [param, ID].into_iter().find(|name| properties.contains_key(*name))?;
        let item = format!("{collection}/{{{param}}}");
        let saved = saved(collection, param);
        Some(Link { creator, id: route.id.clone(), item, var: camel(param), saved, property: property.to_owned() })
    }
}

/// Gives the creators the `used` routes call distinct saves, in route order: a creator whose
/// save an earlier one already has saves `<name>2`, `<name>3`… instead.
pub fn unique(needs: &mut [Needs], used: &[bool]) {
    let mut links: Vec<&Link> =
        needs.iter().zip(used).filter(|(_, used)| **used).flat_map(|(needs, _)| needs.links()).collect();
    links.sort_by_key(|link| link.creator);
    let mut names: Vec<(usize, String)> = Vec::new();
    for link in links {
        if !names.iter().any(|(creator, _)| *creator == link.creator) {
            let name = free(&link.saved, names.len(), |name| names.iter().any(|(_, taken)| taken == name));
            names.push((link.creator, name));
        }
    }
    for link in needs.iter_mut().flat_map(Needs::links_mut) {
        if let Some((_, name)) = names.iter().find(|(creator, _)| *creator == link.creator) {
            link.saved.clone_from(name);
        }
    }
}

/// The vars a test passes so each path parameter reads its creator's save:
/// `<param>: ${var.<saved>}`, where the two names differ.
pub fn passed(links: &[Link]) -> IndexMap<String, Value> {
    let differ = links.iter().filter(|link| link.var != link.saved);
    differ.map(|link| (link.var.clone(), json!(format!("${{var.{}}}", link.saved)))).collect()
}

/// Gives each creator `link` names the `save` its dependents read and, when an api in the
/// same routes deletes the item, the `cleanup` that removes it, keyed by the deleter's own
/// parameter.
pub fn provide(apis: &mut [DraftApi], routes: &[Route<'_>], link: &Link) {
    let read = format!("${{response.body.{}}}", link.property);
    let deleter =
        routes.iter().zip(apis.iter()).find(|(route, _)| route.method == Method::Delete && route.path == link.item);
    let cleanup = deleter.map(|(_, api)| DraftCleanup {
        call: api.id.clone(),
        vars: IndexMap::from([(link.var.clone(), read.clone())]),
    });
    let Some(creator) = apis.get_mut(link.creator) else { return };
    creator.save.entry(link.saved.clone()).or_insert(read);
    if creator.cleanup.is_none() {
        creator.cleanup = cleanup;
    }
}

/// The collection a path's last parameter is under, and the parameter's name:
/// `/users/{userId}/orders` gives `/users` and `userId`.
fn last_param(path: &str) -> Option<(&str, &str)> {
    let start = path.rfind("/{")?;
    let (collection, rest) = path.split_at_checked(start)?;
    let param = rest.strip_prefix("/{")?.split_once('}')?.0;
    Some((collection, param))
}

/// Why a test cannot run when no api creates the item `param` names under `collection`, and
/// what makes it run: a `skip` is static, so setting the var alone does not.
fn missing(collection: &str, param: &str) -> String {
    let resource = resource(collection).map_or_else(|| param.to_owned(), |word| word.replace(['-', '_'], " "));
    format!("needs an existing {resource}: set vars.{} to one and remove this skip", camel(param))
}

/// The var a creator on `collection` saves before [`unique`] tells clashes apart:
/// `<resource>Id` (`/users` gives `userId`, `/line_items` gives `lineItemId`), else the
/// parameter's var.
fn saved(collection: &str, param: &str) -> String {
    resource(collection).map_or_else(|| camel(param), |word| format!("{}Id", camel(&word)))
}

/// The singular of a collection's last word (`/line_items` gives `line_item`); none for the
/// root or a collection that ends in a parameter.
fn resource(collection: &str) -> Option<String> {
    let word = collection.rsplit('/').next().filter(|word| !word.is_empty() && !word.contains('{'))?;
    Some(singular(word))
}

#[cfg(test)]
mod tests {
    use super::{last_param, missing, saved};

    #[test]
    fn a_path_s_last_parameter_is_under_its_collection() {
        assert_eq!(last_param("/users/{userId}/orders/{orderId}"), Some(("/users/{userId}/orders", "orderId")));
        assert_eq!(last_param("/users/{userId}/orders"), Some(("/users", "userId")));
        assert_eq!(last_param("/users"), None);
    }

    #[test]
    fn a_missing_item_is_named_by_its_collection() {
        assert_eq!(missing("/pets", "petId"), "needs an existing pet: set vars.petId to one and remove this skip");
        assert_eq!(
            missing("/line_items", "id"),
            "needs an existing line item: set vars.id to one and remove this skip"
        );
        assert_eq!(missing("", "id"), "needs an existing id: set vars.id to one and remove this skip");
    }

    #[test]
    fn a_creator_saves_its_resource_s_id() {
        assert_eq!(saved("/users", "id"), "userId");
        assert_eq!(saved("/line_items", "id"), "lineItemId");
        assert_eq!(saved("/users/{userId}/orders", "orderId"), "orderId");
        assert_eq!(saved("", "item_id"), "itemId");
        assert_eq!(saved("/{tenant}", "id"), "id");
    }
}
