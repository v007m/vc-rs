# Streams: SSE and WebSocket (#92, #93)

Status: approved by Sarath on 20 November 2026, with Q1 (a) and Q2 (b) below. The decisions are Round 79 (D376–D385) in `decision-log.md`.

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
```

```yaml
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
- A check is a map over the message's fields, matched field by field as an `expect.errors` entry is (D360): each written field is a literal or a matcher, and unwritten fields are ignored.
- `data` takes `match` (paths into the JSON, each a matcher, as `body.match`) or `equals` (the whole value, as `body.equals`), exactly one of them (V2306 for neither or both, D419). Any other field takes a literal or a matcher.
- A `receive` list with no check is V2306 (D419).

Message shape. The shape is per type, and a field the type's messages lack is V1004 with the close name as its fix:

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
- A 2xx SSE answer whose `content-type` is not `text/event-stream` is also V5131. With no `receive` written, V5131 is at the step, and only when the head itself succeeded, since a failed head already fails its status check (D422).

Defaults (`sse`):

- `accept: text/event-stream` and `cache-control: no-cache` are added only when no level sets them, as for GraphQL (D343).
- `method` defaults to `GET`.

Defaults (`websocket`):

- `method` and `body` are V2213, also on a test or step. The handshake is an HTTP/1.1 `GET`.
- The URL may use `ws`/`wss` or `http`/`https`.

Keys that keep their meaning: auth, `headers`, `query`, `http.retry`, `timeout`, `expect`, `save`, `saveOn`, `cleanup`, hooks and `deprecated`.

- `http.retry` retries only the open.
- `saveOn` and `cleanup` read the opening status. A `websocket` call succeeds on `101`, not `2xx`: with no status written, its `expect` checks `status: 101`, its `saveOn` defaults to `101` and only a `101` registers its `cleanup` (`Protocol::success_status`, D386), whatever its `expect.status` says (D387). An `sse` call keeps `2xx`.
- `poll` is V2213 on a stream type.

Size: the bytes of every frame the call reads, added up, are capped by `SendContext::body_limit` (64 MiB). Going over the cap fails the call with V5105, as an over-large body does (D424).

A `send` still going out when the time runs out fails with V5104, the code of an `http` timeout; a `send` the transport did not confirm leaves its `response.exchange` item `null`. A failure after the stream opened (the size cap, a stuck `send`, a failed reconnect) is a failed `messages` check at the step, under the failure's code: the report keeps the messages read before it, and the opening status still registers the `cleanup`. Only a failed open or `afterResponse` hook halts the call. Only an open stream holds its per-host permit; an answer that opened none lets it go at once, and a finished exchange lets it go before the hooks run (D425).

A dropped connection ends the read as a server close does: the messages so far are kept, and an unmet `receive` is V5130, naming the drop (D424). `afterResponse` hooks run once, after the exchange, and see the full `response` root, `messages` and `exchange` included (D423).

Reconnect (`sse` only):

- `sse.reconnect: true` on the api, else on its api file, turns it on. It is off by default.
- When the stream ends, by a server close or a dropped connection, before the exchange is done, the call opens it again once. It waits the last `retry` the server sent, else 1s, then sends `last-event-id` with the last `id` received, over any written one. With no `id` received, it sends the written header, if any.
- The second open goes through the pipeline as the first did, so auth is fresh. Messages keep adding to `response.messages`, and the exchange carries on where it was.
- The wait and the second open count against the call's `timeout`; when the wait would pass it, there is no reconnect. A second end is final, and an unmet `receive` is then V5130.
- A reconnect that answers with no event stream ends the read too: an unmet `receive` is V5130, naming the answer. One that fails as a first open would, by a transport error or a hook, is a failed `messages` check under that error, and one that does not answer within the time limit leaves an unmet `receive` V5130 naming the limit (D425).
- Without `sse.reconnect`, `last-event-id` is a header like any other, so a resume test can also be two steps: the first saves `${response.exchange[0].id}`, the second sends it. `retry` is a message field a check can read.

## Architecture, by facet (D342)

