use axum::extract::{MatchedPath, Request, State};
use axum::middleware::Next;
use axum::response::Response;

use crate::store::Store;

/// Counts the request under `METHOD /route/{template}` and tracks it as in flight until the
/// handler finishes or the request is dropped.
pub async fn track(State(store): State<Store>, request: Request, next: Next) -> Response {
    let template = request.extensions().get::<MatchedPath>().map_or("<unmatched>", MatchedPath::as_str);
    let route = format!("{} {template}", request.method());
    let _in_flight = InFlight::begin(store, route);
    next.run(request).await
}

struct InFlight {
    store: Store,
    ticket: u64,
}

impl InFlight {
    fn begin(store: Store, route: String) -> Self {
        let ticket = store.ticket();
        store.tell(move |state| state.traffic.begin(ticket, route));
        Self { store, ticket }
    }
}

impl Drop for InFlight {
    fn drop(&mut self) {
        let ticket = self.ticket;
        self.store.tell(move |state| state.traffic.end(ticket));
    }
}
