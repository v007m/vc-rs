//! `/users`: a small store, bounded (D331). Ada (`1`) and Grace (`2`) are always there and
//! cannot be changed; created users get `3`, `4`, … (never reused), and at most [`MAX_USERS`] of
//! them are held at once. One task owns the users and runs each request's job in turn, so the
//! store needs no lock: handlers send it jobs over a channel and wait for the answer.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{Map, Value};
use tokio::sync::{OnceCell, mpsc, oneshot};

use crate::scripted::{Asked, Refused, Segment, number};

/// The most created users the demo holds at once; deleting one makes room.
pub const MAX_USERS: usize = 1024;

/// The largest user, as compact JSON without its `id`, in bytes.
pub const MAX_USER_BYTES: usize = 4096;

/// The largest body read as a user: room for spacing and for an `id` sent back, as the user
/// itself is measured once it is read.
const MAX_BODY_BYTES: usize = 2 * MAX_USER_BYTES;

/// The ID the first created user gets, after Ada and Grace.
const FIRST_ID: u64 = 3;

/// Ada and Grace, the users the demo starts with.
const SEEDS: usize = 2;

/// The roles a user may have.
const ROLES: [&str; 2] = ["admin", "member"];

/// The jobs waiting for the store, at most.
const QUEUE: usize = 256;

/// A user: a JSON object with a text `id`, `name` and `email`, an optional `role`, and whatever
/// else it was given.
type User = Map<String, Value>;

/// A job the store runs on its users.
type Job = Box<dyn FnOnce(&mut Book) + Send>;

/// The users, by number, and the number the next created one gets.
struct Book {
    users: BTreeMap<u64, User>,
    next: u64,
}

/// Why a users request is refused.
enum Refusal {
    /// 400 with the reason.
    Bad(Refused),
    /// 403: Ada and Grace stay as they are.
    Fixed,
    /// 404: no such user.
    Missing,
    /// 503: the store has stopped, as the demo is shutting down.
    Stopped,
}

/// A users route's answer.
type Answer = Result<Response, Refusal>;

/// The handle the routes share: the store's job queue, opened by the first request, so building
/// the router needs no runtime.
#[derive(Clone, Default)]
struct Users {
    jobs: Arc<OnceCell<mpsc::Sender<Job>>>,
}

impl Users {
    /// Runs `job` on the store and returns what it gave.
    async fn call<T: Send + 'static>(&self, job: impl FnOnce(&mut Book) -> T + Send + 'static) -> Result<T, Refusal> {
        let jobs = self.jobs.get_or_init(|| async { open() }).await;
        let (reply, answer) = oneshot::channel();
        let job: Job = Box::new(move |book| {
            let _gone = reply.send(job(book));
        });
        jobs.send(job).await.map_err(|_| Refusal::Stopped)?;
        answer.await.map_err(|_| Refusal::Stopped)
    }
}

/// Starts the task that owns the users; it ends when the last [`Users`] is dropped.
fn open() -> mpsc::Sender<Job> {
    let (jobs, mut inbox) = mpsc::channel::<Job>(QUEUE);
    drop(tokio::spawn(async move {
        let mut book = Book::new();
        while let Some(job) = inbox.recv().await {
            job(&mut book);
        }
    }));
    jobs
}

impl Book {
    fn new() -> Self {
        let seed =
            |id: &str, name: &str| User::from_iter([("id".to_owned(), id.into()), ("name".to_owned(), name.into())]);
        let users = BTreeMap::from([(1, seed("1", "Ada Lovelace")), (2, seed("2", "Grace Hopper"))]);
        Self { users, next: FIRST_ID }
    }

    /// The users in ID order, at most `limit` of them.
    fn list(&self, limit: Option<usize>) -> Vec<User> {
        self.users.values().take(limit.unwrap_or(usize::MAX)).cloned().collect()
    }

    /// Adds `user` with the next ID, while there is room.
    fn add(&mut self, mut user: User) -> Result<User, Refusal> {
        if self.users.len().saturating_sub(SEEDS) >= MAX_USERS {
            let full = format!("the demo holds at most {MAX_USERS} created users; delete some, or restart it");
            return Err(Refusal::Bad(Refused(full)));
        }
        let id = self.next;
        self.next += 1;
        user.insert("id".to_owned(), id.to_string().into());
        self.users.insert(id, user.clone());
        Ok(user)
    }

    /// The created user `id`, to change or remove: Ada and Grace are refused.
    fn created(&mut self, id: u64) -> Result<&mut User, Refusal> {
        let user = self.users.get_mut(&id).ok_or(Refusal::Missing)?;
        if id < FIRST_ID { Err(Refusal::Fixed) } else { Ok(user) }
    }

    /// Replaces user `id` with what `change` makes of it, once that is a valid user.
    fn change(&mut self, id: u64, change: impl FnOnce(&User) -> Result<User, Refused>) -> Result<User, Refusal> {
        let user = self.created(id)?;
        let mut changed = change(user).map_err(Refusal::Bad)?;
        changed.insert("id".to_owned(), id.to_string().into());
        user.clone_from(&changed);
        Ok(changed)
    }

    fn remove(&mut self, id: u64) -> Result<(), Refusal> {
        self.created(id)?;
        self.users.remove(&id);
        Ok(())
    }
}

/// The users routes, sharing one store.
pub fn routes() -> Router {
    Router::new()
        .route("/users", get(list).post(create))
        .route("/users/{id}", get(read).put(replace).patch(patch).delete(remove))
        .with_state(Users::default())
}

