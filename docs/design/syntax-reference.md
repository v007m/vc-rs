# Vibranium v2 — Syntax Reference

Oct 23, 2026 · @Sarath

This is the complete syntax for Vibranium v2 definition files. Every concept has one way to write it:

- Apis say how to call an endpoint, including how to undo what a call creates.
- Tests `call` apis and `expect` results.
- Lifecycle blocks (`beforeRun`, `beforeAll`, `beforeEach`, `before` and their `after` partners) prepare data by calling apis, and where a block sits decides how long that data lives.
- Every value is a `var`.
- Profiles switch values per environment without duplicating logic.

YAML and JSON are equivalent; examples use YAML.

## Design rules

These rules govern every file; `vc check` enforces each one with a diagnostic.

1. **camelCase keys everywhere.** No kebab-case (`repeat-delay`) or snake\_case (`max_parallel_executors`). Request bodies are your API's data and are sent exactly as written.
2. **The file suffix decides the kind:** `vibranium.config.yaml`, `*.api.yaml`, `*.test.yaml`, `*.load.yaml`. There is no `kind:` key.
3. **Every item has an `id`, and may have a `name`.** `id` is the reference: camelCase letters and digits only (`createsMember`), no spaces, dots or dashes. `name` is free text for reports. A full ID is `<fileId>#<id>` (`users.crud#createsMember`), and that is how another file's item is referenced; inside the same file the short `id` works. IDs are unique within a file across every kind (tests, apis, actions, steps, datasets…), so one short `id` never names two things (D128, D138). File IDs are lowercase words joined by dots (`users.crud`), so a bare dotted word is always a file ID. A file's ID comes from its path (D139): the conventional root (`services/`, `load/`) and the kind suffix are dropped and the folders and file name are joined with dots, so `services/payments/refunds/partial.test.yaml` is `payments.refunds.partial` and `services/users/users.api.yaml` is `users` (an api file named like its folder collapses; D142). Other kinds never collapse: `services/orders/orders.test.yaml` is `orders.orders`, and the convention lint suggests a better name such as `crud`. The `id:` key is optional; when present it must match the path (V2007), and `vc check --fix` renames it with its references.
4. **Things with several properties are list items; plain name–value pairs are maps.** Apis, tests, profiles, auth definitions, generators and plugins are lists of items with `id`; steps and lifecycle actions are lists whose `id` is optional. Vars, headers, query and saved values are maps.
5. **One kind of value: vars.** Defaults, inputs to an api, environment values and values saved from a response are all vars, read as `${var.x}`. Values set closer to the call win (see Vars and profiles).
6. **Sections are plural, references are singular.** `vars:` is read as `${var.x}`, `secrets` as `${secret.X}`, `examples:` as `${example.x}`.
7. **One template syntax:** `${namespace.path}`, optionally followed by pipes that convert the value (`${var.count | number}`), plus generator calls with literal arguments (`${fake.lorem.words(5)}`). No other code inside templates.
8. **One check form.** `expect` looks the same on apis, tests, steps, actions, polling and load thresholds. A literal means "equals"; anything else is a matcher such as `lt: 500ms`.
9. **Units are written out.** Durations `250ms`, `2s`, `1m`; sizes `4kb`, `1mb` (binary: `kb` is 1024 bytes, `mb` 1024², `gb` 1024³; D146); rates `1%`. A bare number is never milliseconds or bytes: `vc check` reports V2301 with a certain fix that adds `ms` or `b`. The one exception is a bare `0` in a check or threshold (`size: {ne: 0}`, `connect: {eq: 0}`, `p95: {eq: 0}`), which is zero in any unit.
10. **One flexible `body`.** Nested YAML or pasted JSON is sent as JSON, a string as text, `$file` as file bytes, `$base64` as raw bytes, `$extend` as an existing object with your changes; the `content-type` header decides the encoding.
11. **Logic lives in two places only:** CEL one-liners (under `expect.cel`, or as a computed value with `$cel`), and functions in code files referenced with `function:`.
12. **Placement decides lifetime.** Data made in `beforeRun` lives for the run, in `beforeAll` for one test file, in `beforeEach` or `before` for one test. There is no separate scope setting.
13. **Whatever creates, undoes.** An api that creates something declares its `cleanup` once; every successful call registers it, and it runs when the block that made the call ends.
14. **Nested style.** Examples and `vc fmt` use indented YAML; inline `{ … }` parses the same but is not the house style.

## Files at a glance

A project is one `vibranium.config.yaml` at the root plus api, test and load files anywhere below it; folders are only for people.

| File | Holds | Top-level keys | Item IDs |
| --- | --- | --- | --- |
| `vibranium.config.yaml` | Project settings, vars, profiles (at least one), auth, secrets, generators, datasets, reports, history, alerts, plugins, run-level lifecycle blocks | `syntaxVersion`, `project`, `paths`, `defaults`, `vars`, `profiles`, `auth`, `secrets`, `generators`, `datasets`, `reports`, `history`, `alerts`, `plugins`, `redact`, `beforeRun`, `afterRun` | auth: `partnerLogin` |
| `*.api.yaml` | How to call endpoints, and how to undo them | `id`, `name`, `baseUrl`, `auth`, `headers`, `vars`, `profiles`, `source`, `beforeRequest`, `afterResponse`, `apis` | `users#create` |
| `*.test.yaml` | Tests and the lifecycle blocks around them | `id`, `name`, `tags`, `vars`, `profiles`, `beforeAll`, `afterAll`, `beforeEach`, `afterEach`, `tests` | `users.crud#getsCreatedUser` |
| `*.load.yaml` | A load scenario built from tests | `id`, `name`, `mix`, `stages`, `thinkTime`, `checks`, `thresholds`, `abortOn` | `checkout.peak` |
| `schemas/*.json` | JSON Schemas, loaded with `$file` | — | — |
| `datasets/*.csv`, `*.json`, `*.yaml` | Your own data lists | — | file name, e.g. `cities` |
| `hooks/*.ts`, `*.js`, `*.py`, … | Functions for lifecycle actions, request hooks and custom checks, referenced as `path#exportName` | — | — |
| `.vibranium/` | Generated: reports, `history.db`, `cache.db`; not committed | — | — |

`syntaxVersion` is the version of the file format, not of Vibranium or of your API. It changes only when the format has a breaking change. Upgrading Vibranium never edits it; `vc migrate` raises it when you choose to upgrade old files, so a newer Vibranium always knows how to read older ones.

## Templates and references

Every dynamic value is written `${namespace.path}`; `vc check` resolves each reference and reports unknown names before anything runs.

| Namespace | Value | Example |
| --- | --- | --- |
| `var` | A var: defaults, call values, profile values and saved values (see Vars and profiles) | `${var.usersUrl}`, `${var.admin.userId}` |
| `env` | Operating-system environment variables | `${env.CI_COMMIT_SHA}` |
| `secret` | Secrets from the secrets provider; always redacted | `${secret.SHOP_CLIENT_SECRET}` |
| `response` | The current call's `status`, `headers`, `body`, `timing`; used in `expect`, `save`, `poll` and an api's `cleanup` | `${response.headers.location}` |
| `request` | The current call's request as sent: `method`, `url`, `headers`, `body`; available where `response` is (D125) | `${request.headers["x-request-id"]}` |
| *action id* | The last response (`status`, `headers`, `body`, `timing`) of an action or step with that `id`, within the same test or block | `${place.body.total}` |
| `example` | The current row of `examples` | `${example.email}` |
| `item` | The current element of a `forEach` loop | `${item.sku}` |
| `iteration` | `index` (from 1) and `count` in a `repeat` or `forEach` loop, or a repeated test | `${iteration.index}` |
| `fake` | Generated data (see Generators and datasets) | `${fake.location.city}` |
| `dataset` | One of your datasets: its whole list of rows (see Generators and datasets) | `${dataset.capitals}` |
| `run` | `id` (the history run number, 0 when unrecorded), `startedAt` (RFC 3339 in UTC), `profiles` | `${run.id}` |

