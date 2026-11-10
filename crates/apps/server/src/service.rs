//! One protobuf service over the Connect protocol: its methods by name, each a unary call or a
//! server stream, served at `POST /api/<service>/<method>`.

use std::collections::HashMap;
use std::convert::Infallible;
use std::marker::PhantomData;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::{Path, Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use bytes::Bytes;
use futures_util::future::BoxFuture;
use prost::Message;
use tokio::sync::mpsc;

use crate::envelope::{self, MAX_MESSAGE};
use crate::{Code, ConnectError, error};

const PROTO: &str = "application/proto";
const CONNECT_PROTO: &str = "application/connect+proto";

/// How many stream messages wait for a slow client before the handler waits too.
const STREAM_BUFFER: usize = 64;

type Unary = Arc<dyn Fn(Bytes) -> BoxFuture<'static, Result<Vec<u8>, ConnectError>> + Send + Sync>;
type Streaming = Arc<dyn Fn(Bytes, mpsc::Sender<Bytes>) -> BoxFuture<'static, Result<(), ConnectError>> + Send + Sync>;

#[derive(Clone)]
enum Rpc {
    Unary(Unary),
    ServerStream(Streaming),
}

/// The methods of one protobuf service, such as `vibranium.engine.v1.EngineService`.
pub struct ConnectService {
    name: &'static str,
    rpcs: HashMap<&'static str, Rpc>,
}

impl ConnectService {
    /// A service with no methods yet; `name` is its full protobuf name.
    #[must_use]
    pub fn new(name: &'static str) -> Self {
        Self { name, rpcs: HashMap::new() }
    }

    /// Serves `method` as a unary call: one request message, one response message.
    #[must_use]
    pub fn unary<Req, Resp, F, Fut>(mut self, method: &'static str, handler: F) -> Self
    where
        Req: Message + Default + 'static,
        Resp: Message + 'static,
        F: Fn(Req) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Resp, ConnectError>> + Send + 'static,
    {
        let rpc: Unary = Arc::new(move |body: Bytes| -> BoxFuture<'static, _> {
            let call = decode::<Req>(&body).map(&handler);
            Box::pin(async move { Ok(call?.await?.encode_to_vec()) })
        });
        self.rpcs.insert(method, Rpc::Unary(rpc));
        self
    }

    /// Serves `method` as a server stream: one request message, then the messages the handler
    /// sends to its [`Sink`] until it returns.
    #[must_use]
    pub fn server_stream<Req, Resp, F, Fut>(mut self, method: &'static str, handler: F) -> Self
    where
        Req: Message + Default + 'static,
        Resp: Message + 'static,
        F: Fn(Req, Sink<Resp>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), ConnectError>> + Send + 'static,
    {
        let rpc: Streaming = Arc::new(move |body: Bytes, frames: mpsc::Sender<Bytes>| -> BoxFuture<'static, _> {
            let call = decode::<Req>(&body).map(|request| handler(request, Sink::new(frames)));
            Box::pin(async move { call?.await })
        });
        self.rpcs.insert(method, Rpc::ServerStream(rpc));
        self
    }

    /// The routes that serve the methods.
    pub fn into_router(self) -> Router {
        let path = format!("/api/{}/{{method}}", self.name);
        Router::new().route(&path, post(dispatch)).with_state(Arc::new(self.rpcs))
    }
}

/// Where a server-streaming handler sends its messages, in order.
pub struct Sink<T> {
    frames: mpsc::Sender<Bytes>,
    message: PhantomData<fn(T)>,
}

impl<T: Message> Sink<T> {
    const fn new(frames: mpsc::Sender<Bytes>) -> Self {
        Self { frames, message: PhantomData }
    }

    /// Sends one message; `false` once the client has gone, when the handler should stop.
    pub async fn send(&self, message: &T) -> bool {
        self.frames.send(envelope::message(&message.encode_to_vec())).await.is_ok()
    }

    /// Resolves when the client has gone: it closed the stream or the connection dropped.
    pub async fn closed(&self) {
        self.frames.closed().await;
    }
}

async fn dispatch(
    State(rpcs): State<Arc<HashMap<&'static str, Rpc>>>,
    Path(method): Path<String>,
    request: Request,
) -> Response {
    match rpcs.get(method.as_str()).cloned() {
        Some(Rpc::Unary(call)) => unary(&call, request).await,
        Some(Rpc::ServerStream(call)) => server_stream(call, request).await,
        None => ConnectError::new(Code::Unimplemented, "the service has no such method").into_response(),
    }
}

async fn unary(call: &Unary, request: Request) -> Response {
    if let Err(refused) = accepts(request.headers(), PROTO) {
        return *refused;
    }
    let reply = match envelope::read(request.into_body(), MAX_MESSAGE).await {
        Ok(body) => call(body).await,
        Err(error) => Err(error),
    };
    match reply {
        Ok(message) => ([(header::CONTENT_TYPE, PROTO)], message).into_response(),
        Err(error) => error.into_response(),
    }
}

/// Answers at once and streams the handler's messages as it sends them, then the end-stream
/// message. A client that goes away closes the channel, which the handler sees.
async fn server_stream(call: Streaming, request: Request) -> Response {
    if let Err(refused) = accepts(request.headers(), CONNECT_PROTO) {
        return *refused;
    }
    let body = envelope::read_single(request.into_body()).await;
    let (frames, rx) = mpsc::channel(STREAM_BUFFER);
    tokio::spawn(async move {
        let outcome = match body {
            Ok(body) => call(body, frames.clone()).await,
            Err(error) => Err(error),
        };
        let end = envelope::end_stream(&error::end_stream(outcome.err().as_ref()));
        frames.send(end).await.ok();
    });
    let stream = futures_util::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|frame| (Ok::<_, Infallible>(frame), rx))
    });
    ([(header::CONTENT_TYPE, CONNECT_PROTO)], Body::from_stream(stream)).into_response()
}

/// Refuses a request whose content type is not `content_type` (415) or whose
/// `connect-protocol-version` is not 1.
fn accepts(headers: &HeaderMap, content_type: &str) -> Result<(), Box<Response>> {
    let given = headers.get(header::CONTENT_TYPE).and_then(|value| value.to_str().ok());
    let essence = given.and_then(|value| value.split(';').next()).map(str::trim);
    if !essence.is_some_and(|value| value.eq_ignore_ascii_case(content_type)) {
        return Err(Box::new(StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response()));
    }
    match headers.get("connect-protocol-version") {
        Some(version) if version != "1" => Err(Box::new(
            ConnectError::new(Code::InvalidArgument, "connect-protocol-version must be 1").into_response(),
        )),
        _ => Ok(()),
    }
}

fn decode<T: Message + Default>(body: &Bytes) -> Result<T, ConnectError> {
    T::decode(body.as_ref())
        .map_err(|_| ConnectError::new(Code::InvalidArgument, "the request is not a valid message for this method"))
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;
