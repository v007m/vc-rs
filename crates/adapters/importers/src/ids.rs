//! An api's ID from its method and templated path, the way a person names it: an item is got,
//! replaced, updated or deleted (`getCart`), a collection is listed or created into
//! (`listOrders`, `createOrder`), a nested collection is added to (`addItem`), and a word after
//! an item is the verb done to it (`POST /orders/{id}/cancel` gives `cancelOrder`). Any other
//! method or shape is named by its route.

use vc_model::Method;

use crate::naming::{camel, capitalised, from_route, singular};

/// A method's verb for an item and for a collection.
struct Verbs {
    method: Method,
    item: &'static str,
    collection: &'static str,
}

/// The verbs by method.
const VERBS: [Verbs; 5] = [
    Verbs { method: Method::Get, item: "get", collection: "list" },
    Verbs { method: Method::Post, item: "create", collection: "create" },
    Verbs { method: Method::Put, item: "replace", collection: "replace" },
    Verbs { method: Method::Patch, item: "update", collection: "update" },
    Verbs { method: Method::Delete, item: "delete", collection: "delete" },
];

/// The ID of the api `method` sends to `path` (template text, `/carts/${var.cartId}`).
pub fn api_id(method: Method, path: &str) -> String {
    let segments: Vec<&str> = path.split('/').filter(|segment| !segment.is_empty()).collect();
    let var = |segment: &&str| segment.starts_with("${");
    let route = || from_route(method.name(), &path.replace("${var.", "{"));
    let Some(verbs) = VERBS.iter().find(|verbs| verbs.method == method) else { return route() };
    match segments.as_slice() {
        [] => method.name().to_ascii_lowercase(),
        [.., noun, last] if var(last) && !var(noun) => verbs.item.to_owned() + &word(&singular(noun)),
        [.., owner, id, last] if var(id) && !var(owner) && !var(last) => nested(verbs, owner, last),
        [.., last] if !var(last) => collection(verbs, last),
        _ => route(),
    }
}

/// `GET /orders` lists them; `POST /orders` creates one; the others act on them all.
fn collection(verbs: &Verbs, noun: &str) -> String {
    let noun = if verbs.method == Method::Post { singular(noun) } else { noun.to_owned() };
    verbs.collection.to_owned() + &word(&noun)
}

/// `last` after an item of `owner`: a plural is a collection it holds, anything else what is
/// done to it.
fn nested(verbs: &Verbs, owner: &str, last: &str) -> String {
    let held = singular(last) != last;
    match (verbs.method, held) {
        (Method::Get, true) => format!("list{}", word(last)),
        (Method::Post, true) => format!("add{}", word(&singular(last))),
        (Method::Post, false) => camel(last) + &word(&singular(owner)),
        (_, true) => verbs.item.to_owned() + &word(last),
        (_, false) => verbs.item.to_owned() + &word(&singular(owner)) + &word(last),
    }
}

fn word(text: &str) -> String {
    capitalised(&camel(text))
}

#[cfg(test)]
mod tests {
    use vc_model::Method;

    use super::api_id;

    #[test]
    fn ids_read_as_what_the_call_does() {
        let named = [
            (Method::Get, "/v1/carts/${var.cartId}", "getCart"),
            (Method::Get, "/v1/orders", "listOrders"),
            (Method::Post, "/v1/orders", "createOrder"),
            (Method::Put, "/v1/users/${var.userId}", "replaceUser"),
            (Method::Patch, "/v1/users/${var.userId}", "updateUser"),
            (Method::Delete, "/v1/users/${var.userId}", "deleteUser"),
            (Method::Post, "/v1/carts/${var.cartId}/items", "addItem"),
            (Method::Get, "/v1/carts/${var.cartId}/line-items", "listLineItems"),
            (Method::Post, "/v1/orders/${var.orderId}/cancel", "cancelOrder"),
            (Method::Get, "/v1/orders/${var.orderId}/status", "getOrderStatus"),
            (Method::Delete, "/v1/carts/${var.cartId}/items", "deleteItems"),
            (Method::Get, "/", "get"),
            (Method::Head, "/v1/orders", "headV1Orders"),
            (Method::Get, "/${var.id}/${var.id2}", "getIdId2"),
        ];
        let ids: Vec<_> = named.iter().map(|(method, path, _)| api_id(*method, path)).collect();
        let expected: Vec<_> = named.iter().map(|(_, _, id)| *id).collect();
        assert_eq!(ids, expected);
    }
}