Where each root is available (D125): `response` and `request` exist only once a call has been made (in `expect`, `save`, `poll` and an api's `cleanup`); `item` only inside `forEach`; `iteration` only inside `repeat`, `forEach` or a repeated test; `example` only in a test with `examples`; an action or step id only after that action, in the same test or block. `var`, `env`, `secret`, `fake`, `dataset` and `run` are available everywhere, except in an `examples` written as one template (`examples: ${…}`): it is read once before the run starts, so it reads only `var`, `env` and `dataset`, and any other root there is V2105. Inline rows are rendered per instance and may read every root. There are no shorthands: write `response.status`, `response.headers` and `response.body`, in `expect` too. A bare `status`, `headers` or `body` that is not a step id is V2105, with a fix that inserts `response.` (D150). Using a root where it is not available is V2105 (`${item}` outside `forEach`, `${example}` without `examples` and `${iteration}` outside a loop are V2102) (D125, D137).

Paths: after a namespace, and as keys under `expect.body.match`, paths use dotted names, `[n]` for an index (negative counts from the end), `[*]` for every element, and quotes for unusual keys (`headers["x-request-id"]` or `['x-request-id']`; quoted keys and text arguments have no escapes). Arguments to pipes and generators are numbers written as in JSON (`-2`, `0.5`, `1e3`, without leading zeros), `true`, `false` and single-quoted text (`default('none')`); `null` and double-quoted text are V2305 (D151). Filtering is done with CEL.

Typing: a value that is exactly one template keeps the referenced type, so `limit: ${var.pageSize}` stays a number and `cities: ${var.cities}` stays a list; a template inside other text gives a string (`user-${run.id}`). Quoting does not change this. To change a type, use a pipe.

### Converting types with pipes

A pipe after the path converts or tidies the value: `${response.body.count | number}`. Pipes chain left to right: `${var.name | trim | lower}`.

| Pipe | Takes | Gives | Example |
| --- | --- | --- | --- |
| `number` | `"42.5"` | `42.5` | `${var.price \| number}` |
| `int` | `"42"` or `42.9` | `42` (truncated) | `${response.headers["x-total-count"] \| int}` |
| `bool` | `"true"`, `"false"` | `true`, `false` | `${var.enabled \| bool}` |
| `string` | any value | its text; objects and lists become compact JSON | `${var.id \| string}` |
| `json` | a string holding JSON | the parsed object or list | `${response.body.payload \| json}` |
| `jsonString` | an object or list | a JSON string | `${var.order \| jsonString}` |
| `base64`, `fromBase64` | text or bytes | encoded or decoded | `${var.csv \| base64}` |
| `lower`, `upper`, `trim` | a string | the changed text | `${var.email \| lower}` |
| `urlEncode` | a string | percent-encoded text | `${var.query \| urlEncode}` |
| `default('x')` | a missing or null value; as the first pipe, a var no level sets is not V2101 | the fallback, any literal | `${var.note \| default('none')}` |
| `random` | a list | one element, chosen at random (follows `--seed`) | `${dataset.capitals \| random}` |
| `shuffle` | a list | the list in random order | `${dataset.capitals \| shuffle}` |
| `take(n)` | a list | its first `n` elements | `${dataset.capitals \| take(10)}` |

To read inside a string of JSON, save it parsed first (`payload: ${response.body.payload | json}`), then read `${var.payload.orderId}`. A failed conversion, such as `"abc" | number`, fails the step with the value and the pipe in the message; `vc check` reports conversions it can already see will fail, such as `| number` on a field the api's schema says is an object.

### Computed values

Where a value needs arithmetic or a choice, `$cel` computes it with the same CEL used in checks. Its variables are the roots above, wherever each is available (D125), except `secret`, `fake`, `dataset` and `run`, which are V2105 in CEL: set a var from a template and read it as `var.<name>`. Whole numbers are CEL `int`s and decimals are `double`s, and CEL does not mix them in arithmetic: `var.price * 2` works when `price` is a whole number, and a decimal needs `var.price * 2.0` (comparisons such as `var.total > 100.0` work either way).

```yaml
vars:
  doubled:
    $cel: var.price * 2
  tier:
    $cel: "var.total > 100.0 ? 'gold' : 'standard'"
```

Escaping: write `$${` for a literal `${`.

Reserved names: no `id` (test, api, action, step, dataset…) can be a root name: `var`, `env`, `secret`, `response`, `request`, `example`, `item`, `iteration`, `fake`, `dataset` or `run`; `vc check` reports V2006 (D128, D137).

## Project config

`vibranium.config.yaml` sits at the project root and is the only project-wide file; `vc` finds it by walking up from the current directory.

Settings are layered, lowest to highest: built-in defaults < user config (`~/.config/vibranium/config.yaml`) < `vibranium.config.yaml` < the active profiles, in the order given < `.env` < `VIB_*` environment variables < `--set key=value` on the command line. Every value keeps the layer it came from, and `vc config explain <key>` prints it (D129).

- **User config.** `$XDG_CONFIG_HOME/vibranium/config.yaml`, else `$HOME/.config/vibranium/config.yaml` (`%APPDATA%\vibranium\config.yaml` on Windows). Only its `profile`, `defaults`, `reports` and `vars` keys are settings, and each counts only where `vibranium.config.yaml` and its profiles leave the value unset; other keys are preferences that commands read, such as `plugins.allowUnconfined: true`, which lets plugins run unconfined where they cannot be confined (D260; never read from the project, see `plugin-protocol.md`). A file that cannot be read or is not YAML stops the command with exit 2 and names the file.
- **`VIB_*` names (D182).** A setting's variable is its `--set` key upper-snaked: `VIB_PROFILE`, `VIB_DEFAULTS_CONCURRENCY`, `VIB_DEFAULTS_PER_HOST`, `VIB_DEFAULTS_TIMEOUT`, `VIB_REPORTS_DIR`, `VIB_REPORTS_FORMATS`. A var uses `VIB_VAR_<name>`, with the name exactly as written (`VIB_VAR_userId`). A `VIB_*` line in `.env` sets the same thing one layer lower.
- **`.env`.** Read from the project root, also when the environment is injected (tests, the MCP server). Its other lines fill `${env.X}` only for names the process environment does not set.

| Key | Type | Default | Purpose |
| --- | --- | --- | --- |
| `syntaxVersion` | integer | required, `2` | File-format version (see Files at a glance); a version below 2 or above the newest this `vc` reads is V1003 |
| `project` | string | `vibranium` (`vc init` writes the folder name) | Display name in reports (CTRF's `environment.appName`, JUnit's `<testsuites name>`) and on the server |
| `paths.include`, `paths.exclude` | lists of globs | everything except `node_modules` and `.vibranium` | Where definition files are found; written excludes replace the default ones, and `vibranium.config.yaml` itself is always read |
| `defaults.timeout` | duration | `30s` | Per request |
| `defaults.concurrency` | integer, at least 1 | `16` | Test instances in flight |
| `defaults.perHost` | integer, at least 1 | `8` | Connections per host |
| `defaults.retry` | `attempts`, `delay` | none | Applied to every test unless overridden; re-runs the whole test instance (D126) |
| `defaults.headers` | map | none | Sent with every request; each name an HTTP token, never a template (V2310) |
| `http.retry` | `attempts`, `on`, `methods`, `backoff` | `attempts: 0` (off) | Transport retry of one request (D143). `on` defaults to `[connect, timeout, 502, 503, 504]`, `methods` to `[GET, HEAD, OPTIONS, PUT, DELETE]`, `backoff` to `{ base: 100ms, max: 2s, jitter: true }` (exponential). Each attempt gets the full `timeout` |
| `vars` | map | — | Project-wide values |
| `profiles` | list | required, at least one; `vc init` creates `default` | Each: `id`, `name`, `default`, `vars`, `agents` (see Vars and profiles) |
| `auth` | list | — | Auth definitions (see Authentication) |
| `secrets` | `provider`, `schema` | `env` with `.env.schema` | Where `${secret.*}` comes from |
| `generators` | list | — | Named fake-data generators |
| `datasets` | `allowRoots`, `files` | every file in `datasets/` | Your own data lists: `files` lists data kept outside `datasets/` (each `id`, `file`), and `allowRoots` the folders outside the project those files may be in (see Datasets) |
| `reports` | `dir`, `formats` | CTRF only | Output formats and folder; html.layout is folder (default: an index plus data files loaded on demand) or single (one self-contained file). A format may also be the ID of a reporter plugin in `plugins:`; any other unknown name is V2302 |
| `history` | `bodies` (`none`, `failures`, `all`), `maxBodySize`, `keep` (a number of runs or `all`) | bodies of failures only, `64kb`, `50` (D170) | Run history and retention: bodies are cut at `maxBodySize` before they are stored, and the oldest runs past `keep` are deleted when a run ends |
| `alerts` | list | none | Notifications |
| `plugins` | list | none | Third-party plugins |
| `redact` | `headers`, `fields`, `rules` | auth headers and secrets | Extra values to mask; `rules` are your own `regex`, `cel` or `function` masks (see Secrets) |
| `beforeRun`, `afterRun` | lists of actions | none | Run-level lifecycle blocks (see Lifecycle blocks) |

```yaml
# vibranium.config.yaml
syntaxVersion: 2
project: shop

defaults:
  timeout: 10s
  concurrency: 32
  perHost: 16
  headers:
    accept: application/json

vars:
  usersUrl: http://localhost:8080
  ordersUrl: http://localhost:8081
  identityUrl: http://localhost:8090

profiles:
  - id: dev
    default: true
    agents:
      run: true
      call: [GET, POST, PUT, DELETE]
      writeDefinitions: true
  - id: staging
    vars:
      usersUrl: https://users.staging.shop.example
      ordersUrl: https://orders.staging.shop.example
      identityUrl: https://id.staging.shop.example
    agents:
      run: true
      call: [GET, POST]
      approve: [PUT, PATCH, DELETE]
      writeDefinitions: ask
  - id: prod
    vars:
      usersUrl: https://users.shop.example
      ordersUrl: https://orders.shop.example
      identityUrl: https://id.shop.example
    agents:
      run: true
      call: [GET]
      approve: [POST, PUT, PATCH, DELETE]
      writeDefinitions: false

auth:
  - id: shopOauth
    type: oauth2ClientCredentials
    tokenUrl: ${var.identityUrl}/oauth/token
    clientId: ${secret.SHOP_CLIENT_ID}
    clientSecret: ${secret.SHOP_CLIENT_SECRET}
    scopes: [users.read, users.write, orders.write]

secrets:
  provider: varlock    # opt-in; the default is env
  schema: .env.schema

reports:
  dir: .vibranium/reports          # each run's files go in <dir>/<run>/
  formats: [ctrf, junit, html]
  html: {layout: folder}           # or single: one self-contained report.html

history:
  bodies: failures
  maxBodySize: 4kb
  keep: 50

alerts:
  - on: [failure, recovery, flaky, threshold]
    profiles: [staging, prod]
    notify:
      - slack:
          channel: "#qa-alerts"
          webhook: ${secret.SLACK_WEBHOOK}

beforeRun:
  - command: docker compose up -d --wait
  - id: admin                    # created once, before any test starts
    call: users#create
    vars:
      role: admin
    cache:
      for: 12h
      check:
        call: users#get
        vars:
          userId: ${var.admin.userId}

afterRun:
  - function: hooks/notify.ts#postSummary
```

## Vars and profiles

Every value you set is a var, read as `${var.x}`, and a profile only changes values, never logic.

Where vars are set, from the closest to the call to the furthest:

1. `vars` on the call itself: a test, a step or an action
2. Values saved earlier in the same test, by its `beforeEach` and `before` blocks and its steps; the latest wins
3. Values saved by the file's `beforeAll`, under the action's `id` (`${var.catalog.productIds}`)
4. The test file's `vars`
5. Values saved by `beforeRun`, under the action's `id` (`${var.admin.userId}`)
6. The api's own `vars` (its defaults)
7. The api file's `vars`
8. The config's `vars`

The closest value wins. A template that no level sets is a `vc check` error naming the var, so there is no separate "required" flag.

Profiles replace environments and work like Spring Boot profiles. In Vibranium, "environment" only ever means operating-system environment variables (`${env.X}`).

- **Every project has at least one profile.** `vc init` creates one called `default` and marks it `default: true`.
- **Every run has an active profile.** Name it with `--profile staging` (or `VIB_PROFILE=staging`); otherwise the profile marked `default: true` applies. A project with no profiles, or a run with none active, is a `vc check` error.
- **Several profiles apply in order, later ones winning:** `--profile staging,eu`.
- **Profiles override vars at each level.** The config's `profiles` override config `vars`. Any api or test file can carry its own `profiles` list to override that file's `vars`; a profile id used there must exist in the config, so a typo is an error, not a silent no-op.
- **A config profile's `agents` block says what an AI agent may do under it** (MCP or the web UI, D195): `run` (bool, default `false`) lets it run tests; `call` lists the methods its requests send without asking and `approve` those sent once a person approves (both default `[]`); `writeDefinitions` is `true`, `false` (default) or `ask`, which goes through the same approval flow. A profile with no `agents` block allows only reads: list, search, get, check and query history. With several active profiles, the most restrictive rule wins for each field.

```yaml
# services/orders/checkout.test.yaml (excerpt)
vars:
  currency: USD
  couponCode: WELCOME10
profiles:
  - id: eu
    vars:
      currency: EUR
  - id: prod
    vars:
      couponCode: PROD-SMOKE
```

Saving values from a response: `save` maps names to templates over `response`; a value can be one template, or a map of them, which saves an object. Where you save decides who sees the value:

- **On a test, step or per-test action** (`before`, `beforeEach`): the values are vars for the rest of that test.
- **In `beforeAll` and `beforeRun`:** everything an action saves is shared under the action's `id`, which those blocks require.
- **On an api:** `save` runs on every call of it whose status matches the api's `saveOn`, so every caller gets the same names. `saveOn` takes what `expect.status` takes (`201`, `2xx`, `[200, 201]`, a matcher or a template) and defaults to the api's own `expect.status`, or `2xx` when it has none. A test that expects another status, such as a 400, therefore skips the api's `save`, just as a non-2xx call registers no cleanup (D191). A test's or step's own `save` runs whatever the status, once its checks hold.
- **Inside a loop:** every saved name collects a list (see Loops: repeat and forEach).

```yaml
save:
  orderId: ${response.body.id}
  location: ${response.headers.location}
  buyer:
    id: ${response.body.buyer.id}
    email: ${response.body.buyer.email}
```

## Api files

An api file describes how to call a group of endpoints, the checks that must hold on every call of each one, and how to undo what a call creates.

File-level keys:

| Key | Type | Purpose |
| --- | --- | --- |
| `id` | file ID, optional; must match the path (D139) | Prefix of every api in the file |
| `name` | text | Shown in listings |
| `baseUrl` | template | Prefixed to every `path`; written without a trailing `/` (V4005) |
| `auth` | auth ID, or `none` | Default auth for the file |
| `headers` | map | Default headers |
| `vars` | map | Defaults for every api in the file |
| `profiles` | list | Per-profile overrides of the file's `vars` |
| `source.openapi` | path or URL | Spec this file syncs from: a URL or a file, JSON or YAML (a path is relative to the project root and must exist, V2004); with source.headers for a protected URL, source.include to take only some tags or path prefixes, and source.only to take only some operation IDs (D277) |
| `beforeRequest`, `afterResponse` | lists of functions | Run around every request in the file (see Functions and request hooks) |
| `apis` | list | The apis |

Each item in `apis`:

| Key | Type | Default | Purpose |
| --- | --- | --- | --- |
| `id` | camelCase, required | — | Reference, e.g. `create` → `users#create` |
| `name` | text | — | Shown in listings |
| `method` | `GET` `POST` `PUT` `PATCH` `DELETE` `HEAD` `OPTIONS` | `GET` | HTTP method |
| `path` | template | required | Appended to `baseUrl`, e.g. `/users/${var.id}`; starts with `/` unless it is a full URL or starts with a template (V4005); a path parameter is `${var.name}`, never `{name}` (V4006). Literal text is sent as written; each `${…}` value is percent-encoded as one segment, so a `/`, `?`, `#`, `%` or space in it stays data and `.` or `..` names no other path (D278). A value before the first `/` is sent as written only when it is an absolute `http(s)://` URL (`${var.usersUrl}/users`), and one after a literal `?` is encoded as a query value. The `urlEncode` pipe is not needed, and is not applied twice (V4007) |
| `vars` | map | — | Defaults for this api; callers override them |
| `query` | map of templates | — | Query string |
| `headers` | map of templates | — | Merged over the file's headers |
| `body` | see Request body | none | Request body |
| `auth` | auth ID, `none`, or `id` + `vars` | the file's | Overrides the file's auth |
| `timeout` | duration | `defaults.timeout` | Per request attempt |
| `http.retry` | as in config | config's `http.retry` | Transport retry for this api; keys given here replace the config's (D143) |
| `expect` | checks | `status: 2xx` | Checks that hold on every call; a test's `expect` adds to them and replaces matching keys |
| `save` | map of templates over `response` | — | Values every caller gets as vars, when the status matches `saveOn` |
| `saveOn` | status: code, list of codes, class (`2xx`) or matcher | the api's `expect.status`, else `2xx` | The statuses on which `save` runs (D191) |
| `cleanup` | `call` + `vars` | — | How to undo a successful call; registered automatically (see Cleanup on an api) |
| `operationId` | text | — | Link to the OpenAPI operation, kept by sync |
| `beforeRequest`, `afterResponse` | lists of functions | — | Run around this api's requests, after the file's |
| `deprecated` | boolean | false | Set by sync when the operation leaves the spec, or by hand; vc check warns (V3203) where tests still call it, saying the operation left the spec only for an api with an `operationId` or in a file with a `source` |

### Request body

| You write | Sent as | Default `content-type` |
| --- | --- | --- |
| no `body` key | empty | none |
| nested YAML, or pasted JSON | JSON | `application/json` |
| a string | the text as written | `text/plain` |
| one `${…}` that renders to a map or list (`body: ${var.payload}`) | JSON; a map under a form or multipart `content-type` is encoded as in the rows below | `application/json` |
| `$file: path` | the file's bytes, any file, binary included (D272). The path is literal (a `${…}` in it is V5111), relative to the file that names it, and checked by `vc check` (V2004); the file is read before the run, only when the run can send it. Inside a JSON body a `$file` map is sent as written | by extension: `json` `application/json`, `csv` `text/csv`, `txt` `text/plain`, `xml` `application/xml`, `html`/`htm` `text/html`, `yaml`/`yml` `application/yaml`, `png` `image/png`, `jpg`/`jpeg` `image/jpeg`, `gif` `image/gif`, `pdf` `application/pdf`, `zip` `application/zip`; any other `application/octet-stream` |
| `$base64: …` | the decoded bytes | `application/octet-stream` |
| `$extend: <object>` plus changes | the object with your changes (see below) | `application/json` |
| a map with `content-type: application/x-www-form-urlencoded` | URL-encoded form (D273): a space as `+`, a list repeats its key, a null is left out; a nested object or list is V5111. The media type is matched without its parameters or case | the header as written |
| a map with `content-type: multipart/form-data` | multipart (D274, RFC 7578), one part per field: text as UTF-8 with no part type; a `{$file: path}` value as a file part with the file's name and its extension's type; another map or a list of lists as an `application/json` part. A list repeats the part; a null is left out. A part's `$file` path must be written literally in the body: a `${…}` in it, or a `$file` map that arrives through `${…}`, is V5111. The boundary is the header's `boundary=` if it names one (1 to 70 RFC 2046 characters, not ending in a space, found in no part; else V5111), else `vc-` and 24 hex characters from the run's seeded random source | the header as written, with `; boundary=…` added when it names none |

```yaml
# services/users/users.api.yaml
id: users
name: User accounts service
baseUrl: ${var.usersUrl}/api/v1
auth: shopOauth
source:
  openapi: https://users.staging.shop.example/v3/api-docs

apis:
  - id: create
    name: Create a user
    method: POST
    path: /users
    vars:
      name: ${fake.person.fullName}
      email: ${fake.internet.email}
      role: member
    body:
      name: ${var.name}
      email: ${var.email}
      role: ${var.role}
    expect:
      status: 201
    save:
      userId: ${response.body.id}
    cleanup:
      call: delete
      vars:
        userId: ${response.body.id}

  - id: get
    name: Get a user
    path: /users/${var.userId}

  - id: delete
    name: Delete a user
    method: DELETE
    path: /users/${var.userId}
    expect:
      status: [200, 204, 404]

  - id: importCsv
    name: Import users from CSV
    method: POST
    path: /users/import
    headers:
      content-type: text/csv
    body:
      $file: ../../fixtures/users.csv

  - id: uploadAvatar
    name: Upload an avatar image
    method: PUT
    path: /users/${var.userId}/avatar
    headers:
      content-type: multipart/form-data
    body:
      file:
        $file: ../../fixtures/avatar.png
      caption: Profile photo
    beforeRequest:
      - function: ../../hooks/sign.ts#addUploadSignature
```

JSON pasted as is works too:

```yaml
  - id: rename
    method: PATCH
    path: /users/${var.userId}
    body: {"name": "${var.name}", "tags": ["renamed"]}
```

OpenAPI import and sync (`vc import openapi`, `vc sync users`) are described in the Vibranium v2 CLI and Developer Workflow doc. In short: the spec can be a URL or a file, JSON or YAML (Swagger 2.0 is converted). Every import and sync saves a snapshot to `openapi/<fileId>.yaml`, which you commit; sync uses it as the merge base. Sync updates `method`, `path`, each `query` entry and the `content-type` header, and never touches IDs, names, `body`, `expect`, `save`, `cleanup`, hooks or defaults you wrote. Conflicts are reported instead of overwritten, and an operation removed from the spec leaves its api in place with `deprecated: true`.

### Reusing a payload with changes

`$extend` starts a map from an existing object, usually a response body or a saved value, and the keys beside it change it. Maps merge deeply, so `shipping.city` changes one field and keeps the rest of `shipping`; lists and plain values are replaced whole; `null` sets a field to null. `$remove` lists paths to drop, such as server-generated fields. Both work in any map value: a body, a var, a saved object. The base must be a template that renders an object: a written value is V2302, a `$remove` without `$extend` is V2211, and a base that renders anything else fails the action (V5111).

`body` on a test, step or action replaces the api's body for that one call. Use it when a test needs a body the api's vars cannot express, as here:

```yaml
  - id: reordersToNewCity
    name: Re-placing an order with a new city keeps every item
    steps:
      - id: original
        call: orders#get
        vars:
          orderId: ord_existing
      - call: orders#place
        body:
          $extend: ${original.body}
          shipping:
            city: Pune
          $remove: [id, status, createdAt, total, shipping.geo]
        expect:
          status: 201
          cel:
            same items: response.body.items == original.body.items
```

If a value arrives as text but you need it as JSON, or the other way round, convert it with a pipe (see Converting types with pipes): `$extend: ${response.body.payload | json}`.

### Cleanup on an api

An api that creates something says once how to undo it. Every call with a 2xx status registers its cleanup, with the cleanup's `vars` resolved at that moment against that call's `response`, so ten creates register ten deletes. Registered cleanups run in reverse order of creation when the block that made the call ends. `keep: true` on a call skips its cleanup.

| The call was made in | Its cleanup runs |
| --- | --- |
| a test's steps, `before` or `beforeEach` | after the test's `after` and `afterEach`, whether the test passed or not |
| the file's `beforeAll` | after the file's `afterAll` |
| the config's `beforeRun` | after `afterRun` |
| an action with `cache` | when the cache entry expires, when a fresh result replaces it (at the end of that run), or on `vc cache clear`; not at the end of the run that made it |

Cleanups also run after Ctrl-C. A failed cleanup is reported but does not change test results; the cleanup call itself never registers a cleanup.

### Which base URL and auth a call uses

Every api takes its base URL and auth from its own file, so one test can call several services and each request goes to the right host with the right credentials. In a test that calls `users#create` and then `orders#place`, the first request goes to `${var.usersUrl}` with the users file's auth and the second to `${var.ordersUrl}` with the orders file's auth, without the test saying anything.

An api can override its file's `auth`, and a test, step or action can override it again with `auth:` for one call, for example to act as a different user. The base URL changes only through vars: to point one call elsewhere, set the var its file's `baseUrl` uses in that call's `vars` (`usersUrl: http://localhost:9000`).

## Tests

A test calls an api (or several, as steps) and states what it expects; it passes only if every check passes.

File-level keys of a test file:

| Key | Type | Purpose |
| --- | --- | --- |
| `id` | file ID, optional; must match the path (D139) | Prefix of every test in the file |
| `name` | text | Shown in reports |
| `tags` | list | Added to every test in the file |
| `vars` | map | File-level vars |
| `profiles` | list | Per-profile overrides of the file's `vars` |
| `beforeAll`, `afterAll` | lists of actions | Run once for the file, around all its tests (see Lifecycle blocks) |
| `beforeEach`, `afterEach` | lists of actions | Run around every test instance in the file |
| `tests` | list | The tests |
| `skip` | a reason, or reason and until | Skip every test in the file (see Skipping) |
| `allowFailure` | `true`, or reason and until | Every test in the file may fail without failing the run (see Allowing a failure) |

Each item in `tests`:

| Key | Type | Default | Purpose |
| --- | --- | --- | --- |
| `id` | camelCase, required | — | Reference, e.g. `users.crud#getsCreatedUser` |
| `name` | text | `id` | Shown in reports |
| `tags` | list | — | Added to the file's tags |
| `before`, `after` | lists of actions | — | Run around this test only, inside `beforeEach` and `afterEach` |
| `call` | api ID | one of `call` or `steps` | The api to call |
| `steps` | list of actions | — | Several actions in order (see Actions); usually `call`s, each with its own `id`, `vars`, `expect`, `save` |
| `vars` | map | — | Values for this test and its call; each is evaluated once per test instance |
| `auth` | auth ID, `none`, or `id` + `vars` | the api's | Call as a different identity |
| `headers`, `query` | maps of templates | — | Merged over the api's headers and query |
| `body` | as an api's `body` | the api's | Replaces the api's body for this test |
| `expect` | checks | the api's `expect` | Merged with the api's checks (see Checks and matchers) |
| `save` | map of templates over `response` | — | Name values for later steps, `after` and `afterEach`; with `steps`, reads the last step's response (D124) |
| `poll` | `until`, `every`, `timeout` | none: all three are required (D156) | Re-send until `until` (checks) holds, then check `expect`. Only the last exchange is reported, also when the poll gives up (V5104) |
| `examples` | list of maps, `$file`, or one template giving a list of maps | — | One test instance per row; values in `${example.*}`. A template is read once before the run, from `var`, `env` and `dataset` only. A run skips a test given no rows, with a warning |
| `repeat` | integer, at least 1 | `1` | Run the whole test this many times, each reported as its own instance |
| `retry` | `attempts`, `delay` | `defaults.retry` | Re-run the whole test instance after a failure, cleanups included; a pass after a failure is reported as flaky (D126) |
| `timeout` | duration | per-request timeout × calls | Whole-test limit, per attempt. `after` and `afterEach` actions are outside it: one with its own `timeout` is limited by that, one without only by its requests' timeouts |
| `skip` | a reason, or reason and until | — | Skip with a reason, optionally until a date; reported as skipped (see Skipping) |
| `allowFailure` | `true`, or reason and until | the file's | Run and report the test, but a failure does not fail the run (see Allowing a failure) |
| `standalone` | boolean | `true` | `false`: runs only when reused with `test:`, never on its own |

With `steps`, the test-level `auth`, `headers`, `query`, `body`, `poll` and `expect` are defaults for every `call` step, and a step's own value wins; `test:`, `function` and `command` steps take none (D124). A `test:` step runs the reused test as written, so neither the defaults nor those keys written on the step itself reach its calls (D270). A test-level `save` reads the last step's response. `vc check` warns when a `body` default reaches a `GET` or `HEAD` step (V3205), when no step uses a default (V3206), and when a `test:` action has one of those keys (V3207).

```yaml
  - id: renamesUser
    name: A renamed user reads back with the new name
    headers:
      x-request-source: vibranium-tests
    expect:
      headers:
        content-type:
          contains: json
    steps:
      - call: users#create
      - call: users#rename
        vars:
          name: Ada Lovelace
      - call: users#get
        expect:
          body:
            match:
              name: Ada Lovelace
    save:
      finalName: ${response.body.name}
```

Every step sends `x-request-source` and checks the `content-type`; only the last step checks the name, and `finalName` comes from `users#get`.

Retries and statuses (D126): `retry` on a test re-runs the whole test instance, with its cleanups run between attempts. A pass after a failure is reported as **flaky**. A **blocked** instance is never retried. Retrying a single request on a 5xx or a timeout is a transport setting, `http.retry` (D143, off by default), not a test retry. Every test instance ends as passed, failed, blocked, skipped or flaky.

Order of work: the file's `beforeAll` runs once, before any of its selected tests starts (D122). Then, for each test instance: `beforeEach` → `before` → the call or steps (each: request, the api's `expect` merged with the test's and the step's, `poll`, `save`, cleanup registered) → the test-level `save` → `after` → `afterEach` → registered cleanups, newest first → record. `after` and `afterEach` run even when something before them failed. After the file's last test: `afterAll`, then the file's cleanups.

