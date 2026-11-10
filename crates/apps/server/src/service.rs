//! One protobuf service over the Connect protocol: its methods by name, each a unary call or a
//! server stream, served at `POST /api/<service>/<method>`, in binary protobuf or protobuf JSON
//! as the request's content type says (D254).

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
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::sync::{mpsc, watch};

use crate::codec::{Codec, Shape};
use crate::envelope::{self, MAX_MESSAGE};
use crate::{Code, ConnectError, error};

/// How many stream messages wait for a slow client before the handler waits too.
const STREAM_BUFFER: usize = 64;

type Unary = Arc<dyn Fn(Bytes, Codec) -> BoxFuture<'static, Result<Vec<u8>, ConnectError>> + Send + Sync>;
type Streaming = Arc<dyn Fn(Bytes, Frames) -> BoxFuture<'static, Result<(), ConnectError>> + Send + Sync>;

#[derive(Clone)]
enum Rpc {
    Unary(Unary),
    ServerStream(Streaming),
}

/// The methods of one protobuf service, such as `vibranium.engine.v1.EngineService`.
pub struct ConnectService {
    name: &'static str,
    rpcs: HashMap<&'static str, Rpc>,
    stopping: watch::Receiver<bool>,
}

/// The methods by name, and the server's stop signal their streams watch.
struct Methods {
    rpcs: HashMap<&'static str, Rpc>,
    stopping: watch::Receiver<bool>,
}

/// Where a stream's frames go, the codec they are written in, and the server's stop signal.
struct Frames {
    sender: mpsc::Sender<Bytes>,
    codec: Codec,
    stopping: watch::Receiver<bool>,
}

impl ConnectService {
    /// A service with no methods yet; `name` is its full protobuf name.
    #[must_use]
    pub fn new(name: &'static str) -> Self {
        Self { name, rpcs: HashMap::new(), stopping: watch::channel(false).1 }
    }

    /// The service's full protobuf name.
    pub(crate) const fn name(&self) -> &'static str {
        self.name
    }

    /// Streams that end when `stopping` turns true, as the server's shutdown does.
    #[must_use]
    pub(crate) fn stopped_by(mut self, stopping: watch::Receiver<bool>) -> Self {
        self.stopping = stopping;
        self
    }

    /// Serves `method` as a unary call: one request message, one response message, in the
    /// codec the request came in.
    #[must_use]
    pub fn unary<Req, Resp, F, Fut>(mut self, method: &'static str, handler: F) -> Self
    where
        Req: Message + Default + DeserializeOwned + 'static,
        Resp: Message + Serialize + 'static,
        F: Fn(Req) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Resp, ConnectError>> + Send + 'static,
    {
        let rpc: Unary = Arc::new(move |body: Bytes, codec: Codec| -> BoxFuture<'static, _> {
            let call = codec.decode::<Req>(&body).map(&handler);
            Box::pin(async move { codec.encode(&call?.await?) })
        });
        self.rpcs.insert(method, Rpc::Unary(rpc));
        self
    }

    /// Serves `method` as a server stream: one request message, then the messages the handler
    /// sends to its [`Sink`] until it returns.
    #[must_use]
    pub fn server_stream<Req, Resp, F, Fut>(mut self, method: &'static str, handler: F) -> Self
    where
        Req: Message + Default + DeserializeOwned + 'static,
        Resp: Message + Serialize + 'static,
        F: Fn(Req, Sink<Resp>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), ConnectError>> + Send + 'static,
    {
        let rpc: Streaming = Arc::new(move |body: Bytes, frames: Frames| -> BoxFuture<'static, _> {
            let call = frames.codec.decode::<Req>(&body).map(|request| handler(request, Sink::new(frames)));
            Box::pin(async move { call?.await })
        });
        self.rpcs.insert(method, Rpc::ServerStream(rpc));
        self
    }

    /// The routes that serve the methods.
    pub fn into_router(self) -> Router {
        let path = format!("/api/{}/{{method}}", self.name);
        let methods = Methods { rpcs: self.rpcs, stopping: self.stopping };
        Router::new().route(&path, post(dispatch)).with_state(Arc::new(methods))
    }
}

/// Where a server-streaming handler sends its messages, in order.
pub struct Sink<T> {
    frames: Frames,
    message: PhantomData<fn(T)>,
}

impl<T: Message + Serialize> Sink<T> {
    const fn new(frames: Frames) -> Self {
        Self { frames, message: PhantomData }
    }

