# Streams: SSE and WebSocket (#92, #93)

Status: design note, not yet approved. The D rows and V codes below are candidates for Round 79 and are not in `decision-log.md` yet.

## Goal and scope

Test Server-Sent Events and WebSocket apis the way HTTP and GraphQL are tested: `type:` on the api (D342), the same `expect` merge (D123), matchers, `save`, auth and pipeline. Add as little syntax as possible: one new call key, `exchange`, and one new check root, `messages`.

Core scope:

- Open a stream: an SSE request (any method, any body) or a WebSocket handshake, with the api's headers, query and auth.
- Send messages (WebSocket only).
- Receive messages, one at a time in order or a group in any order, checked with the existing matchers.
- One time limit for the call, and a close at the end.
- Check the opening response (`status`, `headers`, `timing`) and the whole list of messages.

Everything else is in "Later" below.

## Syntax by example

An SSE api. `sse` reads the opening response as `http` does, so `method`, `body`, `status` and `headers` keep their meaning; LLM-style streams that `POST` a body work unchanged.

```yaml
# services/orders/orders.api.yaml
type: sse
baseUrl: ${var.ordersUrl}
apis:
  - id: events
    path: /orders/${var.orderId}/events
    expect:
      status: 200
    exchange:
      - receive: { event: created }
      - receive:
          event: paid
          data: { match: { total: { gt: 0 } } }
```

A WebSocket api, and a test that overrides its exchange:

```yaml
# services/chat/chat.api.yaml
type: websocket
baseUrl: ${var.chatUrl}            # ws://, wss://, http:// or https://
apis:
  - id: room
    path: /ws/rooms/${var.roomId}
    auth: member

# services/chat/rooms.test.yaml
tests:
  - id: echoesEachMessageToTheRoom
    call: chat#room
    vars: { roomId: r1 }
    timeout: 5s
    exchange:
      - send: { type: join }
      - receive: { data: { match: { type: joined } } }
      - send: { type: say, text: hi }
      - receive:                       # a group: both, in any order
          - data: { match: { type: message, text: hi } }
          - data: { match: { type: seen } }
    expect:
      messages: { none: { data: { match: { type: error } } } }
    save:
      memberId: ${response.exchange[1].data.memberId}
```

## Semantics

Exchange (`exchange`, a list):

- Each item is `send: <value>` or `receive: <check>`. Items run in order over one connection. `send` on an `sse` api is V2213.
- `exchange` may be written on the api, the test or a step. Like `body`, the most specific one replaces the others whole; it is not merged.
- With no `receive` item, the call reads until the server closes the stream.

Sending:

- `send` is a template. A map or list is sent as one JSON text frame; any other value is sent as a text frame.
- Every `send` is rendered before the connection opens, so it reads vars and earlier saves, not messages from this call.

Receiving:

- `receive: <check>` waits for the next message that the check matches. Messages before it that do not match are skipped. This tolerates heartbeats and interleaved noise.
- A list under `receive` is a group. Each entry pairs with its own message, in any order, one-to-one, as `expect.errors` lists do (D362). The group is done when every entry has a message.
- A check is a map of matchers over the message's fields, the same matchers as `body.match`.

Message shape. The shape is per type, and an unknown field in a check is V2213 with the close name as its fix:

- `sse`: `event` (default `message`), `data`, `id`, `retry` (ms) and `text`.
- `websocket`: `type` (`text`, `binary` or `close`), `data`, `text`, `code` and `reason`.
- `data` is the parsed JSON when the text parses as JSON, else the text. `text` is always the raw text.
- A binary frame's `data` is base64 text.
- A server close shows up as one final `type: close` message, so `receive: { type: close, code: 1000 }` checks it.

What a call's checks and saves read (the `response` root):

- `response.messages` lists every message received, in order.
- `response.exchange[i]` follows the written list: for a `send` item, what was sent; for a `receive` item, the message it matched; for a group, a list of the matched messages in written order.
- `status`, `headers`, `body` and `timing` describe the opening response: the SSE response head, or the WebSocket handshake.