```yaml
# services/users/crud.test.yaml
id: users.crud
name: User accounts
tags: [users, smoke]

tests:
  - id: createsMember
    name: Creates a member account
    call: users#create
    expect:
      body:
        match:
          role: member

  - id: getsCreatedUser
    name: Returns the user that was just created
    before:
      - call: users#create
    repeat: 5
    call: users#get
    expect:
      status: 200
      body:
        schema:
          $file: ../../schemas/user.json
        match:
          id: ${var.userId}
          email:
            endsWith: "@example.com"
      timing:
        total:
          lt: 500ms

  - id: rejectsBadEmail
    name: Rejects malformed email addresses
    examples:
      - email: no-at-sign
      - email: ""
    call: users#create
    vars:
      email: ${example.email}
    expect:
      status: 400
      cel:
        names the email field: response.body.errors.exists(e, e.field == 'email')

  - id: exportFinishes
    name: A user export completes within two minutes
    steps:
      - id: start
        call: users#startExport
        save:
          jobId: ${response.body.jobId}
      - id: wait
        call: users#exportStatus
        poll:
          until:
            body:
              match:
                state: done
          every: 2s
          timeout: 2m
        expect:
          body:
            match:
              rows:
                gt: 0
```

Notes on the example: `users#create` saves `userId` and declares its own cleanup, so no test here has to delete anything. `getsCreatedUser` creates a fresh user in `before` for each of its five instances and reads it as `${var.userId}`. `rejectsBadEmail` replaces the api's `status: 201` with `400` simply by stating it; a 400 creates nothing, so no cleanup is registered, and the api's `save` is skipped because 400 does not match its `saveOn`, which defaults to the api's own `status: 201` (D191). The error body needs no `id` for the test to pass.

