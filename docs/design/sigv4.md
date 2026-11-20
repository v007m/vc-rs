# AWS SigV4: the `awsSigV4` auth type (#106)

Status: approved by Sarath on 20 November 2026, with Q1 (a) below. The decisions are Round 83 (D431–D443) and Round 84 (D444–D446) in `decision-log.md`.

## Goal and scope

Let a test call an AWS-signed api with no hook and no proxy: API Gateway with IAM auth, Amazon OpenSearch (managed `es` and serverless `aoss`), Lambda function URLs, and S3 or S3-compatible stores. It is one more auth type, an entry in the `AuthTypes` registry, written once in `vibranium.config.yaml` and used by ID like every other.

Core scope:

- Sign each request with AWS Signature Version 4 (`AWS4-HMAC-SHA256`) in the `authorization` header: region, service, access key ID, secret access key, and an optional session token.
- Hash the payload, or send `UNSIGNED-PAYLOAD` for S3-style uploads.
- Sign after every other part of the request is set, `beforeRequest` hooks included, and sign again on every attempt.
- Credentials come from vars and secrets, as for every other type.

Everything else is in "Later" below.

## Syntax by example

`vc check` reads these examples (PR 1 below), and the docs check checks them. No run signs with them until PR 3.

```yaml
# vibranium.config.yaml
vars:
  awsRegion: eu-west-1
  ordersApiId: a1b2c3d4e5
  orderId: "42"
auth:
  - id: ordersIam                       # API Gateway with IAM auth
    type: awsSigV4
    region: ${var.awsRegion}
    service: execute-api
    accessKeyId: ${secret.AWS_ACCESS_KEY_ID}
    secretAccessKey: ${secret.AWS_SECRET_ACCESS_KEY}
    sessionToken: ${secret.AWS_SESSION_TOKEN}   # optional; empty sends none
  - id: reportsBucket                   # S3: no path normalising, single encoding
    type: awsSigV4
    region: ${var.awsRegion}
    service: s3
    accessKeyId: ${secret.AWS_ACCESS_KEY_ID}
    secretAccessKey: ${secret.AWS_SECRET_ACCESS_KEY}
    payload: unsigned                   # sends UNSIGNED-PAYLOAD
```

```text
# .env.schema
AWS_ACCESS_KEY_ID=
AWS_SECRET_ACCESS_KEY=
AWS_SESSION_TOKEN=
```

With the default secrets provider (`provider: env`, D163), these three names are read from the process environment, which is where CI, `aws-vault exec` and `aws sso` exports put them. The project's `.env` fills in any the environment does not set. Use the definition by ID, as any other:

```yaml
# services/orders/orders.api.yaml
baseUrl: https://${var.ordersApiId}.execute-api.${var.awsRegion}.amazonaws.com/prod
auth: ordersIam
apis:
  - id: getOrder
    path: /orders/${var.orderId}
```

## Semantics

Keys:

| Key | Type | Default | Purpose |
| --- | --- | --- | --- |
| `region` | template | required | The signing region, such as `us-east-1` |
| `service` | template | required | The signing name, such as `execute-api`, `es`, `aoss`, `lambda` or `s3`. `s3`, `s3express`, `s3-outposts` and `s3-object-lambda` follow S3's path rules (D454) |
| `accessKeyId` | template | required | The access key ID |
| `secretAccessKey` | template | required | The secret access key |
| `sessionToken` | template | — | A temporary credential's token, sent as `x-amz-security-token` and signed. When it renders empty, no token is sent, so one definition serves both long-term and temporary keys |
| `payload` | `signed` or `unsigned` | `signed` | `signed` hashes the body as sent; `unsigned` signs the literal `UNSIGNED-PAYLOAD`, which S3 accepts and most other services refuse |

The shared keys keep their meaning: `vars`, `clientCert` (D350), `refreshBefore` and `refreshOn`. The credentials hold for the run, since they are only the rendered settings. `refreshOn` keeps its default of `[401]`, but AWS answers a bad signature with `403`, so in practice it does not fire.