/// `GET /users?limit=n`: every user in ID order, or the first `n`.
async fn list(State(users): State<Users>, query: Asked) -> Answer {
    let Query(query) = query.map_err(|rejection| Refusal::Bad(rejection.into()))?;
    let limit = query.contains_key("limit").then(|| number(&query, "limit")).transpose().map_err(Refusal::Bad)?;
    let limit = limit.map(|limit| usize::try_from(limit).unwrap_or(usize::MAX));
    Ok(Json(users.call(move |book| book.list(limit)).await?).into_response())
}

/// `POST /users`: 201 with the user and its new ID. The sample's `createUser` saves
/// `response.body.id` on a 201 alone (D191).
async fn create(State(users): State<Users>, body: Bytes) -> Answer {
    let user = valid(posted(&body)?).map_err(Refusal::Bad)?;
    let made = users.call(move |book| book.add(user)).await??;
    Ok((StatusCode::CREATED, Json(made)).into_response())
}

/// `GET /users/{id}`: the user, or 404.
async fn read(State(users): State<Users>, id: Segment) -> Answer {
    let id = numbered(id)?;
    let found = users.call(move |book| book.users.get(&id).cloned()).await?;
    Ok(Json(found.ok_or(Refusal::Missing)?).into_response())
}

/// `PUT /users/{id}`: replaces a created user with the body, as `POST` would make it.
async fn replace(State(users): State<Users>, id: Segment, body: Bytes) -> Answer {
    let (id, user) = (numbered(id)?, posted(&body)?);
    let changed = users.call(move |book| book.change(id, |_| valid(user))).await??;
    Ok(Json(changed).into_response())
}

/// `PATCH /users/{id}`: merges the body's keys into a created user (`null` removes a key), and
/// keeps the result when it is still a valid user.
async fn patch(State(users): State<Users>, id: Segment, body: Bytes) -> Answer {
    let (id, patch) = (numbered(id)?, posted(&body)?);
    let changed = users.call(move |book| book.change(id, |user| valid(merged(user, patch)))).await??;
    Ok(Json(changed).into_response())
}

/// `DELETE /users/{id}`: 204 once a created user is gone.
async fn remove(State(users): State<Users>, id: Segment) -> Answer {
    let id = numbered(id)?;
    users.call(move |book| book.remove(id)).await??;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// The user number `id` names, in its plain form (`3`, never `03` or `+3`); else no user has it.
fn numbered(id: Segment) -> Result<u64, Refusal> {
    let Path(id) = id.map_err(|rejection| Refusal::Bad(rejection.into()))?;
    id.parse::<u64>().ok().filter(|number| number.to_string() == id).ok_or(Refusal::Missing)
}

/// The body as a JSON object, at most [`MAX_BODY_BYTES`].
fn posted(body: &Bytes) -> Result<User, Refusal> {
    let refused = |reason: String| Refusal::Bad(Refused(reason));
    if body.len() > MAX_BODY_BYTES {
        return Err(refused(format!("a user is at most {MAX_USER_BYTES} bytes")));
    }
    match serde_json::from_slice(body) {
        Ok(Value::Object(user)) => Ok(user),
        _ => Err(refused("a user is a JSON object, with an email".to_owned())),
    }
}

/// `user` with `changes` merged in, key by key; a `null` removes the key.
fn merged(user: &User, changes: User) -> User {
    let mut merged = user.clone();
    for (key, value) in changes {
        if value.is_null() {
            merged.remove(&key);
        } else {
            merged.insert(key, value);
        }
    }
    merged
}

/// `user` once it is a valid one: a text `email`, a text `name` (`someone` when not given), a
/// `role` of [`ROLES`] when given, and at most [`MAX_USER_BYTES`] in all. Its `id` is dropped
/// first, as the store sets it, so the size is measured the same way on every route.
fn valid(mut user: User) -> Result<User, Refused> {
    user.remove("id");
    let text = |key: &str, user: &User| user.get(key).is_none_or(Value::is_string);
    if !user.get("email").is_some_and(Value::is_string) {
        return Err(Refused("a user needs an email".to_owned()));
    }
    if !text("name", &user) {
        return Err(Refused("a user's `name` is text".to_owned()));
    }
    if !user.get("role").is_none_or(|role| ROLES.iter().any(|known| role == known)) {
        return Err(Refused(format!("a user's `role` is one of {}", ROLES.join(", "))));
    }
    user.entry("name").or_insert_with(|| "someone".into());
    let size = serde_json::to_vec(&user).map_or(usize::MAX, |bytes| bytes.len());
    if size > MAX_USER_BYTES {
        return Err(Refused(format!("a user is at most {MAX_USER_BYTES} bytes")));
    }
    Ok(user)
}

impl IntoResponse for Refusal {
    fn into_response(self) -> Response {
        let error = |status: StatusCode, error: &str| Refused(error.to_owned()).with_status(status);
        match self {
            Self::Bad(refused) => refused.into_response(),
            Self::Fixed => error(StatusCode::FORBIDDEN, "users 1 and 2 are the demo's own; create a user to change"),
            Self::Missing => StatusCode::NOT_FOUND.into_response(),
            Self::Stopped => error(StatusCode::SERVICE_UNAVAILABLE, "the demo is stopping"),
        }
    }
}

#[cfg(test)]
#[path = "users_tests.rs"]
mod tests;