### Skipping

`skip` parks a test, or a whole file when written at the top of the file, like JUnit's `@Disabled`. A reason is required and shows in every report, where the test is counted as **skipped**; `until` is optional and is a date written `YYYY-MM-DD` (UTC); after that day the test runs again and `vc check` warns that the skip expired (V3104). A skipped file's `beforeAll` and `afterAll` don't run. `vc run --include-skipped` runs skipped tests anyway; `vc skip` and `vc unskip` edit the YAML for you.

```yaml
  - id: appliesCoupon
    skip:
      reason: "coupon service flaky on staging, see #212"
      until: 2026-11-15
```

### Allowing a failure

`allowFailure` keeps a known failure from failing the run while it is still worth running, like GitLab CI's `allow_failure`. It is `true`, or a map with the same shape as `skip`: a required `reason` and an optional `until` date (`YYYY-MM-DD`, UTC). Written at the top of a file it covers every test in the file; a test's own `allowFailure` wins over the file's. The test still runs and is reported. When it fails (after its `retry`), it ends as **allowed to fail** (`allowedFailure`, shown as ⚠ with the reason), is counted apart in the summary (`1 allowed to fail`), in CTRF (`other`) and in JUnit (a skipped case whose message gives the reason and the failure), and does not change the exit code. A test that passes, or is blocked by a setup, is reported as usual. After `until`, the setting is ignored, so a failure fails the run again, and `vc check` warns that it expired (V4008); an expired test-level setting falls back to the file's. `vc run --strict` treats every allowed failure as a failure (D248).

```yaml
allowFailure: {reason: "payments sandbox is down, see #340", until: 2026-12-01}
tests:
  - id: refundsOrder
    call: payments#refund
  - id: listsRefunds
    call: payments#list
    allowFailure: true
```

## Lifecycle blocks

Lifecycle blocks prepare data and tidy up around tests, like JUnit's `@BeforeAll` and `@BeforeEach`. Each block is a list of actions, and where the block sits decides when it runs and how long what it creates lives. There is no separate scope setting.

| Block | Written in | Runs | What it creates lives until |
| --- | --- | --- | --- |
| `beforeRun`, `afterRun` | `vibranium.config.yaml` | once per run, when any test is selected: before any test starts, after the last (D122) | the end of the run, or the cache expiry |
| `beforeAll`, `afterAll` | a test file | once per file, when one of its tests is selected: before any of them starts, after the last (D122) | the end of the file |
| `beforeEach`, `afterEach` | a test file | around every test instance in the file, including each `repeat` and `examples` row | the end of that test instance |
| `before`, `after` | a test | around that test's instances, inside `beforeEach` and `afterEach` | the end of that test instance |

### Actions

An action is one entry in a lifecycle block or in a test's `steps`. Most actions call an api: the api already knows its URL, auth, body, checks, saved values and cleanup, so a block only says which api to call and with which values.

| Action | Does | Use it for |
| --- | --- | --- |
| `call: <api>` | Calls an api with its base URL, auth, checks, saves and cleanup | Almost everything |
| `test: <test>` | Runs another test as written: its calls in order, each with its api's checks and that test's own test-level `expect` (D270) | A multi-step flow that already exists as a test (see Reusing a multi-step flow) |
| `function: path#export` | Runs a function from a code file | Work no api does: seeding a database, reading a mailbox; it may return an object whose fields are saved |
| `command: <shell command>` | Runs a command through `sh -c` (`cmd /C` on Windows) in the project root; a non-zero exit fails the action (V5110), and its output is attached to the report (details below) | Starting services, `docker compose up -d --wait` |

**`command:` safety (D181).** The text you write runs as written; each `${…}` in it is quoted. On unix every rendered value becomes one POSIX word in single quotes (a `'` inside is written `'\''`), so `--set "vars.word=it's; touch x"` echoes that text and runs nothing else. On Windows a value is wrapped in double quotes, and a value holding any of `& | < > ^ % ! " ( )` or a newline fails the action with V5111 before anything runs; pass such a value through an environment variable instead. The child's environment is the process environment, plus the `.env` lines the process does not set, plus `VIB_VAR_<name>` for each config var; it is never logged. The folder of the running `vc` goes on the end of its `PATH` when it is not there yet, so `vc …` in a line finds the running vc even when vc was started by its path; a `vc` already on `PATH` is still the one found. It keeps the last 64 KiB of stdout and of stderr, both passing through the redactor, and a V5110 failure shows the exit code and the last 20 lines. The limit is the action's own `timeout`, else `defaults.timeout` (30 s when unset); past it the whole process group is killed (unix) and the action fails with V5104. Ctrl-C ends the command the same way. The action ends when the command exits: a process it leaves in the background (`./server &`) keeps running while vc runs, and what that process writes after the command's exit (and a further 200 ms) is discarded. Once vc exits, such a process ends at its next write to that output; one that must outlive vc should send its output elsewhere (`./server > server.log 2>&1 &`).

`call` and `test` actions also take `id`, `vars`, `auth`, `headers`, `query`, `body`, `expect`, `save`, `poll`, `retry`, `keep`, `cache` and the loop keys `repeat`, `forEach` and `parallel`, each meaning the same as on a test. `function` and `command` take `id`, `vars`, `timeout` and the same loop keys; a loop runs any action kind once per turn. `timeout` on an action is that action's own limit (D192): a `command`'s process timeout, a `function`'s run budget (200 ms by default), a `call`'s limit on each request attempt (it wins over the api's `timeout` and `defaults.timeout`), and a reused `test`'s limit on all its calls together (past it the call in flight fails with V5104). Without it, each kind keeps its usual limit. `function` and `test` actions may also declare `provides` (below).

In `beforeRun` and `beforeAll` every action that produces values needs an `id`, and everything it saves, including its api's `save`, is shared under that id: `${var.catalog.productIds}`. In per-test blocks and steps, saved names are plain (`${var.userId}`), as for any step.

A `function:` or `test:` action can't always be read ahead of time to see what it saves, so it may declare `provides: [name, …]`; `vc check` then counts those names as set when it looks for vars that are never set (V2101). A `test:` action that names a test `vc check` can resolve already declares everything that test saves (its own `save`, its api's `save`, and what its steps save), so it needs `provides` only for names it can't see, such as those a `function:` step inside it returns (D282). At run time, using a name such an action saved without declaring it is a V5108 warning (the run goes on, D186), and a name declared in `provides` that it did not save is V5109 (D128); the names a `test:` action declares by being resolved are not held to V5109, since the reused test may save some only on some paths.

```yaml
# services/orders/checkout.test.yaml (excerpt)
beforeAll:
  - id: catalog
    call: catalog#seedProducts

beforeEach:
  - call: users#create
  - call: carts#create
  - call: carts#addItem
    forEach: ${var.catalog.productIds}
    vars:
      sku: ${item}

afterAll:
  - command: ./scripts/print-order-stats.sh
```