    /// Sends one message. The handler should stop on an error and return it, which ends the
    /// stream with it.
    ///
    /// # Errors
    ///
    /// `internal` when the message cannot be written in the stream's codec; `canceled` once the
    /// client has gone.
    pub async fn send(&self, message: &T) -> Result<(), ConnectError> {
        let payload = self.frames.codec.encode(message)?;
        let sent = self.frames.sender.send(envelope::message(&payload)).await;
        sent.map_err(|_| ConnectError::new(Code::Canceled, "the client has gone"))
    }

    /// Resolves when the client has gone (it closed the stream or the connection dropped) or
    /// the server is shutting down; either way the handler should wind up.
    pub async fn closed(&self) {
        let mut stopping = self.frames.stopping.clone();
        tokio::select! {
            () = self.frames.sender.closed() => {}
            () = stopped(&mut stopping) => {}
        }
    }
}

/// Resolves once `stopping` turns true; never when its sender went without stopping.
async fn stopped(stopping: &mut watch::Receiver<bool>) {
    if stopping.wait_for(|stop| *stop).await.is_err() {
        std::future::pending::<()>().await;
    }
}

async fn dispatch(State(methods): State<Arc<Methods>>, Path(method): Path<String>, request: Request) -> Response {
    match methods.rpcs.get(method.as_str()).cloned() {
        Some(Rpc::Unary(call)) => unary(&call, request).await,
        Some(Rpc::ServerStream(call)) => server_stream(call, request, methods.stopping.clone()).await,
        None => no_such_method(),
    }
}

/// The answer to a procedure the service does not have: HTTP 404, as a Connect router answers a
/// path it does not serve and as a client reads `unimplemented`, with that code in the error JSON.
fn no_such_method() -> Response {
    let mut response = ConnectError::new(Code::Unimplemented, "the service has no such method").into_response();
    *response.status_mut() = StatusCode::NOT_FOUND;
    response
}

async fn unary(call: &Unary, request: Request) -> Response {
    let codec = match accepts(request.headers(), Shape::Unary) {
        Ok(codec) => codec,
        Err(refused) => return *refused,
    };
    let reply = match envelope::read(request.into_body(), MAX_MESSAGE).await {
        Ok(body) => call(body, codec).await,
        Err(error) => Err(error),
    };
    match reply {
        Ok(message) => ([(header::CONTENT_TYPE, codec.content_type(Shape::Unary))], message).into_response(),
        Err(error) => error.into_response(),
    }
}

/// Answers at once and streams the handler's messages as it sends them, then the end-stream
/// message. A client that goes away closes the channel, which the handler sees, as it sees the
/// server's `stopping`.
async fn server_stream(call: Streaming, request: Request, stopping: watch::Receiver<bool>) -> Response {
    let codec = match accepts(request.headers(), Shape::Stream) {
        Ok(codec) => codec,
        Err(refused) => return *refused,
    };
    let body = envelope::read_single(request.into_body()).await;
    let (frames, rx) = mpsc::channel(STREAM_BUFFER);
    tokio::spawn(async move {
        let outcome = match body {
            Ok(body) => call(body, Frames { sender: frames.clone(), codec, stopping }).await,
            Err(error) => Err(error),
        };
        let end = envelope::end_stream(&error::end_stream(outcome.err().as_ref()));
        frames.send(end).await.ok();
    });
    let stream = futures_util::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|frame| (Ok::<_, Infallible>(frame), rx))
    });
    ([(header::CONTENT_TYPE, codec.content_type(Shape::Stream))], Body::from_stream(stream)).into_response()
}

/// The codec of a request whose content type is one a call of `shape` takes; refuses any other
/// content type (415) and a `connect-protocol-version` that is not 1.
fn accepts(headers: &HeaderMap, shape: Shape) -> Result<Codec, Box<Response>> {
    let given = headers.get(header::CONTENT_TYPE).and_then(|value| value.to_str().ok());
    let essence = given.and_then(|value| value.split(';').next()).map(str::trim);
    let Some(codec) = essence.and_then(|essence| Codec::of(essence, shape)) else {
        return Err(Box::new(StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response()));
    };
    match headers.get("connect-protocol-version") {
        Some(version) if version != "1" => Err(Box::new(
            ConnectError::new(Code::InvalidArgument, "connect-protocol-version must be 1").into_response(),
        )),
        _ => Ok(codec),
    }
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;