Where credentials come from:

- Only from the definition's settings, rendered like every other auth setting from vars, `${secret.X}` and `${env.X}`.
- There is no implicit lookup of the AWS environment variables, `~/.aws/credentials`, SSO, `credential_process` or the instance metadata service.
- An implicit lookup would make a run depend on whatever machine it runs on, and could sign with the wrong account without a word. An explicit `${secret.AWS_SECRET_ACCESS_KEY}` already reads the standard environment variable through the secrets adapter, and keeps it masked.
- A secrets provider that reads a named profile from the AWS shared files is in "Later". It would plug in behind the same `${secret.X}` names, with nothing changed here.

When signing happens (D433, D443, amending D141):

- The pipeline becomes timing → retry (`refreshOn`, `http.retry`) → auth → `beforeRequest` hooks → per-host limit → **sign** → timeout (per attempt) → transport.
- The sign stage is a null object for every other auth type.
- It runs after the per-host limit, so a request that waited for a permit still carries a fresh `x-amz-date`. AWS refuses a date too far from its own clock (5 to 15 minutes, by service).
- It runs inside retry, so each attempt is signed again with its own date. A `beforeRequest` hook that sets a header, changes the body or changes the URL is signed with it.
- The time is the injected `Clock`'s `now_ms`, in UTC. Tests use a fixed or stepping clock.
- `sse` and `websocket` opens, and `graphql` calls, go through the same pipeline, so they are signed the same way. A signed WebSocket handshake is what API Gateway's WebSocket `$connect` with IAM expects.

What the signer sets on the request:

- `x-amz-date`: `YYYYMMDD'T'HHMMSS'Z'`.
- `x-amz-content-sha256`: the payload hash, or `UNSIGNED-PAYLOAD`. It is sent for every service, not only S3. S3 and OpenSearch Serverless require it, and the other services accept it.
- `x-amz-security-token`, when there is a session token.
- `authorization`: `AWS4-HMAC-SHA256 Credential=<accessKeyId>/<yyyymmdd>/<region>/<service>/aws4_request, SignedHeaders=<names>, Signature=<hex>`.
- Each of these four replaces a header of that name that the request already has, from any level.

The canonical request (D435):

- **Method**: as sent.
- **Path, every service but S3**:
  - The path as sent, `/` when empty.
  - Dot segments are removed and runs of `/` collapse to one (RFC 3986 §5.2.4, as AWS normalises).
  - Every byte outside `A-Za-z0-9-._~/` is then percent-encoded with uppercase hex. The path as sent is already encoded once (D278), so this is AWS's double encoding: a `%20` on the wire is `%2520` in the canonical path.
  - The path sent is not changed.
- **Path, S3** (`service` is `s3`, `s3express`, `s3-outposts` or `s3-object-lambda`, D436, D454):
  - No normalising, since an object key may hold `//` or `..`, and single encoding.
  - The signer first rewrites the path it sends into AWS's encoding: each segment is percent-decoded, then every byte outside `A-Za-z0-9-._~` is encoded. A `%` not followed by two hex digits counts as a literal `%`.
  - The canonical path is that rewritten path, so what is signed is what is sent, whatever characters a key holds.
- **Query**:
  - Split on `&`, skipping empty items. Each item splits at its first `=`; an item with no `=` has an empty value.
  - A `+` is a space, as AWS reads a query, so it is signed as `%20`; a literal plus is written `%2B` (D453).
  - Names and values are percent-decoded, then encoded as above, with `/` encoded too.
  - Items are sorted by encoded name, then encoded value, and written `name=value` (`name=` when the value is empty), joined by `&`.
  - The query sent changes only in that each `+` is sent as `%20`, so the server reads what was signed. AWS decodes and re-encodes it the same way.