Every test in this file gets its own buyer and cart (`${var.userId}` and `${var.cartId}`, from those apis' `save`), and both are deleted after the test by their apis' cleanups. The catalog is seeded once for the whole file.

### Shared values and concurrency

Tests run in parallel, up to `defaults.concurrency`; within one test, actions and steps run in order.

- `beforeRun` and `beforeAll` run eagerly (D122). `beforeRun` runs once at the start of any run that selects at least one test, and finishes before any test starts; `beforeAll` runs only for a file with a selected test, and finishes before any test in the file starts. Each shared action runs at most once, and its saved values are computed once and shared. The file's tests then run in parallel, and `afterAll` waits for all of them. What stays lazy is reuse: cached actions and auth tokens are only fetched again when missing or expired.
- Values from `beforeRun` and `beforeAll` are shared by tests running at the same moment, so treat them as read-only. A test that changes or deletes data creates its own in `beforeEach` or `before`. `vc check` warns when a `PUT`, `PATCH` or `DELETE` call uses a shared value.
- `beforeEach` and `before` run separately for every test instance, so parallel instances never share what they create.
- If a `beforeRun` or `beforeAll` action fails after its `retry`, it is reported once, with its request, response and failed check, and every test that needs it is marked **blocked** with a link to that failure. If `beforeEach` or `before` fails, that test instance is marked blocked the same way; its `after`, `afterEach` and cleanups still run. Which tests are blocked is decided by what each test reads, and a blocked test is never retried (D122, D126).
- A cycle (a test reused with `test:` that, directly or through others, reuses the first) is a compile error that prints the whole cycle.

### Loops: repeat and forEach

Any `call` or `test` action can run several times without being written several times.

| Key | Runs the action | Available inside it |
| --- | --- | --- |
| `repeat: 10` | 10 times | `${iteration.index}` (from 1) and `${iteration.count}` |
| `forEach: <list>` | once per element of a literal list, a list-valued template (`${var.cities}`) or a `$file` with a CSV, JSON or YAML list | `${item}` is the element (`${item.name}` for a row), plus `iteration` |
| `parallel: 5` | up to 5 iterations at a time; at least 1, and only with `repeat` or `forEach` (V2211) | default `1`: one after another, in order |

Write one of `repeat` and `forEach` on an action; when both are written, `forEach` is used.

Every name saved inside a loop collects a list, one entry per iteration in iteration order, even when iterations run in parallel. A `forEach` whose items render as an empty list runs nothing and passes, and each name the action (or its api) saves is an empty list. `${var.cities[*].id}` reads one field from every entry. Each successful iteration registers its own cleanup. If an iteration fails, the action fails once the iterations in flight finish, and the cleanups of the ones that succeeded still run.

`repeat` on a test is different: it runs the whole test that many times and reports each instance.

Example: create ten cities, then a location group from their IDs.

```yaml
# services/geo/geo.api.yaml
id: geo
name: Geography service
baseUrl: ${var.geoUrl}/api/v2
auth: geoApiKey

apis:
  - id: createCity
    name: Create a city
    method: POST
    path: /cities
    vars:
      cityName: ${fake.location.city}
      country: ${fake.location.countryCode}
    body:
      name: ${var.cityName}
      country: ${var.country}
    expect:
      status: 201
    cleanup:
      call: deleteCity
      vars:
        cityId: ${response.body.id}

  - id: deleteCity
    name: Delete a city
    method: DELETE
    path: /cities/${var.cityId}
    expect:
      status: [204, 404]

  - id: createLocationGroup
    name: Create a location group
    method: POST
    path: /location-groups
    vars:
      groupName: ${fake.lorem.words(2)}
    body:
      name: ${var.groupName}
      cityIds: ${var.cityIds}
    expect:
      status: 201
    cleanup:
      call: deleteLocationGroup
      vars:
        groupId: ${response.body.id}

  - id: deleteLocationGroup
    name: Delete a location group
    method: DELETE
    path: /location-groups/${var.groupId}
```

```yaml
# services/geo/groups.test.yaml
id: geo.groups
name: Location groups

tests:
  - id: groupsTenCities
    name: A location group keeps the ten cities it was created with
    steps:
      - call: geo#createCity
        repeat: 10
        parallel: 5
        save:
          cities:
            id: ${response.body.id}
            name: ${response.body.name}
            longitude: ${response.body.location.lon | number}
      - call: geo#createLocationGroup
        vars:
          cityIds: ${var.cities[*].id}
        expect:
          body:
            match:
              cityIds:
                length: 10
          cel:
            same cities in the same order: response.body.cityIds == var.cities.map(c, c.id)
```

After the test, the group is deleted first and then the ten cities, because cleanups run newest first. To send whole objects instead of IDs, the api's body can use `cities: ${var.cities}`. To create named cities from a file instead of random ones:

```yaml
      - call: geo#createCity
        forEach: ${dataset.capitals}
        vars:
          cityName: ${item.name}
          country: ${item.country}
```

### Caching across runs

Data that is slow or rate-limited to create can outlive a run. `cache` on a `call` or `test` action keeps what the action saved in `.vibranium/cache.db`, per profile, and later runs reuse it instead of calling again.

| Key | Type | Purpose |
| --- | --- | --- |
| `cache.for` | duration | How long the result is reused |
| `cache.check` | an action | Optional quick call that confirms the cached data still exists; if it fails, the action runs again |

Rules:

- The cache key is the api (or test), the action's own resolved `vars` and the active profiles, so `users#create` with `role: admin` and with `role: member` are cached separately. Vars that read `fake.*` are left out of the key, so a fresh fake name does not defeat the cache.
- A hit returns only what the action saved; the action sends no request and nothing is checked. `cache.check` runs first, and its call is reported (an `exchanged` event, hit or miss) before the action's own, seeing the cached values both by name and under the action's `id` (`${var.admin.userId}`); when the check fails, the action runs again, its new result replaces the entry and the old entry's cleanup runs at the end of that run, after every test, since another test may still be using what it made; it belongs to no test, so only a failure shows, as a warning. If that run is killed or interrupted a third time first, the next run cleans the old entry up when it starts, as an expired one.
- Only a result that passed is stored, and only when each cleanup it registered is its api's `cleanup`; anything else runs every time.
- A cached action's cleanup does not run at the end of the run. It runs, under the profiles the entry was made with, when the entry expires (at the start of the next run that finds it expired), when a fresh result replaces it (at the end of that run), or on `vc cache clear [id]`. A cleanup that fails is a warning, not a failure.
- The whole entry is encrypted (ChaCha20-Poly1305). The key is `VIB_CACHE_KEY` (64 hex digits) when it is set, else the one the OS keychain holds, generated and stored there on first use. Where there is neither, as in most CI, the cache is off: `cache` actions run every time and the run prints warning V5113. An entry sealed under another key, or changed on disk, reads as a miss.
- `vc run --no-cache` neither reads nor writes the cache for one run, and cleans up no expired entry; `vc cache list` shows the entries and when they expire.
- `cache` belongs in `beforeRun` and `beforeAll`. On a per-test action `vc check` warns, because every test instance would share one cached object.

```yaml
# vibranium.config.yaml (excerpt)
beforeRun:
  - id: admin
    call: users#create
    vars:
      role: admin
    cache:
      for: 12h
      check:
        call: users#get
        vars:
          userId: ${var.admin.userId}
```

### Reusing a multi-step flow

Most blocks call apis directly. When the data takes several calls that already exist together as a test, `test:` runs that test's calls instead of repeating them. Its saved values arrive as if the calls were written in place (under the action's `id` in shared blocks), its test-level `expect` is checked as when it runs on its own (D270), and its apis' cleanups are registered as usual. A test meant only for reuse has `standalone: false`, so it never runs or appears in reports on its own.

```yaml
# services/shared/flows.test.yaml
id: shared.flows
name: Reusable flows

tests:
  - id: verifiedMember
    name: Sign up, confirm the email, log in
    standalone: false
    steps:
      - call: users#signUp
      - call: mail#latestCode
        vars:
          to: ${var.email}
        save:
          code: ${response.body.code}
      - call: users#confirmEmail
      - call: users#login
```

```yaml
# in any test file
beforeEach:
  - test: shared.flows#verifiedMember
```

## Checks and matchers

`expect` has one shape wherever it appears: on an api (checks that always hold), on a test, step or action, in `poll.until`, and in load thresholds. Within it every value is either a literal (equals) or a matcher.

| Key | Value | Example |
| --- | --- | --- |
| `status` | code, list of codes, class (`2xx`), or matcher (at least one); a template such as `${example.status}` is read at run time as a written status: a code as a number or text (`201`, `"201"`), a class (`"2xx"`) or a list of them | `status: [200, 201]` |
| `headers.<name>` | matcher; names are case-insensitive | `content-type: {contains: json}` |
| `body.schema` | JSON Schema, usually `$file` | `schema: {$file: schemas/user.json}` |
| `body.match.<path>` | matcher at a path in the body | `items[*].price: {every: {gt: 0}}` |
| `body.equals` | the whole body, compared deeply | `equals: {"ok": true}` |
| `size` | matcher on body size; a bare size and `eq`/`ne` are read in bytes, as `lt` is (`size: 32b`) | `size: {lt: 100kb}` |
| `timing.<phase>` | matcher on `dns`, `connect`, `tls`, `firstByte`, `download`, `total`; a phase that did not happen (a reused connection, an IP literal, plain HTTP) reads 0 ms (D275) | `total: {lt: 500ms}` |
| `cel.<label>` | CEL expression that must be true; variables are the roots available there (see Templates and references); write `response.body`, not `body` (D125, D150) | `has items: size(response.body.items) > 0` |
| `custom` | list of functions, `- function: path#export` | `- function: ../../hooks/checks.ts#isSorted` |
| `replace` | `true` | Drop every inherited check and keep only the ones written here (D123) | `replace: true` |

Merging (D123): checks are inherited from the api, then the test, then the step, and the more specific level wins. Each check is keyed by its kind and leaf path, such as `status`, `headers.content-type` or `body.$.id`; a check with the same key replaces the inherited one, and every other inherited check still applies. `cel` and `custom` checks are never replaced, only added. `replace: true` drops everything inherited. That is how a negative test expects 400 from an api whose normal result is 201.

```yaml
  - id: rejectsDuplicateEmail
    name: Rejects a second account with the same email
    call: users#create
    vars:
      email: taken@example.com
    expect:
      status: 409
      cel:
        names the email field: response.body.errors.exists(e, e.field == 'email')

  - id: reportsMaintenance
    name: Returns 503 during maintenance, whatever the body
    call: users#get
    vars:
      userId: u_existing
    expect:
      replace: true
      status: 503
```

The first test replaces the api's `status: 201` and keeps its other checks, and its CEL check is added to any the api has. The second checks only the status.

### CEL in one minute

CEL (Common Expression Language) is a small expression language from Google, also used for rules in Kubernetes, Firebase and Google Cloud. It reads like a JavaScript one-liner: `response.body.total > 0 && response.body.items.all(i, i.price > 0)`. It has no loops, network or file access, so an expression always finishes (an evaluation that loops over more than a million elements in all is stopped), cannot leak secrets, and is checked by `vc check` before the run: its syntax, every root it reads, and literal regexes. The same expression behaves the same in every SDK. Useful parts: `size(x)`, `x.all(i, …)`, `x.exists(i, …)`, `x.map(i, …)`, `x.filter(i, …)`, `x.sum()` (vc adds it: an `int` for whole numbers, otherwise a `double`), `has(response.body.field)`, `matches(s, regex)`, `timestamp(…)`, `duration(…)`. The same language computes values with `$cel` (see Templates and references). Anything longer than a line belongs in a `custom` function.

Matchers:

| Matcher | Passes when | Example |
| --- | --- | --- |
| literal | value equals it (deep for objects) | `role: admin` |
| `eq`, `ne` | equal, not equal | `{ ne: null }` |
| `gt`, `gte`, `lt`, `lte` | numeric, duration or size comparison | `{ gte: 1 }` |
| `between` | inclusive range `[low, high]` | `{ between: [1, 10] }` |
| `in`, `notIn` | value is (not) one of a list | `{ in: [active, pending] }` |
| `type` | `string`, `number`, `integer`, `boolean`, `object`, `array`, `null` | `{ type: string }` |
| `exists` | path is present (`true`) or absent (`false`) | `{ exists: false }` |
| `empty` | string, array or object is empty | `{ empty: false }` |
| `length` | length matches a number or matcher | `{ length: { gte: 1 } }` |
| `contains` | string has substring; array has element | `{ contains: admin }` |
| `startsWith`, `endsWith` | string prefix or suffix | `{ endsWith: .com }` |
| `regex` | string matches the pattern | `{ regex: "^[A-Z]{3}-\\d+$" }` |
| `format` | `email`, `uuid`, `uri`, `date`, `dateTime`, `ipv4` | `{ format: uuid }` |
| `every`, `some`, `none` | applies a matcher to each element of a list | `{ every: { gt: 0 } }` |
| `not` | inverts a matcher | `{ not: { contains: test } }` |
| `allOf`, `anyOf` | combines matchers | `{ anyOf: [{ eq: 0 }, { gt: 10 }] }` |

Several matchers in one object must all pass: `{ type: string, length: 32 }`. An object with no matcher key is a literal compared for equality. When its one key has five or more characters and is close to a matcher's name, case counted (`{ startWith: A }`, `{ lenght: 3 }`, `{ Length: 3 }`), it is still that literal, but `vc check` warns V3208 with the suggestion; put such an object under `eq:` to say it is meant (D276). A misspelt name next to a real matcher (`{ gt: 1, gtt: 2 }`) is V1004. A map at a status (`expect.status`, `saveOn`) takes only matchers, so any other key there is V1004, and it needs at least one (`status: {}` is V2303). A path with `[*]` yields a list, so it takes a list matcher (`every`, `some`, `none`, `length`, `contains`); a scalar matcher there (`gt`, `between`, `in`, `notIn`, `type`, `startsWith`, `endsWith`, `regex`, `format`), even inside `not`, `allOf` or `anyOf`, is V2306 with a help to put it under `every:`.