`expect.messages`:

- It is a matcher on the whole `response.messages` list: `{ length: 3 }`, `{ every: … }`, `{ some: … }`, `{ none: … }`.
- Its D123 merge key is `messages`.
- It is checked after the exchange ends.

Ending and the time limit:

- The call's `timeout` (default `defaults.timeout`) limits the open per attempt, as it does for `http`. It then limits the whole exchange that follows.
- After the last item the client closes the stream. A WebSocket sends close 1000 and waits for the server's close within the remaining time. An SSE call drops the body.
- A `receive` that is still unmet when the stream ends or the limit passes fails the call with V5130. The report shows the unmet item and the messages received since the last match.

When the stream does not open:

- An SSE answer that is not 2xx, or a WebSocket handshake that is not 101, has no stream. The call keeps the response with its status, headers and body, and `expect.status: 401` can pass.
- In that case the first `receive` fails with V5131, which names the status.
- A 2xx SSE answer whose `content-type` is not `text/event-stream` is also V5131.

Defaults (`sse`):

- `accept: text/event-stream` and `cache-control: no-cache` are added only when no level sets them, as for GraphQL (D343).
- `method` defaults to `GET`.

Defaults (`websocket`):

- `method` and `body` are V2213, also on a test or step. The handshake is an HTTP/1.1 `GET`.
- The URL may use `ws`/`wss` or `http`/`https`.

Keys that keep their meaning: auth, `headers`, `query`, `http.retry`, `timeout`, `expect`, `save`, `saveOn`, `cleanup`, hooks and `deprecated`.

- `http.retry` retries only the open.
- `saveOn` and `cleanup` read the opening status.
- `poll` is V2213 on a stream type.

Size: each message, and the total kept for the call, is capped by `SendContext::body_limit`. Going over the cap is `BodyTooLarge` (V5105), as an over-large body is today.

`Last-Event-ID` and `retry`:

- Both are cheap without new syntax.
- `last-event-id` is a header like any other, so a resume test is two steps: the first saves `${response.exchange[0].id}`, the second sends it.
- `retry` is a message field that a check can read.
- Automatic reconnect is not in core (Q2).

## Architecture, by facet (D342)

| Facet | SSE | WebSocket |
| --- | --- | --- |
| Shape (`vc-model`) | `Protocol::Sse`, its key set: http's keys plus `exchange` and `expect.messages`, minus `send` | `Protocol::Websocket`: no `method` or `body`; `exchange` with `send` |
| Compile rules (`vc-compiler`) | `SseRules`: method as `http`, `body` as `http`, path required | `WebsocketRules`: method `GET`, no body, path required |
| Runtime (`vc-executor`) | `SseRuntime`: adds headers, decodes byte chunks into events with a pure incremental `text/event-stream` parser | `WebsocketRuntime`: encodes `send` values, maps frames to messages |
| Transport (`vc-adapter-http`) | streaming response body (hyper `Incoming`) | HTTP/1.1 upgrade on the same connector, framing from tokio-tungstenite (Q1) |
| Demo route (`bench/mock`) | `/events`: scripted events with ids, a heartbeat comment, `retry`, and honours `last-event-id` | `/ws`: echo; JSON `join`/`say` replies; closes on `bye` |

### The port, in `vc-interfaces`

`Transport::send` stays as it is. A second port carries streams:

```rust
#[async_trait]
pub trait StreamTransport: Send + Sync {
    async fn open(&self, request: RenderedRequest, mode: StreamMode, ctx: &SendContext)
        -> Result<Opened, TransportError>;
}
pub enum StreamMode { Body, WebSocket }
pub struct Opened { pub head: Exchange, pub stream: Option<Box<dyn MessageStream>> }
#[async_trait]
pub trait MessageStream: Send {
    async fn send(&mut self, frame: Frame) -> Result<(), TransportError>;
    async fn next(&mut self) -> Option<Result<Frame, TransportError>>;
    async fn close(&mut self, code: u16) -> Result<(), TransportError>;
}
pub enum Frame { Chunk(Bytes), Text(String), Binary(Bytes), Close { code: u16, reason: String } }
```