- **Headers**:
  - Every header on the request when it is signed, plus `host`, except a deny-list of headers that the transport or a proxy may add, drop or change: `authorization`, `connection`, `content-length`, `expect`, `keep-alive`, `proxy-authorization`, `te`, `transfer-encoding`, `upgrade`, `user-agent` and `x-amzn-trace-id`.
  - Names are lowercased.
  - Values are trimmed, and each run of spaces or tabs collapses to one space. A value is used as its bytes, so a value that is not UTF-8 still signs.
  - A repeated header's values are joined with `,`, in order.
  - Sorted by name. Each is written `name:value\n`, and `SignedHeaders` is the names joined with `;`.
- **Payload**: the lowercase hex SHA-256 of the body as sent (the empty string's hash when there is none), or `UNSIGNED-PAYLOAD`.

The rest follows SigV4 as published:

- The string to sign is `AWS4-HMAC-SHA256`, the date, the scope (`yyyymmdd/region/service/aws4_request`) and the hex SHA-256 of the canonical request, one per line.
- The signing key is the HMAC-SHA256 chain `"AWS4" + secret` → date → region → service → `aws4_request`. It is derived again for each signature and never cached, so nothing shared is mutable. Four HMACs cost microseconds, even under `vc load`.

`host` is signed as it is sent (D438):

- The transport adds `host` (HTTP/1.1) or `:authority` (HTTP/2) on send, after signing, so the signer and the transport must derive it the same way.
- One pure function in `vc-model`, `wire_host(&Uri)`, gives it: the host, lowercased, then `:port` only when the port is not the scheme's default. Both use it.
- The transport suite in `vc-testkit` (D112) pins it against a server that echoes what it received, over HTTP/1.1 and HTTP/2, with and without a port.
- A `host` header the request already sets is signed as written and sent unchanged; the suite pins that too.

Validation:

- `vc check` reports a missing `region`, `service`, `accessKeyId` or `secretAccessKey` as V1005 at `type`, as for the other types.
- A `payload` that is not `signed` or `unsigned` is V2302, as for `apiKey`'s `in`.
- At run time, the rendered settings are checked before anything is signed. The failures are V5114, with a cause that names the key and never a secret's value:
  - an empty `accessKeyId` or `secretAccessKey`;
  - an `accessKeyId` holding `/` or whitespace, which would break the scope;
  - a `region` or `service` that renders to anything but lowercase letters, digits and `-`. A rendered region or service is not a secret, so its value can be named.

## Architecture

| Part | Where | What |
| --- | --- | --- |
| Shape and rules | `vc-compiler` | the six keys in the `auth` shape; `awsSigV4` in `AUTH_TYPES`; required keys; lints V2316 and V4014 |
| Port | `vc-interfaces` | `RequestSigner`, `Signer`, `Credentials::signer`, `AuthProvider::signs` |
| Signer | `vc-adapter-auth`, `sigv4/` | the registry entry, the settings check, the pure canonical request, and the signature |
| Sign stage | `vc-executor`, `action/auth/sign.rs` | `SignStage` in the pipeline, the clock, and errors as V5114 |
| Host | `vc-model` | `wire_host`, used by the signer and by `vc-adapter-http` |
| Demo route | `bench/mock` | `/aws/*`: verifies SigV4 with the suite's example key |

### The port, in `vc-interfaces`

```rust
/// Signs a request as it leaves, when only the whole request can make the credential.
pub trait RequestSigner: Send + Sync {
    /// `request` signed as at `now_ms` (UTC, milliseconds since the Unix epoch).
    fn sign(&self, request: RenderedRequest, now_ms: u64) -> Result<RenderedRequest, AuthError>;
}
/// A signer and the values it holds that the redactor must hide. `Debug` prints neither;
/// two are equal when they share one signer (`Arc::ptr_eq`), so `Credentials` keeps its derives.
#[derive(Clone)]
pub struct Signer { signer: Arc<dyn RequestSigner>, hidden: Vec<Secret> }
pub struct Credentials { pub headers, pub query, pub cookies, pub expires_in, pub signer: Option<Signer> }
pub trait AuthProvider: Send + Sync {
    async fn credentials(&self, request: &AuthRequest) -> Result<Credentials, AuthError>;
    /// Whether this request's credentials sign the request (`AuthTypes` asks its entry).
    fn signs(&self, request: &AuthRequest) -> bool { false }
}
```

How the parts fit:

- `Credentials::values` also yields the signer's hidden values. The `AuthStage` already hands those values to the redactor when credentials arrive, so it needs no change.
- `AuthProvider::signs` is answered by the registry entry. The executor asks it once per call, in `auth::added`, and installs a `SignStage` only when it is `true`. No string `match` on the kind appears, and other auth types pay nothing.
- `SignStage` holds the call's `AuthStage`, shared through an `Arc`, and the injected `Clock`. For each attempt it reads the credentials through the same `TokenCache` slot and `Lease`, a cache hit, then calls `sign`. A `RefreshOn` invalidation therefore reaches it too.
- `PipelineParts` gains `sign: Arc<dyn RequestStage>`, with `Unchanged` by default. The type becomes `Timing<Retry<Stage<Stage<HostLimit<Stage<Timeout<Transmit>>>>>>>`.
- The `awsSigV4` entry adds no headers, query values or cookies. It returns a `Signer` holding `SigV4 { region, service, key_id, secret, token, payload, s3 }`.
  - `secret` and `token` are `Secret`s, zeroized on drop.
  - `SigV4`'s `Debug` prints only the region and service.
  - Nothing in it is `Serialize`.
- `RequestSigner` stays synchronous and pure, and the crypto stays in the adapter: the executor gains no dependency.

## Redaction (D439)

- `accessKeyId`, `secretAccessKey` and `sessionToken` go to the redactor when the credentials are made, through `Signer`'s hidden values. They are masked in every event, report, history entry and error, wherever they appear.
- `authorization` holds the signature. It is masked by name, as always, and its value is marked sensitive on the request. The signature is not added to the masked values: it differs for every request, so the list would grow without bound under `vc load`.
- `x-amz-security-token` carries a masked value.
- The derived keys live in `Zeroizing` buffers for the length of one `sign` call.
- The canonical request and the string to sign are never logged, reported or put in an error.
- V4014's excerpt masks the value, as V4013's does.

## Decisions (Rounds 83 and 84)

| D | Decision |
| --- | --- |
| D431 | `type: awsSigV4` with `region`, `service`, `accessKeyId`, `secretAccessKey` (all required templates), `sessionToken` (optional; when it renders empty, none is sent) and `payload: signed \| unsigned` (default `signed`). It signs with `AWS4-HMAC-SHA256` in the `authorization` header, and adds no query values or cookies. |
| D432 | Credentials come only from the definition's settings (vars, `${secret.X}`, `${env.X}`). There is no ambient AWS credential chain. The standard `AWS_*` variables reach it through `.env.schema` and the `env` secrets provider. A profile-reading secrets provider is Later. |
| D433 | The pipeline gains a sign stage after the per-host limit and before the per-attempt timeout: timing → retry → auth → `beforeRequest` hooks → per-host limit → sign → timeout → transport. Inside retry, every attempt is signed again with its own `x-amz-date` from the injected `Clock`. It is a null object for types that do not sign. Amends D141. |
| D434 | The port: `RequestSigner` (synchronous, pure: `sign(request, now_ms)`) in `vc-interfaces`; `Credentials::signer: Option<Signer>`, a signer plus the values to hide; `AuthProvider::signs`, default `false`, answered by the registry entry. The executor's `SignStage` reads the signer through the call's `AuthStage` (same cache slot and lease) and is installed only when `signs` is true. |
| D435 | The canonical request: the method as sent; the path normalised (dot segments, repeated `/`) and encoded again, outside `A-Za-z0-9-._~/` (double encoding); the query decoded, re-encoded with `/` encoded, sorted by name then value, `name=` for no value; the headers on the request plus `host`, less a deny-list (`authorization`, `connection`, `content-length`, `expect`, `keep-alive`, `proxy-authorization`, `te`, `transfer-encoding`, `upgrade`, `user-agent`, `x-amzn-trace-id`), lowercased, trimmed, spaces collapsed, repeats joined by `,`, sorted. The path and query sent are not changed, except for S3 (D436). |
| D436 | `service: s3` follows S3's rules: no path normalising and single encoding. The signer rewrites the path it sends into AWS's encoding (each segment decoded, then encoded), and that path is the canonical one. No other service name changes the rules. |
| D437 | The signer sets `x-amz-date`, `x-amz-content-sha256` (for every service: the body's SHA-256, or `UNSIGNED-PAYLOAD`), `x-amz-security-token` when there is a token, and `authorization`, each replacing any value already set. The signing key is derived for each signature and never cached. |
| D438 | `host` is signed as sent. `vc_model::wire_host(&Uri)` (the host lowercased, `:port` only when not the scheme's default) is used by the signer and the HTTP transport. The `vc-testkit` transport suite pins it over HTTP/1.1 and HTTP/2. A `host` the request sets is signed and sent as written. |
| D439 | Redaction: the key ID, secret and token are masked values from the moment the credentials are made. `authorization` is masked by name and marked sensitive. The signature is not a masked value. Derived keys are zeroized after each signature. The canonical request and string to sign never leave the signer. |
| D440 | Diagnostics: V1005 (a missing required key) and V2302 (`payload`) are reused. V2316 is new: an error for a literal `region` or `service` that is not lowercase letters, digits and `-`, with lowercasing as a certain fix when that makes it valid. V4014 is new: a warning for a literal `secretAccessKey` or `sessionToken`, with the value masked in the excerpt. At run time, unusable settings are V5114. Signing itself cannot fail, so there is no new V5 code. |
| D441 | Tests: the AWS SigV4 test-suite vectors (Apache-2.0, from `awslabs/aws-c-auth`) are vendored unchanged with their licence and source commit. They drive the pure signer, and the vectors it cannot represent are listed as skipped, with reasons. The mock verifies signatures independently at `/aws/*`. The transport suite pins `host`. |
| D442 | Dependencies: the signer is hand-rolled on `hmac` 0.13 and `sha2` 0.11, already workspace dependencies (`vc-adapter-hooks` uses both), plus `percent-encoding` and `zeroize`, also already there. The `aws-sigv4` crate is not used: it brings `aws-smithy-*`, `aws-credential-types` and their dependencies for about 300 lines of rules that the AWS vectors pin. No `hex` crate either: hex encoding is a five-line function. |
| D443 | D141 amended: timing → retry → auth → `beforeRequest` hooks → per-host limit → sign → timeout → transport. |
| D444 | `vc new auth` writes `accessKeyId`, `secretAccessKey` and `sessionToken` only as templates; `region` and `service` are the author's (V1005). |
| D445 | An empty literal `region` or `service` is V2316 too, with no fix; the fix quotes only where YAML needs it. |
| D446 | A literal `secretAccessKey` or `sessionToken` is V4014 and masked in any auth, whatever its `type`; a config that does not parse masks every value of those keys. The access key ID is not a secret at check time. |
| D447 | Unusable rendered settings are V5114 (`AuthError::Missing`); the key names the rule broken, never the value. An `accessKeyId` may not hold spaces, `/`, `,` or `=`; a non-text `sessionToken` or `payload` breaks its rule. |
| D448 | `authorization` and `x-amz-security-token` are sensitive on the request; `x-amz-date` and `x-amz-content-sha256` are not. |
| D449 | The vector parser unfolds continued headers, encodes raw request-line bytes once, and keeps `x-amz-content-sha256` only for `sign_body`. |
| D450 | 33 of 38 vectors run; five are skipped with reasons. The presigned (query) files are not read. |
| D451 | Zeroized: the secret, token, seed, derived keys and hash states. Not the canonical request, string to sign or signature. |
| D452 | `wire_host` lands in PR 2; PR 3 wires the transport. A time past year 9999 signs as the epoch. |
| D453 | A `+` in the query is a space: sent and signed as `%20`; a literal plus is `%2B`. Supersedes D435 in part (Sarath, 2026-11-21). |
| D454 | `s3`, `s3express`, `s3-outposts` and `s3-object-lambda` follow S3's path rules. Supersedes D436 in part (Sarath, 2026-11-21). |

Candidate V codes (V1005, V2302 and V5114 are reused; V1 and V3 need nothing new):

- **V2316** (error): a literal `region` or `service` that is not lowercase letters, digits and `-` (`US-East-1`, `execute api`). The certain fix lowercases it when that is enough. A value with a `${…}` is checked when it renders (V5114).
- **V4014** (warning): `secretAccessKey` or `sessionToken` written as text in the config. The help says to read it with `${secret.NAME}` declared in `.env.schema`. The value is masked in this and every other excerpt.

## Test plan

The pure signer (`vc-adapter-auth`, PR 2):

- The AWS SigV4 test suite is copied byte for byte into `crates/adapters/auth/tests/sigv4/`, from `awslabs/aws-c-auth` (`tests/aws-signing-test-suite/v4`, Apache-2.0). Its licence file and a `SOURCE` file naming the commit go beside it.
  - `.gitattributes` marks the folder `-text`, so its bytes stay as AWS ships them on every OS. Any copy script opens files with `newline=""`.
- One test walks every vector.
  - It parses `request.txt` with a small test-only parser into a `RenderedRequest`.
  - It picks S3 or non-S3 rules from the vector's context: normalising and double encoding, or neither.
  - It signs with the vector's credentials and time, leaving `x-amz-content-sha256` out unless the case sets `sign_body`, since the vectors predate it.
  - It compares the canonical request, the string to sign, the signature and the `Authorization` line with the vector's files.
  - A header continued on the next line is unfolded with one space, and raw bytes in the request line are encoded once, as a rendered URL holds them (D449).
- The five cases that cannot run are listed in the test with their reason, and a second test checks each names a vendored case (D450): two sign a raw path a rendered URL cannot hold, one adds the token after signing, and two sign `content-length`. 33 cases run.
- Unit tests beyond the suite cover:
  - the S3 path rewrite (`a b/c+d` and a stray `%`), for each of the four S3 names, and a near name (`s3x`) that normalises;
  - a `+` in the query sent and signed as `%20`, and `%2B` kept;
  - the deny-list;
  - a header that is not UTF-8;
  - an empty `sessionToken`;
  - `UNSIGNED-PAYLOAD`;
  - replacement of an `authorization` or `x-amz-date` that the request already has;
  - `Debug` of `SigV4` and `Signer` showing no secret.

The port and stage (`vc-executor`, PR 3), against the fake transport:

- With a stepping clock, two attempts carry different `x-amz-date` values and different signatures.
- A header that a `beforeRequest` hook adds appears in `SignedHeaders`.
- A signer error is V5114.
- Other auth types get the null stage. A test asserts this with a provider whose `signs` is false.
- The key ID, secret and token are masked in events, and `authorization` is masked.
- `wire_host` has a unit test in `vc-model`, and its `vc-testkit` transport-suite case runs for `HttpTransport`.

Wire and end to end (PR 4):

- `bench/mock` gains `/aws/*`. It verifies SigV4 with its own small verifier, using the suite's example key ID and secret.
  - Its unit tests run two suite vectors, so the oracle is checked too.
  - It refuses a date more than 5 minutes off, the strictest window AWS uses.
  - It answers 200 with the signed header names, or 403 with an AWS-style `SignatureDoesNotMatch` body and its canonical request. That is safe because the key is the published example key.
  - Its `x-amz-content-sha256` check follows the header as S3 does.
- The verifier is a second implementation, because `vc-mock-server` depends on no `vc-*` crate (arch-tests). It adds `hmac` and `sha2` to the mock.
- End-to-end tests in `vc-cli`:
  - `vc call` and `vc run` pass against `/aws/*` for `execute-api` style and `s3` style, with signed and unsigned payloads, a session token, a query that needs sorting and encoding, and a hook header.
  - A run with `http.retry` against a route that fails once passes, and the attempts are signed separately.
  - The secret, token and signature appear in no report, history entry or output.
  - `vc check` reports V2316 and V4014.
- A site page gets examples that the docs check runs.

## Later

- Presigned URLs (signing in the query, with `X-Amz-Expires`).
- SigV4a (ECDSA, multi-region), and the `STREAMING-AWS4-HMAC-SHA256-PAYLOAD` chunked upload.
- An `aws` secrets provider that reads a named profile from `~/.aws/credentials` and `~/.aws/config`; then SSO, `credential_process`, the instance metadata service, and `AssumeRole` through STS.
- A way to see the canonical request and string to sign, with secrets masked, when a 403 says the signature does not match.
- A fixed `x-amz-date`, or a clock offset, for negative tests such as an expired signature.
- Choosing the signed headers, or leaving `x-amz-content-sha256` out, if a service turns out to refuse it.
- Importers: OpenAPI's `x-amazon-apigateway-authtype: awsSigv4` becoming an `awsSigV4` definition.

## Questions Sarath answered (20 November 2026)

**Q1. Where the request is signed.** D141 fixes the pipeline order, and signing "after every header is set" (issue #106, D134) needs a step after the `beforeRequest` hooks, which D141 puts after auth.

- (a) **Chosen.** A new sign stage after the per-host limit, before the per-attempt timeout (D433, D443, amending D141). Hook changes are signed, and the date is fresh even after a wait for a permit. It is a null object for the other types.
- (b) A new sign stage right after the hooks, before the per-host limit. The same, except that a request that waits long for a permit carries an older date. AWS allows 5 to 15 minutes.
- (c) No new stage: sign inside the auth stage, before the hooks. D141 is unchanged, but a hook that touches a signed header, the body or the URL breaks the signature, and the issue's "after every other header" is not met.

The rest was approved as written: the credential source (D432), the S3 rules (D436), the hand-rolled signer (D442), V2316 and V4014.

## PR split (each under about 400 changed lines of code; the vendored vectors are data)

1. **Shape and rules.**
   - The six keys in the `auth` shape, and `awsSigV4` in `AUTH_TYPES`.
   - The V1005 required keys, and `payload` as an enum (V2302).
   - The V2316 and V4014 lints, with tests.
   - The syntax-reference auth row and section, and the LLD diagnostic rows.
   - Until PR 3, a run fails the call with V5114 "unknown auth type".
2. **The pure signer.**
   - `vc-adapter-auth` `sigv4/`: the settings check, the canonical request, the signing key and the signature, with no wiring.
   - The vendored vectors with the `.gitattributes` line, the vector test, and the unit tests.
   - `hmac` and `sha2` added to the crate.
   - `wire_host` in `vc-model`, with its unit test (D452).
3. **The port and the sign stage.**
   - `RequestSigner`, `Signer`, `Credentials::signer` and `AuthProvider::signs`.
   - `vc-adapter-http` sends the `host` from `wire_host`, and its transport-suite case.
   - `PipelineParts::sign`, `SignStage` and the `auth::added` wiring, with executor tests.
   - The `awsSigV4` registry entry.
   - The `mise run public-api` snapshot, and the LLD auth and pipeline text.
4. **Wire and end to end.**
   - The mock's `/aws/*` verifier with its tests, and the e2e tests above.
   - The redaction checks, and the site page.