Every evaluated check is reported on its own, with its source line, expected value, actual value and a diff for objects.

## Load files

A load file turns existing tests into a traffic mix; it adds no request or check logic of its own.

| Key | Type | Default | Purpose |
| --- | --- | --- | --- |
| `id` | file ID, optional; must match the path (D139) | path-derived | Scenario ID for `vc load <id>` |
| `name` | text | — | Shown in the load report |
| `mix` | list | required | Each: `test` (ID), `weight` (integer, at least 1), optional `thresholds` for that test |
| `stages` | list, at least one | required | Each: `duration`, `users` (at least 0); users ramp from the previous stage's count |
| `thinkTime` | duration, or `min` and `max` (`min` no longer than `max`) | `0s` | Pause between iterations |
| `checks` | `api` \| `full` | `api` | `full` evaluates every test's `expect`; `api` evaluates only the apis' own `expect`, to keep the tool fast |
| `thresholds` | map of metric → matcher | — | `p50`, `p90`, `p95`, `p99`, `max`, `errorRate`, `rps`; rolling thresholds use the last 10 s; the final verdict uses the whole run |
| `abortOn` | list of metrics, each with a threshold (V2211) | `[]` | Stop early when one of these thresholds is breached on the rolling window; exit code 3 comes from the final verdict |

The metrics count one sample per iteration of a mix test, timed from its scheduled start, so a tool that falls behind cannot hide a slow server. Scenario `thresholds` cover every mix test together; a mix entry's `thresholds` cover only that test. Requests made by `beforeEach` and `before` are not samples.

| Metric | Measures |
| --- | --- |
| `p50`, `p90`, `p95`, `p99` | Latency percentile, in ms (written as a duration: `lt: 800ms`) |
| `max` | Slowest iteration, in ms |
| `errorRate` | Failed iterations ÷ iterations; errorRate takes `1%` or a share such as `0.01` |
| `rps` | Iterations per second |

Each test's iterations and errors are stored per second, as one row per test per second. A failed iteration is counted under the call that failed it, with that call's status and the failure's code (`users#list 200 V5101`), not under a `before` call or cleanup around it. A breach is reported in its metric's unit, with its limit: `breached p99 at 45.471 ms (lt 30 ms)`, `breached errorRate at 2.5% (lt 1%)`.

Lifecycle blocks work as in functional runs: `beforeRun` and the `beforeAll` of every file in the mix run once, before the first stage, and `afterAll` and `afterRun` run once, after the last stage, followed by the cleanups their files and the run registered. If a `beforeRun` or `beforeAll` action fails, the load run does not start (exit 1). `beforeEach` and `before` run in every iteration; their requests count toward the load but are reported apart from the mix. Cleanups registered during an iteration run at its end. An `abortOn` breach still runs `afterAll` and `afterRun`; Ctrl-C skips them, and the cleanups still run unless a third Ctrl-C abandons them.

```yaml
# load/checkout/peak.load.yaml
id: checkout.peak
name: Black Friday checkout mix
mix:
  - test: users.crud#getsCreatedUser
    weight: 6
  - test: catalog.browse#searchesProducts
    weight: 3
  - test: orders.checkout#placesOrder
    weight: 1
    thresholds:
      p95:
        lt: 1200ms
stages:
  - duration: 1m
    users: 500
  - duration: 5m
    users: 5000
  - duration: 1m
    users: 0
thinkTime:
  min: 500ms
  max: 2s
thresholds:
  p99:
    lt: 800ms
  errorRate:
    lt: 1%
abortOn: [errorRate]
```

## Authentication

Auth definitions live in `vibranium.config.yaml` under `auth`, are written once, and take their environment-specific values (URLs, users, tenants) from vars, so profiles switch them without duplicating anything.

| `type` | Keys | Notes |
| --- | --- | --- |
| `basic` | `username`, `password` |  |
| `bearer` | `token` | A fixed token |
| `apiKey` | `in` (`header` or `query`), `name`, `value` |  |
| `oauth2ClientCredentials` | `tokenUrl`, `clientId`, `clientSecret`, `scopes`, `audience` | Token shared for the run, refreshed before expiry |
| `oauth2Password` | `tokenUrl`, `clientId`, `clientSecret`, `username`, `password`, `scopes`, `audience` | The resource-owner password grant, for test users |
| `oauth2AuthorizationCode` | `authorizeUrl`, `tokenUrl`, `clientId`, `clientSecret`, `scopes`, `audience`, `pkce`, `redirectPort` | Opens a browser once; refresh token kept in the OS keychain |
| `oauth2Refresh` | `tokenUrl`, `clientId`, `clientSecret`, `refreshToken`, `scopes`, `audience` | For CI with a long-lived refresh token |

The `oauth2*` types post to `tokenUrl` with the client in HTTP Basic when `clientSecret` is set, and as a `client_id` form field when it is not (a public client). A refused token request fails the call with V5114, naming the endpoint's status but never its body.
| `flow` | `steps`, `apply`, `expiresIn`, `refreshBefore`, `refreshOn`, `cache` | Any sequence of requests that ends in credentials (below) |
| `plugin` | `plugin`, `config` | Custom schemes through an auth plugin: `plugin` is the ID of an `auth` plugin in `plugins:`, and `config` is passed to its `auth/credentials` call. `config` is rendered like other auth settings, so a `${secret.x}` in it is sent to the plugin whatever the lock grants (writing it there is the grant), and stays masked in output. A plugin that is not ready or answers with no values fails the call with V5114 |

`vc check` reports a definition that leaves out a key its type needs as V1005 at its `type` (`a \`bearer\` auth needs \`token\``): `username` and `password` for `basic`, `token` for `bearer`, `name` and `value` for `apiKey`, `tokenUrl` and `clientId` for every `oauth2*` type (plus `username` and `password` for `oauth2Password`, `authorizeUrl` for `oauth2AuthorizationCode` and `refreshToken` for `oauth2Refresh`), `steps` for `flow` and `plugin` for `plugin`.

Every definition can also have `vars` (its defaults), which can read each other and the use site's vars. A use site's var reads its sibling use-site vars first, then the caller's vars, then the definition's as the last fallback, so `token: ${var.token}-v2` builds on the default. A use-site var that reads its own name follows the same order: it finds the caller's `token` when the caller has one, and the definition's otherwise. Use one by ID on an api file, an api, a test, a step or an action: `auth: shopOauth`, `auth: none`, or with values:

```yaml
auth:
  id: partnerLogin
  vars:
    username: ${secret.PARTNER_ADMIN_USER}
    password: ${secret.PARTNER_ADMIN_PASSWORD}
```

### Multi-step auth with `flow`

A `flow` runs steps, then applies what they saved to every request that uses it. Each step describes its request inline with the api fields (`method`, `url`, `query`, `headers`, `body`, `expect`, `save`), so login endpoints need no api file of their own. A step that `call`s an api by ID is not supported yet. The steps run in a frame of their own that starts with the definition's `vars`; each step's `save` values are visible to the later steps, `apply` and `expiresIn`, and nowhere else.

| Key | Type | Default | Purpose |
| --- | --- | --- | --- |
| `steps` | list of steps | required | The requests that produce credentials |
| `apply.headers`, `apply.query`, `apply.cookies` | maps of templates | — | What is added to each request |
| `expiresIn` | duration (a template is fine) | never | When the credentials go stale |
| `refreshBefore` | duration | `1m` | Re-run the flow this long before expiry, capped at half the credential's lifetime (D193) |
| `refreshOn` | list of status codes | `[401]` | Re-run the flow once and retry the request when a response has one of these |
| `cache` | boolean | `false` | `true` keeps the credentials in `.vibranium/cache.db`, encrypted, and reuses them across runs until they expire (not built yet: today credentials last one run) |

```yaml
# vibranium.config.yaml (excerpt)
vars:
  identityUrl: http://localhost:8090
  partnerTenant: demo

profiles:
  - id: prod
    vars:
      identityUrl: https://id.shop.example
      partnerTenant: acme

auth:
  - id: partnerLogin
    name: Partner portal login, then token exchange
    type: flow
    cache: true
    vars:
      username: ${secret.PARTNER_USER}
      password: ${secret.PARTNER_PASSWORD}
    steps:
      - id: login
        method: POST
        url: ${var.identityUrl}/login
        body:
          username: ${var.username}
          password: ${var.password}
          tenant: ${var.partnerTenant}
        expect:
          status: 200
        save:
          sessionId: ${response.body.session.id}
      - id: token
        method: POST
        url: ${var.identityUrl}/token
        headers:
          content-type: application/x-www-form-urlencoded
        body:
          grant_type: session
          session_id: ${var.sessionId}
        save:
          accessToken: ${response.body.access_token}
          expiresIn: ${response.body.expires_in | int}
    apply:
      headers:
        authorization: Bearer ${var.accessToken}
        x-tenant-id: ${var.partnerTenant}
    expiresIn: ${var.expiresIn}s
```

The same definition serves every profile: under `--profile prod` it logs in to `id.shop.example` as tenant `acme`. Within a run the flow runs once for each set of `vars`, before the first request that needs it, and again when it expires or a response matches `refreshOn`; tests that need it at the same moment wait for that one login. If a step fails, every call that uses it fails with V5114, naming the step and its code but not its request or response. A failed flow login is kept for the run, so tests don't each retry it and lock the account. Only the built-in token types (`oauth2*`) drop a failure that may pass (network, timeout, 429, 5xx), so the next request tries again; a refusal (400, 401, 403) is kept there too. Credentials, and the token after a scheme such as `Bearer`, are redacted in all output from the moment they arrive.

## Secrets

`secrets` defaults to `provider: env` with `schema: .env.schema` (D163). Each secret is declared in `.env.schema` as a `NAME=` line (comments and `# @…` annotations are allowed), and `env` resolves the declared names from the process environment, with the project's `.env` filling the ones it does not set. `provider: varlock` is opt-in: it runs `varlock load` and takes the names from Varlock's providers (1Password, Vault, AWS, GCP, Azure, Bitwarden, Infisical, environment variables). Every declared secret without a value (unset, or blank) stops `vc run`, `vc call` and `vc load` with one V5114 that names it, before any request is sent; a `${secret.X}` that still has no value when it renders is V5114 too, and its message never shows another secret. The web UI's Settings screen edits the names in the root `.env.schema` (D247); `.env` and every other `.env.*` file stay out of reach of the editor and of agents.

`vc check` checks names against `.env.schema`: a `${secret.X}` it does not declare is V2106 (with the closest declared name as the fix), and a `${env.X}` that reads a declared secret is V2107, because it would bypass the redactor (the certain fix is `${secret.X}`).

Every `${secret.*}` value of four characters or more is redacted everywhere it appears, including inside response bodies, and shows as `••••`. So are the values of `Authorization`, `Cookie`, `Set-Cookie` and `redact.headers`, and the JSON fields named in `redact.fields`, at any depth. `redact.rules` add masks of your own, applied after those:

```yaml
redact:
  fields: [password]
  rules:
    - regex: '\d{3}-\d{2}-\d{4}'       # each match becomes ••••
    - cel: item.name == 'ssn'          # a JSON field whose rule is true is masked whole
    - function: hooks/redact.ts#maskCards   # a hook masks what it chooses
```

A `regex` rule masks every match in any text; a `cel` rule sees each JSON field as `item` (`item.name`, `item.value`); a `function` rule is a hook (`path#export`, relative to the project root) that runs within its hook budget (D185) and is given each piece of output whole, `{ text }` for a text or `{ json }` for a JSON value, and returns it masked (a string for a text). Like every hook it can read `secret(name)`; what it returns is masked again, so a secret it read never shows. Rules fail closed: a regex that does not compile masks every text (`vc check` reports such a pattern as V2311), a CEL rule that errors masks the field, and a hook that throws, overruns or returns a text that is not a string masks the whole piece.