How the port is used:

- `head` is the opening response. When there is no stream, its body is read in full, capped as today.
- The adapter answers pings itself. Ping and pong frames are not surfaced.
- `vc-testkit` gets a scripted fake and a `stream_suite` that every implementation runs (D112).
- `HttpTransport` implements both ports, so SSE and WebSocket reuse its TLS, mTLS identities, proxy, host rules and connect timing.

**No new crate and no arch-tests row**: adapters never import each other, so a separate `vc-adapter-ws` would have to copy all of that configuration. `EngineBuilder` hands the same `HttpTransport` to the executor as both `Arc<dyn Transport>` and `Arc<dyn StreamTransport>`.

### The executor

- `ProtocolRuntime` gains one default method, `stream(&self) -> Option<&dyn StreamRuntime>`. It returns `None` for `http` and `graphql`, so the call path picks the stream driver through the registry, not with a `match`.
- `StreamRuntime` has three methods: `mode`, `decoder` (a stateful `push(Frame) -> Vec<Message>`) and `encode(&Value) -> Frame`.
- `ExchangeCursor` (`action/exchange.rs`) is pure and does the matching. `feed(message)` reports which item or group entry matched and whether the exchange is done; `next_send()` returns the next frame to send. Group pairing reuses the one-to-one pairing behind D362.
- The driver (`action/stream.rs`) is the only async part.
  - It opens through the request pipeline (timing, retry, auth, hooks, per-host limit, timeout). If a layer is typed to `Exchange`, it is made generic over a small `HasHead` trait in the same PR.
  - It sends and reads against the call's deadline.
  - It feeds the cursor and builds `response.messages` and `response.exchange`.
- The per-host permit is held for the life of the connection.
- `vc-model`'s `Exchange` gains `messages: Vec<StreamMessage>`, empty for `http` and `graphql`. It flows to history and reports through `RedactingSink` like bodies do.

## Proposed decisions (Round 79, candidates)

| D | Decision |
| --- | --- |
| D376 | Stream types: `type: sse` keeps every `http` request key and reads the opening response as `http`. `type: websocket` drops `method` and `body` (V2213), handshakes with an HTTP/1.1 `GET`, and takes a `ws`, `wss`, `http` or `https` URL. `poll` is V2213 on both. |
| D377 | `exchange`: an ordered list of `send` and `receive` items over one connection, written on the api, test or step. The most specific replaces the others whole, as for `body`. `send` is `websocket` only. Sends are rendered before the open. |
| D378 | `receive`: one check waits for the next matching message and skips non-matching ones. A list is an any-order group paired one-to-one (as D362). Checks use the existing matchers on the message's fields. |
| D379 | Message shape. `sse` has `event` (default `message`), `data`, `id`, `retry` and `text`. `websocket` has `type`, `data`, `text`, `code` and `reason`, and a server close is a final `type: close` message. `data` is JSON when it parses, else text; binary data is base64. An unknown field is V2213. |
| D380 | `response.messages` holds every message in order. `response.exchange[i]` follows the written list. `expect.messages` is a list matcher, merged under the key `messages` (D123). `status`, `headers`, `body` and `timing` describe the opening response. |
| D381 | Ending: after the last item the client closes (WebSocket close 1000). With no `receive` item it reads until the server closes. `timeout` limits the open per attempt, then the exchange. An unmet `receive` is V5130; no stream, or the wrong content type, is V5131. |
| D382 | Transport: a `StreamTransport` port (`open` returns the head and an optional `MessageStream` with `send`, `next` and `close`), implemented by `vc-adapter-http` for both modes. There is no new adapter crate. The SSE parser is pure, in the executor's runtime. |
| D383 | The open goes through the request pipeline. `http.retry` retries only the open. The per-host permit is held for the connection. Each message, and the call's total, is capped by `body_limit`. |
| D384 | `sse` adds `accept: text/event-stream` and `cache-control: no-cache` only when unset. `Last-Event-ID` is a plain header, `retry` a readable field, and there is no automatic reconnect. |