| Facet | SSE | WebSocket |
| --- | --- | --- |
| Shape (`vc-model`) | `Protocol::Sse`, its key set: http's keys plus `exchange` and `expect.messages`, minus `send` | `Protocol::Websocket`: no `method` or `body`; `exchange` with `send` |
| Compile rules (`vc-compiler`) | `SseRules`: method as `http`, `body` as `http`, path required | `WebsocketRules`: method `GET`, no body, path required |
| Runtime (`vc-executor`) | `SseRuntime`: adds headers, decodes byte chunks into events with a pure incremental `text/event-stream` parser | `WebsocketRuntime`: encodes `send` values, maps frames to messages |
| Transport (`vc-adapter-http`) | streaming response body (hyper `Incoming`) | HTTP/1.1 upgrade on the same connector, framing from tokio-tungstenite (D382) |
| Demo route (`bench/mock`, and capped in `vc-demo`) | `/events` (D426): `id`/`data` events that resume after `last-event-id`, and on request a `retry` field, heartbeat comments, a dropped connection (`drop`, or `dropAt` for a reconnect) or a held one (`hold`, mock only) | `/ws`: echo; JSON `join`/`say` replies; closes on `bye` |

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
    async fn send(&mut self, frame: OutgoingFrame) -> Result<(), TransportError>;
    async fn next(&mut self) -> Option<Result<StreamFrame, TransportError>>;
    async fn close(&mut self, code: u16) -> Result<(), TransportError>;
}
pub enum OutgoingFrame { Text(String), Binary(Bytes) }  // close only through `close`
pub enum StreamFrame { Chunk(Bytes), Text(String), Binary(Bytes), Close { code: u16, reason: String } }
```

How the port is used:

- `head` is the opening response. When there is no stream, its body is read in full, capped as today.
- The adapter answers pings itself. Ping and pong frames are not surfaced.
- A stream opens only on its mode's success head, `StreamMode::opens_on`; any other answer is refused, an `Opened` with no stream (D388). `send` on a body stream is `InvalidRequest`, and `send` after `close`, the server's `Close` or an `Err` is an error that sends nothing. `next` is never `None` before the stream ends: while it is open and nothing arrives, it waits as a silent server would, and the caller's timeout ends it. The fake's echo stream with nothing queued waits the same way. An `Err` from `next` ends the stream, and nothing follows a `Close` frame or an `Err`: every later `next` gives `None`, as it does after `close`.
- The frame type is `StreamFrame`, since the executor already has a `Frame` (its run frame). `send` takes an `OutgoingFrame`, text or binary only, so `close(code)` is the only way to close.
- `vc-testkit` gets `FakeStreamTransport` (scripted opens, and `contract()`) and two suites every implementation runs (D112): `stream_transport_suite` and `message_stream_suite`, which the first runs on an open WebSocket. `arch-tests` names a suite after its trait, so there is no `stream_suite`.
- `HttpTransport` implements both ports, so SSE and WebSocket reuse its TLS, mTLS identities, proxy, host rules and connect timing.

**No new crate and no arch-tests row**: adapters never import each other, so a separate `vc-adapter-ws` would have to copy all of that configuration. `EngineBuilder` hands the same `HttpTransport` to the executor as both `Arc<dyn Transport>` and `Arc<dyn StreamTransport>`.

### The executor

- `ProtocolRuntime` gains one default method, `stream(&self) -> Option<&dyn StreamRuntime>`. It returns `None` for `http` and `graphql`, so the call path picks the stream driver through the registry, not with a `match`.
- `StreamRuntime` (`action/stream.rs`) has four methods: `mode`, `decoder`, `encode(&Value) -> OutgoingFrame` (text as is, anything else as JSON text) and `opened(&Exchange)`, which says why a head that opened is not one the type reads (V5131). A `Decoder` is per connection: `push(StreamFrame) -> Vec<StreamMessage>` and `resume()`, the last `id` and `retry` a reconnect uses (D421).
- `SseRuntime` (`action/sse.rs`) parses the event stream as the HTML standard reads it: lines end in CR, LF or CRLF, even across chunks; a leading BOM is dropped; `:` starts a comment; an `id` holding NUL is ignored and an empty one clears the last id; `retry` counts only when all digits; a block with no `data` dispatches nothing. `WebsocketRuntime` (`action/websocket.rs`) makes one message per frame (D421).
- `ExchangeCursor` (`action/exchange.rs`) is pure and keeps the place; the driver judges each message against the waiting group's checks. `feed(fits)` takes the entries of the waiting group the message fits and answers `Skipped`, `Held` or `Met` with each entry's message; `next_send()` returns the next `send` item; `unmet()` names what V5130 reports. Group pairing reuses the one-to-one `Pairing` behind D362 (now `pairing.rs`). The cursor keeps the group's `Pairing` between feeds: `Pairing::push` adds one message and pairs it with the first unpaired entry it fits, else runs one augmenting search, so it pairs as many entries as a full rebuild would (a proptest checks it). A message is kept only while an entry it fits holds fewer messages than the group has entries, so the pairing stays bounded on an endless stream.
- The driver (`action/stream_call.rs`) is the only async part.
  - It opens through the request pipeline (timing, retry, auth, hooks, per-host limit, timeout), built by `stream_pipeline` with an `OpenStream` service in place of `Transmit` (D420). Each layer is generic over its reply's `HasHead`: `head()` is the response head retry and auth refresh read, and `hold(permit)` keeps the per-host permit. An `Exchange` drops it at once; an `Opening` keeps it until the stream is dropped.
  - It sends and reads against the call's deadline.
  - It feeds the cursor and builds `response.messages` and `response.exchange`.
- `vc-model`'s `Exchange` gains `messages: Vec<StreamMessage>`, empty for `http` and `graphql`. It flows to history and reports through `RedactingSink` like bodies do.
- The engine hands the executor the stream port `EngineBuilder::stream_transport` names; without one, a stream call fails with "`sse` calls need a stream transport, and none is set" and sends nothing. PR 5 makes the HTTP adapter the default.

## Decisions (Round 79)

| D | Decision |
| --- | --- |
| D376 | Stream types: `type: sse` keeps every `http` request key and reads the opening response as `http`. `type: websocket` drops `method` and `body` (V2213), handshakes with an HTTP/1.1 `GET`, and takes a `ws`, `wss`, `http` or `https` URL. `poll` is V2213 on both. |
| D377 | `exchange`: an ordered list of `send` and `receive` items over one connection, written on the api, test or step. The most specific replaces the others whole, as for `body`. `send` is `websocket` only. Sends are rendered before the open. |
| D378 | `receive`: one check waits for the next matching message and skips non-matching ones. A list is an any-order group paired one-to-one (as D362). Checks use the existing matchers on the message's fields. |
| D379 | Message shape. `sse` has `event` (default `message`), `data`, `id`, `retry` and `text`. `websocket` has `type`, `data`, `text`, `code` and `reason`, and a server close is a final `type: close` message. `data` is JSON when it parses, else text; binary data is base64. A check matches field by field (D360); `data` takes `match` or `equals` as `body` does. A field the type lacks is V1004. |
| D380 | `response.messages` holds every message in order. `response.exchange[i]` follows the written list. `expect.messages` is a list matcher, merged under the key `messages` (D123). `status`, `headers`, `body` and `timing` describe the opening response. |
| D381 | Ending: after the last item the client closes (WebSocket close 1000). With no `receive` item it reads until the server closes. `timeout` limits the open per attempt, then the exchange. An unmet `receive` is V5130; no stream, or the wrong content type, is V5131. |
| D382 | Transport: a `StreamTransport` port (`open` returns the head and an optional `MessageStream` with `send`, `next` and `close`), implemented by `vc-adapter-http` for both modes: SSE over hyper's streaming body, WebSocket over hyper's HTTP/1.1 upgrade with `tokio-tungstenite` framing (default features off). There is no new adapter crate and no arch-tests row. The SSE parser is pure, in the executor's runtime. |
| D383 | The open goes through the request pipeline. `http.retry` retries only the open. The per-host permit is held for the connection. Each message, and the call's total, is capped by `body_limit`. |
| D384 | `sse` adds `accept: text/event-stream` and `cache-control: no-cache` only when unset. `retry` is a message field a check can read. |
| D385 | `sse.reconnect: true` (on the api, else its file; off by default): when the stream ends before the exchange is done, the call opens it once more after the server's last `retry` (else 1s), sending `last-event-id` with the last `id` received. It goes through the pipeline, counts against `timeout`, and a second end is final. |

Candidate V codes (V2213 and V5105 are reused):

- **V5130**: a `receive` was unmet when the stream ended or the time limit passed.
- **V5131**: the stream did not open: a non-2xx SSE answer, a handshake that is not 101, or a 2xx answer that is not `text/event-stream`.

## Later

- Load: virtual users holding connections, and message rates (`vc load`).
- Timing metrics: time to first message, gaps between messages, and a per-`receive` `within`.
- Sends that read messages from the same call, and request/response correlation by a field.
- More than one reconnect, and reconnect for `websocket`.
- WebSocket subprotocols, binary `send`, ping/pong checks, and an explicit `close` item with a code.
- A "listen for a window" form (read for 2s, then check) for `none`-style checks on endless streams.
- `graphql` subscriptions (graphql-transport-ws over `websocket`; D343), gRPC streams, `jsonrpc` over WebSocket.
- Importers (OpenAPI `text/event-stream` responses, AsyncAPI), and streams in the browser playground. Until then the playground reports the call as unsupported.

## Questions Sarath answered (20 November 2026)

**Q1. WebSocket framing: which library, and where?**

- (a) **Chosen.** `tokio-tungstenite` (default features off, no TLS of its own) inside `vc-adapter-http`, over hyper's HTTP/1.1 upgrade. The mock uses axum's `ws` feature, which wraps the same crate. This is a new dependency. `cargo deny` will likely need skips for the second versions of `rand` and `sha1` it brings, in `deny.toml`, which is Sarath's file, so the exact skip lines go to the orchestrator. There is no arch-tests row.
- (b) A new `vc-adapter-ws` crate that dials through `tokio-tungstenite`'s own connect. It needs a new arch-tests row and an `ENGINE` entry, and it duplicates the TLS, mTLS, proxy and host-rule setup, because adapters cannot share code.
- (c) Hand-written RFC 6455 client framing in `vc-adapter-http`, about 300 lines and no client dependency. The mock still needs a server side, so this means (a)'s dependency in `bench/mock` or a second hand-written framer.

**Q2. SSE reconnect: core or later?** Issue #92 lists `Last-Event-ID` reconnect and `retry`.

- (a) Later. Resuming is tested with two steps and the `last-event-id` header, and `retry` is a field a check can read.
- (b) **Chosen.** Core. One automatic reconnect when the stream drops, waiting `retry` and sending the last `id`, behind a new key `sse.reconnect: true`. This is about 150 more lines and one more D row, D385.

## PR split (each under about 400 changed lines of code)

1. **Types and rules.**
   - `Protocol::Sse` and `Protocol::Websocket` with their key sets, `SseRules` and `WebsocketRules`.
   - `sse.reconnect` on the api and its file.
   - The V2213 cases, with tests.
   - Executor runtimes that fail a stream call with a plain message until the stream call replaces them.
   - The syntax-reference "Api types" bullets.
2. **Port and cursor.**
   - `StreamTransport`, `MessageStream` and `StreamFrame` in `vc-interfaces`.
   - The `vc-testkit` fake, `stream_transport_suite` and `message_stream_suite`.
   - The pure `ExchangeCursor` with unit tests, and `Pairing` moved out of `check/errors.rs` to share.
3. **Exchange and message checks.**
   - The `exchange` shape on api, test and step, with test-level defaults for call steps (D124).
   - `receive` checks lowered field by field, `data.match` and `data.equals`, V1004 for a field the type lacks.
   - `expect.messages` as a check kind.
   - Done; its refinements are D389 and D414–D418 in the decision log.
4. **The executor's stream call.**
   - `StreamMessage` and `Exchange::messages` in `vc-model`.
   - `ProtocolRuntime::stream` and `StreamRuntime`.
   - `SseRuntime` with its pure parser, and `WebsocketRuntime` (moved here from PR 6, since it needs no wire).
   - The driver through the pipeline: the deadline, reconnect, V5130 and V5131, and the `response.messages` and `response.exchange` roots.
   - Engine wiring (`EngineBuilder::stream_transport`) tested against the fake.
   - Done; its refinements are D421–D425.
5. **SSE on the wire.**
   - `HttpTransport` implements `StreamTransport` in `StreamMode::Body`.
   - The mock's `/events` route (the suite's `count` and `last-event-id` contract), with a drop for reconnect; the adapter runs `stream_transport_suite` once PR 6 adds the WebSocket side.
   - End-to-end tests and a site example.
   - Messages in the json, HTML and CTRF reports.
6. **WebSocket on the wire.**
   - `tokio-tungstenite` and the upgrade path in `HttpTransport`.
   - The mock's `/ws` route, end-to-end tests and a site example.