The requests and responses kept in `history.db` and reports are masked the same way before they are stored: the URL (path and query, where a secret also counts in its percent-encoded form), every header and both bodies. Bodies are cut at `history.maxBodySize` only after masking, so a cut never leaves part of a secret showing.

## Generators and datasets

`${fake.*}` generates realistic data from the Faker catalogue, by Faker's own names; generators that take settings accept literal arguments.

| Need | Template |
| --- | --- |
| Names | `${fake.person.fullName}`, `${fake.person.firstName}`, `${fake.person.jobTitle}` |
| Contact | `${fake.internet.email}`, `${fake.phone.number}`, `${fake.internet.userName}` |
| Address | `${fake.location.streetAddress}`, `${fake.location.city}`, `${fake.location.zipCode}`, `${fake.location.country}` |
| IDs | `${fake.string.uuid}`, `${fake.string.alphanumeric(12)}`, `${fake.string.fromRegExp('[A-Z]{3}-[0-9]{4}')}` |
| Text | `${fake.lorem.word}`, `${fake.lorem.words(5)}`, `${fake.lorem.sentence}`, `${fake.lorem.paragraphs(3)}` |
| Numbers | `${fake.number.int(1, 100)}`, `${fake.number.float(0, 1)}`, `${fake.commerce.price}` |
| Dates | `${fake.date.past}`, `${fake.date.future}`, `${fake.date.recent}` |
| Choice | `${fake.helpers.arrayElement('PRIVATE', 'PUBLIC')}` |

Rules:

- Arguments are literals only: numbers, booleans and single-quoted strings; `null` and double quotes are V2305 (D151).
- A var defined as `${fake.…}` is generated once per test instance, or once per loop iteration, and every `${var.…}` read of it gets the same value: put it in `vars` once (`email: ${fake.internet.email}`) and use `${var.email}` (D131).
- A `${fake.…}` written inline in a field generates a new value every time it is rendered (D131).
- `vc run --seed 42` makes generated data repeatable; the seed is printed in every report and with every failure (D131).
- A name that is not a generator is V2002, with the close names and the list of generators. Wrong arguments are V2302: `string.alphanumeric`, `lorem.words` and `lorem.paragraphs` take one count up to 10,000 (defaults 1, 3 and 3); `number.int` takes two whole numbers, the first no larger (default 0 to 9,007,199,254,740,991); `number.float` and `commerce.price` take two numbers (defaults 0 to 1 and 1 to 1000); `helpers.arrayElement` takes one or more values; the rest take none.
- What they give: `number.int` and `number.float` are numbers, `commerce.price` is a number with two decimals, `helpers.arrayElement` is the argument it picked, and everything else is text. Dates are RFC 3339 in UTC, counted from the run's start: `date.recent` within the day before, `date.past` within the year before, `date.future` within the year after. Text is in English.
- `string.fromRegExp` (and a `fromRegExp` generator) follows a bounded subset of regex: literals, escapes (`\d`, `\w`, `\s`, `\D`, `\W`, `\S`, `\.`), `.` (printable ASCII), classes and ranges (`[A-Z0-9_]`, `[^,]`), groups (`(…)`, `(?:…)`), alternation (`a|b`), and quantifiers (`?`, `*`, `+`, `{n}`, `{n,m}`, `{n,}`). An open repeat adds at most 16 copies; a count is at most 256 and the text at most 10,000 characters; groups nest at most 32 deep; `^` and `$` only at the ends. Lookarounds, backreferences, `\b`, named groups and Unicode classes are V2309.

Named generators, for settings you reuse, are declared once in `vibranium.config.yaml` and used in the same `fake` namespace:

```yaml
generators:
  - id: teamName
    fromRegExp: "[A-Z][a-z]{4,12} Team"
  - id: visibility
    oneOf: [PRIVATE, PUBLIC]
  - id: quantity
    int:
      min: 1
      max: 5
  - id: longBio
    lorem:
      words: 500
```

Then `${fake.teamName}`, `${fake.quantity}`. Each generator has exactly one kind: `fromRegExp` (a pattern, as `string.fromRegExp`), `oneOf` (a list, as `helpers.arrayElement`), `int` (`min` and `max`, either optional, as `number.int`) or `lorem` (`words`, as `lorem.words`). None is V1005; several, or settings of the wrong shape, are V2302, once per problem (an unknown settings key such as `lorem: {wordz: 3}` is V1004). A named generator takes no arguments (`${fake.quantity(2)}` is V2302) and cannot reuse a Faker module name such as `person` (V2006).

### Datasets

A dataset is a named list of rows from your own file. Every CSV (with a header row), JSON array or YAML list in `datasets/` is a dataset named after its path, each word in camelCase, so `datasets/capitals.csv` is `capitals`, `datasets/world-cities.csv` is `worldCities` and `datasets/users/admins.json` is `users.admins`; files kept elsewhere are listed under `datasets.files` in the config, and an entry there wins over a file with the same ID. `${dataset.capitals}` is the whole list, and there are three ways to use it.

```
# datasets/capitals.csv
name,country,population
Tokyo,JP,37400000
Delhi,IN,32900000
Paris,FR,11200000
```

| You want | Write | Then read |
| --- | --- | --- |
| One test per row (data-driven test) | `examples: ${dataset.capitals}` on a test | `${example.name}` |
| One call per row (create seed data) | `forEach: ${dataset.capitals}` on an action | `${item.name}` |
| One realistic row, picked at random | `city: ${dataset.capitals \| random}` in `vars` | `${var.city.name}`, `${var.city.country}`: always the same row |

**1. A data-driven test.** The search must find every capital; each row becomes its own reported test instance.

```yaml
  - id: findsEveryCapital
    name: Search finds each capital by name
    examples: ${dataset.capitals}
    call: geo#searchCities
    vars:
      query: ${example.name}
    expect:
      body:
        match:
          results[0].country: ${example.country}
```

**2. Seed data.** The geo tests need the capitals to exist; create them once for the file, and their api's cleanup deletes them afterwards.

```yaml
beforeAll:
  - id: capitals
    call: geo#createCity
    forEach: ${dataset.capitals}
    parallel: 5
    vars:
      cityName: ${item.name}
      country: ${item.country}
      population: ${item.population | int}
```

**3. A realistic random value.** Use one row, so the name and country belong together.

```yaml
  - id: createsOfficeInACapital
    vars:
      city: ${dataset.capitals | random}
    call: offices#create
    body:
      city: ${var.city.name}
      country: ${var.city.country}
```

Rules:

- Large datasets can be trimmed with pipes: `${dataset.capitals | take(10)}` keeps the first ten rows, and `${dataset.capitals | shuffle | take(10)}` keeps ten random ones.
- `random` and `shuffle` follow `--seed`, so a run can be repeated exactly. Each test's rows draw from the seed and the test's full ID alone, so a seed gives a test the same rows whatever else the run selects, and `--failed --seed` re-runs a failed row against the same data.
- `examples: ${…}` is read once before the run starts, under the run's profiles and `--set`, so a profile can choose the rows (`examples: ${var.rows}`). It is read only for the tests the run selects and does not skip. It reads only `var`, `env` and `dataset`, as do the vars it reads, followed as the rows render them (the test's vars, its file's, then the config's): `vc check` reports V2105 for the definitions written without a profile, and a var a profile defines is checked when a run starts under that profile, with the same V2105. A var only a save sets has no value yet, so reading it is V2105 too; a read with `default` (`${var.seeded | default([])}`) gives its default instead, but a var defined in `vars` is still followed. Inside a var `examples` follows, `${secret.ROWS | default('[]')}` gives its default too, since no secret is loaded before the run; `examples: ${secret.ROWS | default('[]')}` written directly is still V2105, and a response, `run`, `item`, `iteration` or `example` read with `default` is V2105 either way, since it has no value at all before the run. A `$file` in `examples` holds only its path (any other key is V1004), written as it is; one holding a template is V2105. Rows that are not a list of maps stop the run with V5111.
- A test whose `examples` give no rows (`examples: []`, an empty file or dataset, or a template that keeps none) is skipped with the reason "examples gave no rows", and a run warns about it, so it is never passed over without a trace; `vc list` shows `examples: []` as skipped. Selecting one of its rows (`id:n`, or a `--failed` rerun) is still a selection error. `vc list` renders no rows, so it lists a test whose rows come from a template or file as one instance.
- CSV cells are text: convert with a pipe (`${item.population | int}`), or use JSON or YAML when the data has types.
- A dataset is read once per run and never changed by it.
- `examples` and `forEach` may also read a file next to the test: `examples: { $file: rows.yaml }`.
- `vc check` reads the rows of `${dataset.<id>}` and `$file` written alone: an `${example.…}` column no row has is V2103, and a `${dataset.…}` that names no dataset is V2002. A CSV record with the wrong number of cells is V1001, a column named twice V1006, and a JSON or YAML dataset that is not a list V1002. A row without a column it reads fails at run time with V5112.

```yaml
# vibranium.config.yaml: datasets kept outside datasets/
datasets:
  allowRoots: [../shared-data]
  files:
    - id: worldCities
      file: ../shared-data/world-cities.csv
    - id: smokeUsers
      file: fixtures/smoke-users.yaml
```

`file` is relative to the project root. A file outside the project must be inside one of `allowRoots` (also relative to the project root), after `..` and symbolic links are resolved, and in no hidden folder below it; any other path is V1012, so a dataset cannot read `/etc/passwd` or climb out through a link.

## Functions and request hooks

Code runs in two kinds of places. In lifecycle blocks and steps, a `function:` or `command:` action runs at that point (see Lifecycle blocks). On api files and apis, request hooks run around every request. Every entry is `function: path#exportName`, a function in a code file. The path is relative to the file that holds the entry, like a `$file` path, and ends in `.ts`, `.mts`, `.js` or `.mjs`.

`vc check` reads every hook file a `function:` reaches (D172): one that does not parse is V1013, a path to no file is V2004 (with a fix when one hook file has a close name), an export the file does not have is V2005 (with a fix when one export has a close name; a file with `export * from` is not checked), and a file that is not a hook file is V2008. Types are stripped before a hook runs; errors at run time name the TypeScript line. A hook file may import only:

- other hook files in the project, by a path starting `./` or `../`, relative to the importing file; the extension may be left off, and `./util.js` finds `util.ts`;
- `vibranium:std`, the helpers a hook may use: `hmacSha256Hex`, `sha256Hex`, `base64Encode`, `base64Decode`, `urlEncode`, `urlDecode`;
- types, with `import type { … } from "@vibranium/sdk"`, which is removed with the other types.

Packages, `node:` modules and `import()` are V2008: hooks run inside the engine, not in Node.

| Where | Key | The function receives |
| --- | --- | --- |
| api file or api | `beforeRequest` | the rendered request, which it may change and return (e.g. to sign it) |
| api file or api | `afterResponse` | the response, read-only (e.g. to log it) |
| any lifecycle block or steps | a `function:` action | vars and run info (`afterRun` also gets the results); it may return an object whose fields are saved |

Custom checks go under `expect.custom`, receive the response and resolved values, and return `true` or a failure message.

Every hook can call the global `secret(name)`, which returns a secret the run loaded (a name `.env.schema` declares); a name the run did not load throws. A value it returns is masked wherever it shows, like `${secret.X}`.

```ts
// hooks/sign.ts
import type { BeforeRequestHook } from "@vibranium/sdk";
import { hmacSha256Hex } from "vibranium:std"; // hooks run in the engine, not Node: no node: imports

export const addUploadSignature: BeforeRequestHook = ({ request }) => {
  const signature = hmacSha256Hex(secret("UPLOAD_KEY"), request.path); // `secret` is a global
  return { ...request, headers: { ...request.headers, "x-signature": signature } };
};
```

