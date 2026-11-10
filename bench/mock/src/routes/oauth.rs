use std::collections::BTreeMap;

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing;
use axum::{Form, Json, Router};
use serde_json::{Value, json};

use crate::error::MockError;
use crate::store::Store;

const EXPIRES_IN_SECS: u64 = 3600;
/// `Basic` credentials for the confidential client `vc-client` with secret `vc-secret`.
const CONFIDENTIAL: &str = "Basic dmMtY2xpZW50OnZjLXNlY3JldA==";
/// The public client, which sends its `client_id` in the form and has no secret.
const PUBLIC: &str = "vc-public";
/// The one user the password grant knows.
const USER: (&str, &str) = ("ada", "lovelace");

type Fields = BTreeMap<String, String>;

/// An OAuth 2.0 authorization server: `POST /oauth/token` answers the `client_credentials`,
/// `password`, `refresh_token` and `authorization_code` grants, and `GET /oauth/authorize`
/// signs in at once and redirects to `redirect_uri` with a code and the `state`.
/// Its access tokens work on `/auth/me` and last an hour, or `?expiresIn=` seconds.
pub fn routes(store: &Store) -> Router {
    Router::new()
        .route("/oauth/token", routing::post(token))
        .route("/oauth/authorize", routing::get(authorize))
        .with_state(store.clone())
}

/// Takes the token URL's query and the client's headers together.
async fn token(
    State(store): State<Store>,
    (Query(query), headers): (Query<Fields>, HeaderMap),
    Form(fields): Form<Fields>,
) -> Response {
    if !known_client(&headers, &fields) {
        return refused(StatusCode::UNAUTHORIZED, "invalid_client");
    }
    match grant(&store, &fields).await {
        Ok(Some(refresh)) => issue(&store, refresh, lifetime(&query)).await.unwrap_or_else(IntoResponse::into_response),
        Ok(None) => refused(StatusCode::BAD_REQUEST, "invalid_grant"),
        Err(error) => error.into_response(),
    }
}

/// The grants the server answers, by `grant_type`.
#[derive(Clone, Copy)]
enum Grant {
    ClientCredentials,
    Password,
    Refresh,
    Code,
}

const GRANTS: [(&str, Grant); 4] = [
    ("client_credentials", Grant::ClientCredentials),
    ("password", Grant::Password),
    ("refresh_token", Grant::Refresh),
    ("authorization_code", Grant::Code),
];

/// Whether the grant in `fields` holds; `Some(true)` when it earns a refresh token.
async fn grant(store: &Store, fields: &Fields) -> Result<Option<bool>, MockError> {
    let field = |name: &str| fields.get(name).cloned().unwrap_or_default();
    let kind = GRANTS.iter().find(|(name, _)| *name == field("grant_type")).map(|(_, grant)| *grant);
    match kind {
        Some(Grant::ClientCredentials) => Ok(Some(false)),
        Some(Grant::Password) => Ok((field("username") == USER.0 && field("password") == USER.1).then_some(true)),
        Some(Grant::Refresh) => {
            let refresh = field("refresh_token");
            Ok(store.call(move |state| state.sessions.refreshes(&refresh)).await?.then_some(true))
        }
        Some(Grant::Code) => {
            let code = field("code");
            Ok(store.call(move |state| state.sessions.redeems(&code)).await?.then_some(true))
        }
        None => Ok(None),
    }
}

async fn issue(store: &Store, refresh: bool, expires_in: u64) -> Result<Response, MockError> {
    let (access, refresh) =
        store.call(move |state| (state.sessions.login(), refresh.then(|| state.sessions.refresh_token()))).await?;
    let mut body = json!({ "access_token": access, "token_type": "Bearer", "expires_in": expires_in });
    if let (Some(refresh), Value::Object(map)) = (refresh, &mut body) {
        map.insert("refresh_token".to_owned(), Value::String(refresh));
    }
    Ok(Json(body).into_response())
}

async fn authorize(State(store): State<Store>, Query(query): Query<Fields>) -> Response {
    let Some(redirect) = query.get("redirect_uri") else {
        return refused(StatusCode::BAD_REQUEST, "invalid_request");
    };
    match store.call(|state| state.sessions.code()).await {
        Ok(code) => {
            let state = query.get("state").map(|state| format!("&state={state}")).unwrap_or_default();
            let separator = if redirect.contains('?') { '&' } else { '?' };
            Redirect::to(&format!("{redirect}{separator}code={code}{state}")).into_response()
        }
        Err(error) => error.into_response(),
    }
}

/// The `expiresIn` the token URL's query asks for, else an hour.
fn lifetime(query: &Fields) -> u64 {
    query.get("expiresIn").and_then(|secs| secs.parse().ok()).unwrap_or(EXPIRES_IN_SECS)
}

fn known_client(headers: &HeaderMap, fields: &Fields) -> bool {
    let basic = headers.get(header::AUTHORIZATION).and_then(|value| value.to_str().ok());
    basic == Some(CONFIDENTIAL) || fields.get("client_id").is_some_and(|id| id == PUBLIC)
}

fn refused(status: StatusCode, error: &str) -> Response {
    (status, Json(json!({ "error": error }))).into_response()
}