Candidate V codes (V2213 and V5105 are reused):

- **V5130**: a `receive` was unmet when the stream ended or the time limit passed.
- **V5131**: the stream did not open: a non-2xx SSE answer, a handshake that is not 101, or a 2xx answer that is not `text/event-stream`.

## Later

- Load: virtual users holding connections, and message rates (`vc load`).
- Timing metrics: time to first message, gaps between messages, and a per-`receive` `within`.
- Sends that read messages from the same call, and request/response correlation by a field.
- Automatic SSE reconnect honouring `retry` and `Last-Event-ID` (unless Q2 says core).
- WebSocket subprotocols, binary `send`, ping/pong checks, and an explicit `close` item with a code.
- A "listen for a window" form (read for 2s, then check) for `none`-style checks on endless streams.
- `graphql` subscriptions (graphql-transport-ws over `websocket`; D343), gRPC streams, `jsonrpc` over WebSocket.
- Importers (OpenAPI `text/event-stream` responses, AsyncAPI), and streams in the browser playground. Until then the playground reports the call as unsupported.

## Open questions for Sarath

**Q1. WebSocket framing: which library, and where?**

- (a) **Recommended.** `tokio-tungstenite` (default features off, no TLS of its own) inside `vc-adapter-http`, over hyper's HTTP/1.1 upgrade. The mock uses axum's `ws` feature, which wraps the same crate. This is a new dependency. `cargo deny` will likely need skips for the second versions of `rand` and `sha1` it brings, in `deny.toml`, which is your file. There is no arch-tests row.
- (b) A new `vc-adapter-ws` crate that dials through `tokio-tungstenite`'s own connect. It needs a new arch-tests row and an `ENGINE` entry, and it duplicates the TLS, mTLS, proxy and host-rule setup, because adapters cannot share code.
- (c) Hand-written RFC 6455 client framing in `vc-adapter-http`, about 300 lines and no client dependency. The mock still needs a server side, so this means (a)'s dependency in `bench/mock` or a second hand-written framer.

**Q2. SSE reconnect: core or later?** Issue #92 lists `Last-Event-ID` reconnect and `retry`.

- (a) **Recommended.** Later. Resuming is tested with two steps and the `last-event-id` header, and `retry` is a field a check can read.
- (b) Core. One automatic reconnect when the stream drops, waiting `retry` and sending the last `id`, behind a new key `sse.reconnect: true`. This is about 150 more lines and one more D row.

## PR split (each under about 400 changed lines)

1. **Shape and rules.**
   - `Protocol::Sse` and `Protocol::Websocket` with their key sets.
   - `SseRules` and `WebsocketRules`.
   - The `exchange` shape on api, test and step; message fields per type; `expect.messages` as a check kind.
   - The V2213 cases, with tests.
   - syntax-reference "Api types" bullets, and D376–D380 added to the log once approved.
2. **Port and cursor.**
   - `StreamTransport`, `MessageStream` and `Frame` in `vc-interfaces`; `StreamMessage` and `Exchange::messages` in `vc-model`.
   - The `vc-testkit` fake and `stream_suite`.
   - The pure `ExchangeCursor` with unit tests.
   - D381 and D382 added to the log.
3. **The executor's stream call.**
   - `ProtocolRuntime::stream` and `StreamRuntime`.
   - `SseRuntime` with its pure parser.
   - The driver through the pipeline: the deadline, V5130 and V5131, and the `response.messages` and `response.exchange` roots.
   - Engine wiring tested against the fake.
4. **SSE on the wire.**
   - `HttpTransport` implements `StreamTransport` in `StreamMode::Body`.
   - The mock's `/events` route.
   - End-to-end tests and a site example.
   - Messages in the json, HTML and CTRF reports.
5. **WebSocket on the wire.**
   - The dependency from Q1 and the upgrade path in `HttpTransport`.
   - `WebsocketRuntime`.
   - The mock's `/ws` route, end-to-end tests and a site example.
   - D383 and D384 added to the log.