TypeScript and JavaScript functions run inside the engine; functions in other languages run through the plugin protocol. A function that throws fails its action, which is reported like any other failed action (a failure in `beforeRun` or `beforeAll` blocks the tests that need it).

## Plugins and alerts

```yaml
plugins:
  - id: acmeAuth
    source: npm:@acme/vibranium-auth@2
    config:
      realm: partners

alerts:
  - on: [failure, recovery, flaky, threshold]
    profiles: [staging, prod]
    tags: [smoke]
    notify:
      - webhook:
          url: https://hooks.example/vibranium
      - pagerduty:
          routingKey: ${secret.PD_ROUTING_KEY}
```

An alert's `on` lists what fires it: `failure` (a test failed or was blocked; the default), `blocked` (a test was blocked because a setup it needs, such as a `beforeRun` action, failed), `recovery` (a run passed after a failed one), `flaky` (a test passed only on a retry) and `threshold` (a load threshold was breached). A rule sends one notice per run, for the first of `failure`, `blocked`, `threshold`, `flaky`, `recovery` that it lists and that happened (D256). A `blocked` notice lists only the blocked tests; every other notice lists the failed and the blocked ones.

`vc plugin add` shows the permissions a plugin asks for (network hosts, secrets, files); the approved list is recorded in `vibranium.lock`. A source is `npm:<package>[@<range>]` or `path:<folder>` (relative to the project root); the plugin is installed to `.vibranium/plugins/<id>/`.

Every plugin has a `vibranium-plugin.yaml` at the top of its folder. `id` is a camelCase word, `kind` is one of `reporter`, `auth`, `importer`, `executor`, and `entry` is the program the host starts, never through a shell:

```yaml
id: htmlReport
version: 1.2.0
kind: reporter
entry:
  command: node
  args: [dist/index.js]
permissions:
  network: [reports.acme.io]
  secrets: [reportToken]
  files: [reports/]
```

`vibranium.lock` is written by `vc plugin add` and `vc plugin remove` and committed with the project. A run refuses a plugin whose installed manifest asks for more than its entry grants:

```yaml
# The plugins `vc plugin add` approved, with what each may reach. Written by vc; commit it and
# review changes to it like code. A run refuses a plugin that asks for more than this grants.
lockVersion: 1
plugins:
  - id: htmlReport
    source: npm:@acme/vibranium-html@^1
    resolved: "@acme/vibranium-html@1.2.0"
    integrity: sha512-…
    permissions:
      network: [reports.acme.io]
      secrets: [reportToken]
      files: [reports/]
```

`integrity` is npm's hash for an `npm:` source and a `blake3-…` hash over the folder's files for a `path:` one. A run checks a `blake3-…` hash against the installed files and refuses the plugin (V5118) when they changed.

A plugin's `config` holds literal values only: a `${…}` in it makes the plugin unusable for the run, with a notice. `vc check` gives a V5120 warning for it, and for an entry the lock does not approve, that is not installed or that asks for more than the lock grants; a `reports.formats` name that is an installed plugin other than a reporter is V2302. A plugin reads a secret with `secrets/get`, and gets only those its lock entry grants. The one other way a secret reaches a plugin is a `${secret.x}` in the `config` of a `type: plugin` auth definition, which the author writes on purpose. A reporter plugin runs when its ID is in `reports.formats`, and an auth plugin when an auth definition of `type: plugin` names it:

```yaml
reports:
  formats: [ctrf, htmlReport]      # a reporter plugin, by its ID

auth:
  - id: partnerSso
    type: plugin
    plugin: acmeAuth
    config: {tenant: eu}
```

Each plugin runs as one process per run, started the first time the run uses it; a plugin that fails or is not ready fails only the work that used it. `plugin-protocol.md` is the contract a plugin author writes against.

Alert channels: `webhook`, `slack`, `teams`, `pagerduty`, `email`.

## v1 to v2

Every duplicate or inconsistent construct in v1 has exactly one replacement; `vc migrate v1` performs these conversions and lists anything it could not convert (JavaScript asserts beyond the common shapes and `scripts`) in its report, with a `# TODO(vc migrate)` comment in the output (D200). It inlines a v1 `schema` reference as `expect.body.schema`, and gives a var that only a v1 script set an empty value with a TODO; a dropped `delay`, `async`, `timing.wait` or repeat delay is listed as a note.

| v1 construct | Problem | v2 replacement |
| --- | --- | --- |
| `{var}`, `{dataset.names}`, `{lorem_500}`, `[PRIVATE\|PUBLIC]`, `[a-z]{10}` | Five template syntaxes, some colliding with regex | `${var.x}`, a named `fromRegExp` generator `[a-z]{500}` (v1 counted characters), `${fake.helpers.arrayElement('PRIVATE', 'PUBLIC')}` (v1 read this as a character class), a Faker call for each built-in v1 dataset, named `generators` (D200) |
| `"!userSchemas/allUsers"` | Sigil inside a string | `$file: schemas/all-users.json` |
| `repeat-delay`, `before-endpoint`, `firstByte`, `max_parallel_executors`, `created_on` | Three casing styles | camelCase everywhere |
| `tags: "a,b,c"` | List hidden in a string | `tags: [a, b, c]` |
| Collection folder + scenario file + `endpoints` array | Position decides identity | File `id` + item `id`: `users.crud#getsUser` |
| `name: "01.get_all_teams"` | Order and identity mixed into one name | `id` for references, `name` for people; list order is run order |
| `dependencies: [{ api, scenario, collection, variable }]` | Re-ran the whole dependency, assertions included, on every use | `before`, `beforeEach`, `beforeAll` or `beforeRun` with `call: <api>`: only the api runs, with its own checks, once per block |
| `variable: "response.id"` vs `variable: { id: "all.id" }` | String or map; paths with or without `response.` | `save: {id: ${response.body.id}}`, then `${var.id}` |
| `variables`, `generate`, dependency `variables` | One word, three meanings | `vars` everywhere, with one lookup order; `generators` and `datasets` for fake data |
| `{response.all.id}`, `any`, `any_n` | A custom path language | Paths with `[*]` plus `every`, `some`, `none`, or CEL |
| `expect.response: { "desc": "JS expression" }`, `schema` mixed in | JavaScript strings run through vm2 | `expect.body.match`, `expect.body.schema`, `expect.cel.<label>`, `expect.custom` |
| `timing: { total: "<700" }`, `delay: 500`, `timeout: 120000` | Unitless numbers, comparison strings | `lt: 700ms`, `500ms`, `2m` |
| `status: 200` only | No ranges or alternatives | `status: 2xx`, `[200, 204]` or a matcher |
| `cache: true` | Cache ignored input values | A `beforeAll` call with `save` (one response per run, as in v1, D200); `cache: {for: 12h}` on a lifecycle action for reuse across runs, keyed by api, vars and profile |
| `async: true` | Meant "run repeats in parallel" | `parallel: N` on a looped action; the scheduler and `defaults.concurrency` for tests |
| `repeat-until` | Separate loop construct | `poll` with `until`, `every`, `timeout` |
| `ignore: true` | No reason recorded | `skip: "reason"` |
| `system`, `accounts`, `default_account` | Three names for credentials, copied per environment | `auth` definitions in the config, values switched by profiles |
| `language: "en"` | Special case for one header | `headers: {accept-language: en}` |
| `payload` | Unclear for non-JSON bodies | One `body`: JSON, text, `$file`, `$base64`, `$extend`; encoding follows `content-type` |
| Eight `scripts` lifecycle points as JavaScript strings | Code in strings | Lifecycle blocks at run, file and test level with `function` or `command` actions; `beforeRequest` and `afterResponse` on apis |
| `_range_index` | Magic variable | `${iteration.index}` |
| `id`, `email`, `created_on`, `app`, `author` in scenario files | Metadata nobody kept current | Removed; git history has it |
| `~/.vib/config.json` pointing at one workspace | Global config, painful in CI | `vibranium.config.yaml` at the project root, with at least one profile |

## Complete example project

This small shop project uses every file kind; the interface mockups show the same project.

```
shop/
  vibranium.config.yaml        # vars, profiles, auth, beforeRun (shown above)
  .env.schema                  # secret names (NAME= lines)
  openapi/
    users.yaml
    orders.yaml
  schemas/
    user.json
    order.json
  datasets/
    capitals.csv
  hooks/
    checks.ts
    notify.ts
    sign.ts
  services/
    shared/flows.test.yaml     # verifiedMember (shown above)
    users/
      users.api.yaml           # shown above
      crud.test.yaml           # shown above
    geo/
      geo.api.yaml             # shown above
      groups.test.yaml         # shown above
    catalog/catalog.api.yaml
    carts/carts.api.yaml
    orders/
      orders.api.yaml
      checkout.test.yaml
  load/
    checkout/peak.load.yaml    # shown above
  .vibranium/                  # reports, history.db, cache.db; not committed
```

```yaml
# services/orders/orders.api.yaml
id: orders
name: Orders service
baseUrl: ${var.ordersUrl}/api/v1
auth: shopOauth
source:
  openapi: https://orders.staging.shop.example/v3/api-docs

apis:
  - id: place
    name: Place an order
    method: POST
    path: /orders
    vars:
      paymentToken: tok_visa
    body: {"cartId": "${var.cartId}", "payment": {"token": "${var.paymentToken}"}}
    expect:
      status: 201
    save:
      orderId: ${response.body.id}
      orderTotal: ${response.body.total}
    cleanup:
      call: cancel
      vars:
        orderId: ${response.body.id}

  - id: get
    name: Get an order
    path: /orders/${var.orderId}

  - id: cancel
    name: Cancel an order
    method: POST
    path: /orders/${var.orderId}/cancel
    expect:
      status: [200, 409]

  - id: refund
    name: Refund an order (admins only)
    method: POST
    path: /orders/${var.orderId}/refund
```

```yaml
# services/orders/checkout.test.yaml
id: orders.checkout
name: Checkout
tags: [orders, checkout]

beforeAll:
  - id: catalog                 # shared by the file: read as ${var.catalog.*}
    call: catalog#seedProducts
    cache:
      for: 12h
      check:
        call: catalog#getProduct
        vars:
          productId: ${var.catalog.productIds[0]}

beforeEach:                     # every test gets its own buyer and cart
  - call: users#create
  - call: carts#create
  - call: carts#addItem
    forEach: ${var.catalog.productIds}
    vars:
      sku: ${item}

tests:
  - id: placesOrder
    name: A cart with two items becomes a paid order
    steps:
      - id: place
        call: orders#place
      - id: fetch
        call: orders#get
        poll:
          until:
            body:
              match:
                status: paid
          every: 1s
          timeout: 30s
        expect:
          status: 200
          body:
            schema:
              $file: ../../schemas/order.json
            match:
              items:
                length: 2
              items[*].quantity:
                every:
                  gte: 1
              total: ${var.orderTotal}
          cel:
            total equals sum of lines: response.body.total == response.body.items.map(i, i.price * i.quantity).sum()
          custom:
            - function: ../../hooks/checks.ts#currencyIsConsistent

  - id: rejectsDeclinedCard
    name: A declined card returns 402
    call: orders#place
    vars:
      paymentToken: tok_chargeDeclined
    expect:
      status: 402
      body:
        match:
          error.code: card_declined

  - id: memberCannotRefund
    name: A member cannot refund an order
    call: orders#refund
    auth:
      id: partnerLogin
      vars:
        username: ${secret.MEMBER_USER}
        password: ${secret.MEMBER_PASSWORD}
    vars:
      orderId: ord_existing
    expect:
      status: 403
```

After each test, its order is cancelled, then its cart and buyer are deleted, all by the apis' own cleanups in reverse order; the seeded catalog stays for 12 hours and is reused by later runs while `catalog#getProduct` still finds it.

```ts
// hooks/checks.ts
import type { CustomCheck } from "@vibranium/sdk";

export const currencyIsConsistent: CustomCheck = ({ response }) => {
  const currencies = new Set(response.body.items.map((item: { currency: string }) => item.currency));
  return currencies.size === 1 || `mixed currencies: ${[...currencies].join(", ")}`;
};
```
