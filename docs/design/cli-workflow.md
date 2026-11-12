# Vibranium v2 — CLI and Developer Workflow

Oct 23, 2026 · @Sarath

This is how a developer works with Vibranium day to day from the terminal: starting a project, adding apis and tests, keeping them in step with OpenAPI, and running exactly the tests they want. The command is `vibranium`, with the short alias `vc`. The file formats themselves are in the Vibranium v2 Syntax Reference; terminal mockups of every flow below are on the **CLI** page of the Vibranium v2 Interface Mockups, next to the existing **Terminal UI**, **Web UI** and **HTML report** pages.

## Everyday workflows

| You want to | Commands | What you end up with |
| --- | --- | --- |
| Start from an existing OpenAPI spec | `vc init`, then `vc import openapi <url-or-file> --id users --tests` | A config with a `default` profile, an api file with one api per operation, a committed spec snapshot, and one passing happy-path test per api |
| Add an endpoint the spec doesn't have yet | `vc new api users#deactivate --method POST --path /users/{id}/deactivate`, or `vc new api --from-curl '<curl command>'` | A new api appended to the right api file, path parameters turned into vars |
| Write tests for an api | `vc new test users.crud#deactivatesUser --from users#deactivate --negative`, or `vc call users#deactivate --var id=… --save-as-test users.crud#deactivatesUser` | Generated tests you then edit, or a test captured from a real call |
| Run while you edit | `vc watch users.crud`, `vc run users.crud#getsCreatedUser`, `vc run --failed` | Fast feedback on just the tests you are touching |
| Keep in step with the backend | `vc sync users`; in CI, `vc sync --check` | Spec changes merged into api files without losing your edits; CI fails when the spec drifts |
| Park a flaky test | `vc skip orders.checkout#appliesCoupon --reason "…" --until 2026-11-15` | The test is reported as skipped with its reason, and runs again after the date |

Every command that writes a file compiles the result first and refuses to write something `vc check` would reject, so generated files always load. The commands that print data (`check`, `plan`, `list`, `history`, `cache list`, `load`, `new auth`) take `--json` for scripts and agents; `vc run` takes `--reporter json`, with `--json` as its alias (D287, D264). `--profile` is taken where a profile matters.

## What `vc init` writes

`vc init` creates a project that runs green straight away and doubles as a worked example. Every file starts with a documentation header, and every value is a sensible default you can change:

```
shop/
  vibranium.config.yaml              project settings with defaults, one profile
  .env.schema                        secret names (none yet), one NAME= line each
  .gitignore                         adds .vibranium/ and .env
  services/sample/sample.api.yaml    four reference apis, annotated
  services/sample/basics.test.yaml   four reference tests, annotated
  schemas/  datasets/  hooks/  openapi/  load/   empty, ready to use
```

The sample calls a small demo service built into Vibranium (`vc demo`: a users endpoint and an echo endpoint on `localhost:7421`), which the sample file starts and stops itself. The sample works offline and never depends on a third-party site. Delete `services/sample/` when you have your own apis, or skip it with `vc init --no-sample`, whose config then has no `sampleUrl` var. Every file `vc init` writes is already in the house style, so `vc fmt` on a fresh project changes nothing.

### The documentation header

Every file Vibranium creates (`vc init`, `vc new`, imports, and the TUI and web UI creators) starts with the same header:

- **Schema line.** The first line points editors at the JSON Schema for that file kind, so VS Code, JetBrains IDEs and Neovim complete keys and flag mistakes. The schemas are also registered with SchemaStore, so editors that use it recognise `*.api.yaml` and `*.test.yaml` even without the line.
- **Description.** A short block says what the file is for and how its items are referenced.
- **Links and commands.** The reference section for that file kind, and the commands to try it.

How much more is written is a personal preference in `~/.config/vibranium/config.yaml` (`new.comments`), or per command with `--comments`:

| Level | Writes | Default for |
| --- | --- | --- |
| `full` | Header, a comment above each item, every optional key shown commented out | `vc init` samples |
| `brief` | Header (when the file is new) and one comment line per new item | `vc new`, imports, UI creators |
| `none` | The schema line only | — |

`vc fmt` keeps comments; `vc fmt --strip-comments` removes the generated ones and keeps yours.

### `vibranium.config.yaml`

```yaml
# yaml-language-server: $schema=https://v007m.github.io/vc-rs/schema/config.schema.json
# ─ Project config ─────────────────────────────────────────────
# One per project, at the root: profiles, project-wide vars, auth,
# secrets and run settings. Only syntaxVersion and one profile are required.
# Reference: https://github.com/v007m/vc-rs/blob/master/docs/design/syntax-reference.md#project-config
# See where a value comes from: vc config explain defaults.timeout
# ─────────────────────────────────────────────────────────────
syntaxVersion: 2
project: shop

defaults:
  timeout: 30s            # per request
  concurrency: 16         # test instances running at once
  perHost: 8              # open connections per host
  headers:
    accept: application/json
    user-agent: vibranium/2

vars:
  sampleUrl: http://localhost:7421    # the demo service used by services/sample

profiles:
  - id: default
    default: true
    # vars:                # override vars for this profile
    #   sampleUrl: https://sample.staging.example
  # - id: staging          # add one per environment; select with --profile staging

# auth:                    # how requests prove who they are
#   - id: myOauth
#     type: oauth2ClientCredentials
#     tokenUrl: ${var.identityUrl}/oauth/token
#     clientId: ${secret.CLIENT_ID}
#     clientSecret: ${secret.CLIENT_SECRET}

secrets:
  provider: env            # environment and .env; varlock adds 1Password, Vault, AWS and more
  schema: .env.schema      # declares the secret names

reports:
  dir: .vibranium/reports
  formats:
    - ctrf
    - junit
    - html

history:
  bodies: failures         # store request/response bodies for failures only
  maxBodySize: 4kb         # cut longer bodies (default 64kb)
  keep: 50                 # runs kept after each run, or `all`
```

Any value here can be set from several places. Layers apply lowest to highest: built-in defaults < user config (`~/.config/vibranium/config.yaml`) < `vibranium.config.yaml` < the active profiles, in the order given < `.env` < `VIB_*` environment variables < `--set key=value` on the command line, so `vc run --set defaults.timeout=60s` wins over every other config layer for one run. `--set vars.<name>` sets a config var, so a file's, test's or step's own `vars` still win over it (D287). A setting's `VIB_*` variable is its `--set` key upper-snaked (`VIB_DEFAULTS_PER_HOST=4`, `VIB_DEFAULTS_RETRY_ATTEMPTS=1`, `VIB_REPORTS_DIR=out`), and a var's is `VIB_VAR_<name>` with the name as written (`VIB_VAR_userId=7`, D182). The user config is `$XDG_CONFIG_HOME/vibranium/config.yaml` (else `~/.config/…`, or `%APPDATA%` on Windows); its `profile`, `defaults`, `reports` and `vars` count only where the project's config is silent. `vc config explain <key>` shows the value and the layer that set it (D129); a secret's value is shown as `••••` (D232).

### `services/sample/sample.api.yaml`

```yaml
# yaml-language-server: $schema=https://v007m.github.io/vc-rs/schema/api.schema.json
# ─ Api file: sample ───────────────────────────────────────────
# How to call one service. Tests call each api by its full ID, sample#<id>.
# Safe to delete once you have your own api files.
# Reference: https://github.com/v007m/vc-rs/blob/master/docs/design/syntax-reference.md#api-files
# Try one: vc call sample#listUsers
# ─────────────────────────────────────────────────────────────
id: sample
name: Demo service built into Vibranium
baseUrl: ${var.sampleUrl}
# auth: myOauth            # an auth ID from vibranium.config.yaml, or none
# headers: {}              # sent with every api in this file

apis:
  # GET /users: the simplest api is a method and a path.
  - id: listUsers
    name: List users
    method: GET
    path: /users
    # query:               # query string; values can be templates
    #   limit: ${var.pageSize}

  # POST /users: vars are the api's inputs, with defaults any caller can override.
  - id: createUser
    name: Create a user
    method: POST
    path: /users
    vars:
      name: ${fake.person.fullName}    # a new realistic value on every call
      email: ${fake.internet.email}
    body:
      name: ${var.name}
      email: ${var.email}
    expect:                            # checks that hold on every call of this api
      status: 201
    save:                              # values every caller gets as vars
      userId: ${response.body.id}
    cleanup:                           # undoes a successful call, automatically
      call: deleteUser
      vars:
        userId: ${response.body.id}

  - id: getUser
    name: Get a user
    path: /users/${var.userId}

  - id: deleteUser
    name: Delete a user
    method: DELETE
    path: /users/${var.userId}
    expect:
      status:                          # already gone is fine for a cleanup
        - 204
        - 404
```

### `services/sample/basics.test.yaml`

```yaml
# yaml-language-server: $schema=https://v007m.github.io/vc-rs/schema/test.schema.json
# ─ Test file: sample.basics ───────────────────────────────────
# Tests call apis and state what they expect.
# Run this file: vc run sample.basics    One test: vc run sample.basics#createsUser
# Reference: https://github.com/v007m/vc-rs/blob/master/docs/design/syntax-reference.md#tests
# ─────────────────────────────────────────────────────────────
id: sample.basics
name: Sample tests
tags:
  - sample

beforeAll:                 # once, before this file's tests
  - command: vc demo start --port 7421 --detach
afterAll:                  # once, after them
  - command: vc demo stop --port 7421

tests:
  # The smallest test: call an api and check the status.
  - id: listsUsers
    name: Lists users
    call: sample#listUsers
    expect:
      status: 200

  # Checks on the body. The api's own check (status 201) applies too,
  # and createUser's cleanup deletes the user afterwards.
  - id: createsUser
    name: Creates a user with the given email
    call: sample#createUser
    vars:
      email: ada@example.com
    expect:
      body:
        match:
          email: ada@example.com
          id:
            type: string

  # Data from setup: before runs createUser, which saves userId.
  - id: getsCreatedUser
    name: Returns the user that was just created
    before:
      - call: sample#createUser
    call: sample#getUser
    expect:
      status: 200
      body:
        match:
          id: ${var.userId}
      timing:
        total:
          lt: 500ms

  # A negative test: stating a status replaces the api's expected 201.
  - id: rejectsMissingEmail
    name: Rejects a user without an email
    call: sample#createUser
    body:
      name: No Email
    expect:
      status: 400
```

`rejectsMissingEmail` expects 400 although `sample#createUser` expects 201: expect checks merge by kind and leaf path, and the more specific level replaces the other (D123).

With `brief` comments, `vc new api users#deactivate` adds a single line above the new item:

```yaml
  # users#deactivate · POST /users/{id}/deactivate · keys: https://github.com/v007m/vc-rs/blob/master/docs/design/syntax-reference.md#api-files
  - id: deactivate
    method: POST
    path: /users/${var.id}/deactivate
```

## Project layout

Only `vibranium.config.yaml` is required, and Vibranium finds every other file by its suffix, wherever it sits. The folders below are the conventions `vc init` creates so a project reads the same everywhere; each is optional and can be renamed under `paths` in the config.

&#91;embedded content: project layout · what points at what\]

Tests and apis are the definitions; everything else is something they point at.

| Folder or file | Holds | Used from | Example |
| --- | --- | --- | --- |
| `vibranium.config.yaml` | Profiles, project vars, auth, secrets, generators, datasets list, reports, history, alerts, `beforeRun` and `afterRun` | Every file: vars and profiles apply everywhere, auth is named by ID | `auth: shopOauth` in an api file |
| `services/<name>/*.api.yaml` | How to call each endpoint, its always-true checks, what it saves and how to undo it | Tests and lifecycle blocks, by api ID | `call: users#create` |
| `services/<name>/*.test.yaml` | Tests, plus `beforeAll`/`beforeEach` blocks around them | `vc run`, load files, `test:` actions | `vc run users.crud` |
| `load/*.load.yaml` | A weighted mix of existing tests, stages and thresholds | `vc load` | `vc load checkout.peak` |
| `schemas/` | JSON Schemas for response bodies | `expect.body.schema` in apis and tests | `schema: {$file: ../../schemas/user.json}` |
| `datasets/` | Your own rows of data: CSV, JSON or YAML | `examples`, `forEach` and `vars`, by dataset name | `examples: ${dataset.capitals}` |
| `hooks/` | Code: lifecycle functions, request signing, custom checks | `function: path#export` | `function: ../../hooks/sign.ts#addUploadSignature` |
| `openapi/` | Snapshots of each spec as it was at the last import or sync; written only by `vc import` and `vc sync` | `vc sync`, as the merge base | `openapi/users.yaml` |
| `.env.schema` | Secret names (`NAME=` lines), and where each comes from when Varlock is the provider | `${secret.X}` anywhere | `${secret.SHOP_CLIENT_SECRET}` |
| `vibranium.lock` | Approved plugin permissions | `vc plugin add` | — |
| `.vibranium/` | Reports, `history.db`, `cache.db`; generated, git-ignored | `vc report`, `vc history`, the TUI and web UI | `.vibranium/reports/report.html` |

Paths in `$file` and `function:` are relative to the file that contains them, so moving a folder of api and test files keeps working as long as its references move with it; `vc check` reports any that break.

### Naming conventions

Every way of creating something (`vc new`, `vc import`, the terminal UI and the web UI) follows these conventions, so generated files double as examples of good practice. Each creator derives the ID from the name you type, shows the file it will write, and refuses an invalid ID with a suggested one. `vc check` warns about existing names that break a convention; `vc check --fix` renames them and updates every reference, and `vc rename <old> <new>` does the same on demand.

| Thing | Convention | Example |
| --- | --- | --- |
| Api file | `services/<service>/<service>.api.yaml`; file ID = the service, lowercase (derived from the path, D139) | `services/users/users.api.yaml`, ID `users` |
| Test file | `services/<service>/<area>.test.yaml`; file ID `<service>.<area>`; deeper folders add words (`services/payments/refunds/partial.test.yaml` → `payments.refunds.partial`). The ID is derived from the path; an `id:` key is optional and must match it (V2007, D139) | `services/orders/checkout.test.yaml`, ID `orders.checkout` |
| Load file | `load/<area>/<scenario>.load.yaml`; ID `<area>.<scenario>` | `load/checkout/peak.load.yaml`, ID `checkout.peak` |
| Api ID | camelCase, verb first, no service prefix (the file ID supplies it) | `create`, `startExport` → `users#startExport` |
| Full ID | `<fileId>#<id>`; a bare dotted word is always a file ID; inside the same file the short `id` works (D138) | `users.crud#getsCreatedUser`, `users#create` |
| Test ID | camelCase behaviour in the third person, subject implied | `createsMember`, `rejectsDeclinedCard` |
| Test name | A plain sentence; the ID is derived from it | "Rejects a declined card" → `rejectsDeclinedCard` |
| Vars and saved values | camelCase nouns; IDs end in `Id`, URLs in `Url` | `userId`, `usersUrl`, `orderTotal` |
| Profiles | Lowercase single words | `dev`, `staging`, `prod`, `eu` |
| Auth IDs | camelCase nouns | `shopOauth`, `partnerLogin` |
| Tags | Lowercase; kebab-case for two words | `smoke`, `slow-network` |
| Datasets | `datasets/<kebab-name>.csv`; ID = the stem in camelCase | `world-cities.csv` → `worldCities` |
| Schemas | `schemas/<service>/<thing>.json`, kebab-case | `schemas/orders/order.json` |
| Hooks | `hooks/<purpose>.ts`, camelCase exports | `hooks/sign.ts#addUploadSignature` |

One service per folder keeps an api file next to the tests that use it, and test files split by area (`checkout`, `history`, `coupons`) stay small enough to read in one sitting. Lifecycle blocks that several files need belong in the config's `beforeRun`; a multi-step flow reused across files goes in `services/shared/flows.test.yaml` with `standalone: false`.

## Commands

Commands are grouped by the job they do. `vc help <command>` shows every flag with an example; `--json` prints structured output instead of text where a command takes it, and `vc run --reporter json` (or its alias `--json`) streams JSON events. `vc import` reads `openapi`, `curl`, `har`, `http`, `postman` and `bruno`; another kind exits 2, naming the importers there are.

**Start and author**

| Command | Does |
| --- | --- |
| `vc init [--no-sample] [--from-openapi <url-or-file>]` | Creates `vibranium.config.yaml` with a `default` profile, the sample api and test files (not with `--no-sample`), `.env.schema` when there is none, the conventional folders, `.gitignore` lines for `.vibranium/` and `.env` that it lacks (D286), and the editor schema hint; with `--from-openapi`, then runs `vc import openapi` on it (when the import is refused, the project stays written and the import can be run again). A definition file it would write that exists already (a project is here) is named and nothing is written: exit 2 |
| `vc import openapi <url-or-file> [--id <fileId>] [--tests] [--negative] [--auth] [--include <tag-or-path>]… [--only <operationId>]…` | Creates an api file from a spec (one api per operation), saves the spec snapshot `openapi/<fileId>.yaml`, adds a base-URL var to the config, and optionally generates tests in `<fileId>.openapi` (`services/<fileId>/openapi.test.yaml`). Without `--id` the file ID is the spec's file name up to its first dot. `--include` and `--only` are kept in the file's `source`, so `vc sync` takes the same operations (D277). What the spec holds that has no api form is listed after the change. An api file that exists is refused (use `vc sync`), and so is a spec read from a snapshot path `openapi/<id>.yaml` (keep specs elsewhere, such as `specs/`), a snapshot path another api file's `source.openapi` names, a snapshot path that leads to another file (a link, or a file named in another case on a file system that ignores case), and a result that would not compile: exit 2, nothing written |
| `vc import curl <file> [--id <fileId>]` | Creates an api file from a file of curl commands, as browser dev tools and API docs copy them: one api per distinct method and path, all calling one origin, which becomes a base-URL var. Without `--id` the commands go to the api file whose base URL renders to that origin, if one does; with it, to the file `--id` names, created when it does not exist; `--id` for commands calling several origins is refused, naming them (exit 2), as one api file has one base URL. IDs in a path become vars; JSON body fields become api vars with the command's values as defaults; form and `-F` data become body maps. A credential (`authorization` and other secret-named headers, cookies, `-u`, a URL's user, secret-named query and body fields) is never written: the notes after the change name each one left out, without its value |
| `vc import har <file> [--flow <fileId#testId>] [--include-host <host>]… [--keep-static] [--profile <ids>] [--dry-run]` | Turns a browser recording into apis in the api files whose base URLs its hosts render to (new files for unknown hosts), credentials dropped and secrets made `${secret.…}` references, plus with `--flow` a test replaying it (see Importing from a HAR file). `--id` puts a recording of one origin in the file it names; for a recording of several origins it is refused, naming them (exit 2). `--dry-run` prints the plan and the diffs without writing. A result that would not compile is refused: exit 2, nothing written |
| `vc import http <file> [--id <fileId>]` | Creates an api file from a `.http` file (VS Code REST Client, JetBrains HTTP Client) by the curl rules: one api per request, named by its `###` title or `# @name`. File variables are written as their values, except one named like a credential, which stays a var; `{{$guid}}` and `{{$randomInt a b}}` become fake data; a variable the file does not define, or another request's response, becomes an empty var with a note. Response handlers and bodies read from files are not imported |
| `vc import postman <file> [--flow <fileId#testId>] [--profile <ids>] [--dry-run]` | Turns a Postman collection (v2.1 export) into apis in the api files whose base URLs its URLs render to (new files for unknown hosts), one api per request named after it. `{{variables}}` become `${var.…}` with the collection's values as defaults, Postman's dynamic variables (`{{$guid}}`) the matching `${fake.…}`, and a base URL without a value `http://localhost`, noted. Auth and credential headers are never written; secret-named or random values become `${secret.…}` references; other headers and scripts are noted, not imported. With `--flow`, a test calling the requests in collection order, saving what their test scripts set from the response (`pm.environment.set("cartId", pm.response.json().id)`). `--dry-run` as for `har` |
| `vc import bruno <path> [--flow <fileId#testId>] [--profile <ids>] [--dry-run]` | Turns a Bruno collection, its folder (the one with its `bruno.json`, D254) or the collection exported from Bruno as one JSON file (D209), into apis by the `postman` rules; both draft the same. The vars default to the collection's vars and its first environment's values (other environments are noted); an environment variable marked secret and `{{process.env.NAME}}` become `${secret.…}` references. Auth inherits from folder and collection; scripts, tests, assertions and request vars are noted, not imported. With `--flow`, each step saves the post-response vars that read the response body (`cartId: res.body.id`). The folder's `bruno.json` `ignore` paths are not read; a folder without `bruno.json` exits 2 |
| `vc new api <fileId#apiId> [--method] [--path \| --url <full-url>] [--from-curl '<curl>']` | Appends an api to the right api file (creating the file if needed); with `--url`, the origin becomes a base-URL var and the rest the path and query (D177); with `--from-curl '<curl>'` (M6, D176), the method, URL, query, content type and body come from one curl command and no credential is written |
| `vc new test <fileId#testId> --from <apiId> [--negative] [--auth]` | Generates tests for an api (see Creating apis and tests) |
| `vc new auth <id> --type <type>` | Adds an auth definition to the config, prompting for its fields; a credential the type needs that is left out (`password`, `token`, `value`, `refreshToken`) is written as `${secret.<ID>_<FIELD>}` (`${secret.PARTNER_TOKEN_TOKEN}`) |
| `vc new file api\|test\|load <fileId>` | Creates an empty file of that kind with its header filled in; a load file also gets a `mix` of the project's first test by ID and one stage (`30s`, 1 user), so it checks. With no test in the project, exit 2 |
| `vc call <apiId> [--var k=v] [--keep] [--save-as-test <testId> [--expect-field body.<path>]...]` | Calls one api ad hoc with project auth and the active profile (`--profile`, `--set` as for `vc run`), then runs the api's cleanup unless `--keep`; `beforeRun` and `beforeAll` are skipped and the call is stored in the history as `adhoc`. Exits 1 when the api's `expect` fails, 2 for an unknown api (with the closest) or a `--var` that is not `name=value`. Optionally saves the call as a test expecting its status and the named body fields (a missing field exits 2 and writes nothing) |
| `vc fmt [--strip-comments] [paths]` | Rewrites definition files in the house style (block YAML, key order, quoting) and keeps comments; `--strip-comments` drops the one-line comments `vc new` wrote above its items. Refuses, writing nothing, a file that does not parse or that formatting would change in meaning; exits 2 then, 0 otherwise |
| `vc rename <oldId> <newId>` | Renames an api, test, file, var, auth or dataset ID and updates every reference; moves the file when a file ID changes. A var, auth or dataset is written with its kind: `vc rename var.userId var.accountId`, `auth.partnerToken`, `dataset.capitals`; a dataset named by its path under `datasets/` is renamed by moving its file. The comments `vc new` wrote follow the new ID; other comments are left alone |

**Check and explore**

| Command | Does |
| --- | --- |
| `vc check [--json \| --fix [--yes]] [paths...]` | Compiles and links everything; prints errors and warnings with file, line and column, then a summary (`1 error · 0 warnings · 3 files checked`); exits 2 on errors, 0 otherwise. With paths (files or folders, relative to the working folder), reports only on the files under them and on problems elsewhere that point into them, such as a var left unset in an api that a test there calls (V2101); the whole project is still compiled, so references into other files are checked. `--json` prints the diagnostics as JSON on stdout instead. With `--fix`, prints each certain fix as a unified diff, asks `Apply N fixes? [y/N]` (`--yes` skips the question, for scripts and agents) and on yes writes them and checks again; the exit code is that of the final check. Nothing is written when you decline, when there are no certain fixes, or when the fixes would add an error. Run anywhere inside a project: it looks up from the working folder for `vibranium.config.yaml` |
| `vc list apis\|tests\|files [selector] [--skipped] [--tag] [--json]` | Lists definitions: one line per item (full ID, name, tags, `(skipped: <reason>, until <date>)`, or `(skip ended <date>: <reason>)` for a skip past its `until`, which runs again), or `--json` (`id`, `name`, `path`, `tags`, `skipped`, `skipExpired`, `skipReason`, `skipUntil`) for scripts and agents (D130). `tests` takes the selectors and flags of `vc run` and lists the skipped tests too; `--skipped` lists only those, and the tests whose skip has ended, so a stale `skip` can be found and removed. `apis` take the same positional selectors, `--grep` and `--changed` (an api file ID such as `users` names its apis; `-t` is refused, as apis have no tags), and `files` take them all, a file's tags being its test file's |
| `vc explain <id> [--profile] [--set]` | Shows one api or test fully resolved for the active profile: final URL with known vars piped, vars, merged headers and auth with where each came from, merged `expect` with the values it checks (and `status: 2xx  (default)` when none checks the status), values earlier calls save, the lifecycle actions it triggers and the cleanups it registers. An unknown ID exits 2 and names the closest |
| `vc plan [selector] [--dot] [--json]` | Shows what a run would do, in order, without sending a request: the plan line, then a tree of lifecycle blocks and test instances, each labelled as `vc run` labels it (`users.pages#listsEachPage › first page`; a row a template draws by its number, D316). Takes the selectors, `--profile`, `--set` and `--repeat` of `vc run`; `--json` for scripts and agents (D130), `--dot` for a Graphviz graph: a cluster per file, a node per lifecycle action and instance, dashed edges for the order they run in and solid ones from a node to each `beforeRun` or `beforeAll` action whose saved values it reads |
| `vc explain <code>` | Explains a diagnostic code (V2003, V2101, …; a lowercase `v2101` too) with a wrong and a right example. The pages are built into the binary, so it works outside a project; an unknown code exits 2 |

**Run**

| Command | Does |
| --- | --- |
| `vc run [selector...]` | Runs tests (see Selecting what to run) |
| `vc watch [selector...]` | Runs the selection, then re-runs the affected tests 200 ms after a definition file, `.env` or `.env.schema` is saved. Takes `vc run`'s flags; a flag value the first run refuses (`--profile nope`) ends it with exit 2, since no save can fix it; Ctrl-C ends it with exit 130 |
| `vc load <scenarioId>` | Runs a load scenario: `beforeRun` and `beforeAll` once, the stages with a live line each second on stderr (its rps over the last 3 s), `afterAll` and `afterRun` once, then the summary: latency p50/p90/p95/p99/max, each breach in its unit with its limit (`breached p99 at 45.471 ms (lt 30 ms)`) and each error kind by the api that failed, its status and code (`errors users#list 200 V5101 × 12`); `--json` for the report as JSON, each breach as `{metric, test, actual, unit, limit}` and each error kind as `{api, test, status, code, count}`. Exit 0 within thresholds, 1 blocked by a shared action, 2 unknown scenario, 3 a threshold breached, 130 cancelled |
| `vc skip <testId\|fileId> --reason <text> [--until <date>]`, `vc unskip <id>` | Writes or removes a `skip` in the YAML (see Skipping). An `--until` before today (UTC) is refused with V3104, as the skip would have expired already |

**Keep in sync**

| Command | Does |
| --- | --- |
| `vc sync [fileId...] [--dry-run] [--check] [--take spec\|mine <apiId>]… [--resolved <apiId>]… [--profile <ids>]` | Merges OpenAPI changes into api files (see OpenAPI import and sync). Files are named by file ID; a path (`services/users/users.api.yaml`) is refused with the file ID to use instead. `--profile` picks the profiles whose vars render `source.headers` (D313) |
| `vc sync <fileId> --take spec\|mine <apiId>`, `--resolved <apiId>` | Settles a sync conflict |
| `vc migrate` | Upgrades files to a newer `syntaxVersion` |
| `vc migrate v1 <path>` | Converts the Vibranium v1 project in a folder inside the project into v2 files in the project root (D194, D200): the config, an api file per collection, a test file per scenario and the secret names in `.env.schema`, passed by the compile guard. It prints each file, each `TODO(vc migrate) <path> <item>: <v1 text>` it left as a comment above the item, and each note. Exit 2, writing nothing, when the project has a config already, the folder holds no `config.json`, or it is not inside the folder the v2 project is written to |

**Results and housekeeping**

| Command | Does |
| --- | --- |
| `vc history [--limit N] [--json]` | Lists the stored runs, newest first: number, start (UTC), status, totals, profiles and selectors; `--json` also gives each run's `kind` (`run`, `load`, `adhoc`). A load run lists as `load · 40 iterations · 3 errors`, and its totals count iterations: `passed` those that passed, `failed` the errors |
| `vc history show [run\|last] [--json]` | One stored run and the tests in it that did not pass, with why. `last` (here, in `vc report` and in `--failed`) is the newest run that is not an `adhoc` call; a call is shown by its number |
| `vc history prune [--keep N]` | Drops the oldest runs beyond `N` (else the config's `history.keep`); a run also prunes when it ends |
| `vc history [query]`, `vc history compare 481 482` | Failures, flaky tests and trends across runs (later) |
| `vc report [run\|last] [--format html,junit,ctrf] [--out DIR] [--single-file]` | Writes a stored run's report files again from its snapshot (D167), `html` when `--format` names none, into `.vibranium/reports/<run>/` unless `--out` says; prints each file. A load run exits 2 (it stores no snapshot), pointing at `vc history` and at `--json` on the next `vc load <scenario>`, which sends the load again. `--open` comes later |
| `vc cache list\|clear [id]` | Shows the cached lifecycle data (`--json` for scripts) or clears it, all of it or one action's (`users#create`), running each entry's cleanup under its profiles; a cleanup that fails is a warning. Needs the cache key (`VIB_CACHE_KEY` or the OS keychain) |
| `vc config explain <key> [--profile] [--set]` | Shows a config value and which layer set it, then each value it replaced (`defaults.timeout = 1m  (--set)`, `  over 30s  (built-in)`; D129). A secret's value, and a var named like a secret, are masked as `••••` (D232). Takes the `--set` keys, or a group of them such as `defaults.retry`, which shows each; an unknown key exits 2 and names the closest |
| `vc alerts test [--profile] [--set]` | Sends one sample notice, titled `[test]` (a fixed 1 passed, 1 failed summary naming `<project>#sampleTest`), through every channel of every `alerts` rule whose `profiles` match, ignoring its `tags` and `on` (D257). Secrets fill the settings as in a run; nothing runs and nothing is recorded. PagerDuty gets an `info` event under `<project>#test`, resolved at once. Prints `✓ slack (rule 1)` or `✗ webhook (rule 2): <why>` per channel, never a URL or key. Exit 0 every channel took it, 1 one did not, 2 no rule matches |
| `vc plugin add <source> [--yes]` | Fetches a plugin (`npm:<package>[@<range>]` through `npm pack`, or `path:<folder>` in the project) into `.vibranium/plugins/.staged/`, prints its ID, version, kind, command and what it asks to reach (network hosts, secrets, files), and asks to approve. On yes it adds the `plugins:` item to the config (through the compile guard; nothing when it is there from the same source), writes the approved permissions to `vibranium.lock`, and installs it to `.vibranium/plugins/<id>/`. Without a terminal it needs `--yes`, else it exits 2 and keeps nothing; a declined plugin is dropped. A different source for an ID the config has exits 2 (`vc plugin remove` it first). A source it cannot fetch exits 2 with why: for `npm:`, npm's error code and its first line that says why (`` `npm` failed: E404: 404 Not Found - GET … ``, from its `--json` error report or its `npm error` lines, never only the pointer to its log), with help about the package and registry; for `path:`, help about the folder |
| `vc plugin list` | One line for each plugin in the config or the lock: ID, kind and version, source, state and what the lock lets it reach. The state is `ready`, `not installed` (`.vibranium/` is not committed: add it again), `not approved`, `asks for more` (its installed manifest asks beyond the lock), `not in the config`, `V5118: its installed files changed since vibranium.lock approved them` (a `blake3-` integrity no longer matches) or ``its entry program `…` cannot be found`` (not in the plugin's folder, not a file, not on `PATH`). `vc plugin add` prints a `warning:` line under the offer when the entry program cannot be found, and installs the plugin anyway |
| `vc plugin remove <id>` | Takes the plugin out of the config (through the guard, so an auth that still uses it refuses) and the lock, and uninstalls it. An unknown ID exits 2 and names the closest |

**Other interfaces**

| Command | Does |
| --- | --- |
| `vc tui [--profile <ids>]` | Opens the terminal UI: the tabs Run, Explore, History, Load and Import (`1`–`5`, `tab`, `shift-tab` back), `↑`/`↓` or `j`/`k` to move in a list, `?` opens and closes the keys (`esc` closes them), `q` to quit (pressed twice during a run or load run), `ctrl-c` to quit at once, even while typing. On Run: `a` runs every test, `R` reruns the failures, `r` the chosen test, `f` lists only failures, `x` stops the run (letting its cleanups run; `x` again, as the footer then says, abandons them). On Explore: `enter` explains the chosen api or test (`pgup`/`pgdn` scroll it), `t` skips the chosen test with a reason typed on the status line or unskips it, `c` lists only what changed in the last 7 days (`--changed-since 7d`, D249) or everything again, `r` reloads. On History: `enter` compares the chosen run of tests with the one before it, `r` reloads. On Load: `enter` runs the chosen scenario and shows each second as `vc load` prints it, then its summary, `x` stops it (and `x` again abandons its cleanups), `r` reloads. On Import: choose the importer with `←→`, type the source and file ID, toggle the tests to generate, then `d` drafts the files and shows their diffs (`n` the next, `pgup`/`pgdn` scroll), `w` writes them, `x` discards them. Quitting during a run or load run stops it first, as up to three Ctrl-Cs would, then abandons it. Plugins' stderr goes to `.vibranium/plugin-logs/<id>/stderr.log` rather than the screen; when that log cannot be made (a folder on its path is a link, or something is in the way), it is dropped instead and a notice says so. Without a terminal on stdin and stdout it exits 2 |
| `vc mcp [--profile] [--set]` | Starts the MCP server for AI agents over stdio; the profiles' `agents` rules judge every call (D195, D197). A bad `--set`, or an unknown profile from any layer (`--profile`, `VIB_PROFILE`, the user config), exits 2 before it serves; a project with errors still serves. A client that closes stdin, even before `initialize`, ends it with exit 0. A client that sends something other than `initialize` or `ping` first breaks the handshake: exit 2, naming what it sent. An `initialize` without `clientInfo` is served, as `agent` |
| `vc ui [--port 7420] [--no-open] [--profile] [--set]` | Starts the local web UI, and MCP at `/mcp`, on 127.0.0.1 only, and opens it in a browser unless `--no-open`. It prints two lines on stdout, the only place its secrets appear: `vc ui: http://127.0.0.1:<port>/?token=<token>`, a link that signs one browser in once (it sets the cookie `vc_token_<port>`; opened again it is 401, so use the open tab or restart `vc ui`), and `vc ui: mcp http://127.0.0.1:<port>/mcp token <agent token>`, which MCP clients send as `Authorization: Bearer` to `/mcp` only. Calls from the browser are the person's and skip the agent policy and log; MCP calls go through both, as `vc mcp`'s do, and an `initialize` without `clientInfo` is served as `agent` there too. Ctrl-C stops it (exit 0). A port in use, or a vc built without the `web-ui` feature, exits 2 |
| `vc approvals [--all]` | Lists the agent calls waiting for a person (D195), newest first: `#<id>  <agent>  <tool>  <profile>  <summary>`, or `no requests waiting`; `--all` also lists the decided and expired ones, each with its status. A waiting agent is refused after 10 minutes |
| `vc approve <id> [--always \| --deny]` | Approves a waiting agent call once; `--always` also allows calls like it (same profiles, tool and method) from now on, kept in `history.db`, never in YAML (D195), and prints the methods it kept a rule for; for a write or a run that reaches code it keeps no rule and says the call was approved once (D202); `--deny` refuses it. The waiting agent wakes at once. The decision is recorded under the OS user (`USER`/`USERNAME`). The ID is the number `vc approvals` lists, with or without its `#`. An unknown or already decided request, or an ID that is not a number, exits 2 |
| `vc serve` | Starts the self-hosted team server (later release). For the local web UI of this project, use vc ui, which opens http://localhost:7420 |
| `vc completion bash\|zsh\|fish\|powershell` | Prints shell completion, including test and api IDs: the commands and flags, plus a hook that asks the hidden `vc __complete <command> <word>` for the project's file, test and api IDs, profiles and tags (nothing outside a project). Tab never offers `__complete` itself. Each script completes both `vc` and `vibranium` |
| `vc demo start [--port 7421] [--detach]`, `vc demo stop [--port]` | Starts the demo service the sample calls (the `vc-demo` binary installed next to `vc`, else on `PATH`; without it, exit 2 naming where it looked and the install command); `--detach` returns once it answers and keeps a pidfile in a private folder of the user in the temp folder (`vibranium-demo-<uid>`, mode 0700; a folder that is a link or another user's is refused, exit 2), which `stop` uses; either way it prints `demo running on http://localhost:<port> (pid <n>)` and how it stops once it answers. A `start` that finds the demo already answering (its `/echo` replies) joins it, and the `stop` after it leaves the demo running for the one that launched it (`left the demo on port <port> running, as another `vc demo start` uses it`), so two runs of the sample share one demo; a join counts while its calling process lives and for an hour after it joined (the caller may be a shell line that ended first), and is dropped after both. Two `start`s at once leave one demo: they take turns on a lock file in the private folder (D297), so the second finds the first's demo answering and joins it; the one whose demo did not get the port joins the other, and only a demo that names its own pid (`x-vc-demo-pid` on `/echo`) is written to the pidfile. Another program on the port is refused, exit 2. Stopping when none runs exits 0 and says so |
| `vc schema export [--out <dir>]` | Writes the JSON Schema of each file kind (config, api, test, load) for editors: one JSON object keyed by kind on stdout, or one `<kind>.schema.json` per kind in `<dir>`. Needs no project |

## Global flags

| Flag | Does |
| --- | --- |
| `--project <dir>` | Works on the project in `<dir>`, or in the nearest folder above it with a `vibranium.config.yaml`, instead of the one around the working folder. Relative paths, path selectors and `vc schema export --out` then start at the project root. Goes before or after the command (`vc --project api-tests run`, `vc run --project api-tests`). A missing folder, or one with no config above it, exits 2 with a hint. It has no short form and no environment variable |

## Selecting what to run

`vc run` with no arguments runs every test. Arguments and flags narrow it down, and they combine: positional selectors are joined with "or", flags filter the result with "and".

| Selector | Runs | Example |
| --- | --- | --- |
| nothing | every test in the project | `vc run` |
| a test ID | one test, all its instances (repeats, example rows) | `vc run users.crud#getsCreatedUser` |
| a test ID with `:n` | one example row or repeat of a test (D140) | `vc run users.crud#rejectsBadEmail:2` |
| a file ID | every test in one file | `vc run users.crud` |
| a glob over IDs | matching tests | `vc run 'users.*' 'orders.checkout#places*'` |
| a path | every test file under a folder, or one file | `vc run services/orders/` |
| `-t`, `--tag` | tests with a tag; `!` excludes, `,` means or. A test's tags are its file's tags plus its own | `vc run -t smoke,checkout -t '!slow'` |
| `-g`, `--grep` | tests whose ID or name contains text | `vc run -g export` |
| `--changed [ref]` | tests in files git reports as changed since a ref, plus tests that call apis in changed api files (D130). With no ref, the working tree against `HEAD`: uncommitted and untracked files. A changed `vibranium.config.yaml` or `.env.schema` selects every test (D179) | `vc run --changed origin/main` |
| `--changed-since <duration>` | tests in files committed in the window (`12h`, `7d`, `2w`: a whole number of hours, days or weeks, at most 10 years), counting the files a merge made in the window brought in, plus uncommitted and untracked files, and tests that call apis in them; a changed config or `.env.schema` selects every test, as with `--changed`. Not with `--changed`; a duration of another form or over 10 years (`3650d`) exits 2 (D249). Needs git 2.37 or later (`git log --since-as-filter`); an older git is refused with that reason and exit 2, never a wrong list | `vc run --changed-since 7d` |
| `--failed` | the tests that failed or were blocked in the last run (not counting `vc call`s), under its profiles unless `--profile` names some; a failed `examples` row reruns as its own instances. Takes no other selector; exits 2 when no run is stored or the last one had nothing to rerun | `vc run --failed` |
| `--include-skipped` | also runs tests marked `skip` | `vc run users.crud --include-skipped` |

A full ID joins the file ID and the item ID with `#` (`users.crud#getsCreatedUser`); a bare dotted word is always a file ID (D138). A `#` inside a word is not a comment in bash, zsh, fish or PowerShell, so selectors work unquoted; quote them only under zsh's `extendedglob` option. Web UI URLs percent-encode it (`%23`).

Useful run flags: `--profile staging,eu` (profiles, as a comma list or repeated; later wins), `--bail` (start no more tests after the first failure), `--repeat 20` (run every selected test this many times instead of its own `repeat`, to hunt flakiness; each instance is labelled `[repeat n]`, as with `repeat:`), `--concurrency 4`, `--seed 42` (repeatable generated data; a failing run prints the seed it used), `--set key=value` (override one config value for this run, D129), `--no-cache` (`cache:` actions run every time; the cache is neither read nor written, and no expired entry is cleaned up), `--reporter spec|json` (console format; `dots` comes later; `--json` is `--reporter json`, D264), `--strict` (an `allowFailure` test that fails fails the run, D248), `--report ctrf,junit,html` (files to write, as a comma list or repeated, over the config's `reports.formats`; `ctrf` when neither names any), `--out <dir>` (where the report files go, as they are, over the config's `reports.dir`; without it each run's files go in `<reports.dir>/<run>/`, named by the run's number in the history), `--single-file` (the HTML report as one self-contained `report.html`, over `reports.html.layout`). Every run is stored in `.vibranium/history.db` (D170); a history that cannot be opened leaves the run unrecorded with a warning and its files straight in `reports.dir`. `plan`, `run` and `list` exit 2 when they refuse to start: errors in the definitions, an empty selection, a selector that picks only `standalone: false` tests (the error says they run only when reused), or an unknown flag value.

`--set` takes `profile`, `defaults.concurrency`, `defaults.perHost`, `defaults.timeout`, `defaults.retry.attempts`, `defaults.retry.delay`, `reports.dir`, `reports.formats`, `reports.html.layout` and `vars.<name>`; any other key exits 2 and lists these.

Selecting a few tests never runs other files' setup. Running one checkout test runs the config's `beforeRun` (it runs for any run that selects a test), its own file's `beforeAll` and its `beforeEach`; other files' `beforeAll` blocks don't run. `beforeRun` and `beforeAll` finish before any selected test starts (D122). `vc plan` prints this before you run, and the first line of `vc run` output repeats it:

```
$ vc run orders.checkout#placesOrder --profile staging
plan  1 test · lifecycle: beforeAll catalog (cached, 3h left) · beforeEach ×3 calls · profile staging
```

## Creating apis and tests

### A new api

`vc new api <fileId>#<apiId>` appends to the api file with that ID, creating it if none exists at the path its ID derives from (D139): the ID's words become folders and the last word the file name, and a one-word ID gets its own folder (`users` → `services/users/users.api.yaml`, `payments.refunds` → `services/payments/refunds.api.yaml`). Path parameters become vars, so `{id}` is written as `${var.id}`; the `name` is the ID as a sentence (`listOrders` → `List orders`), and the method is written only when it is not GET. A new file made from `--path` gets `baseUrl: ${var.<last word>Url}`; when neither the config nor a profile sets that var it is added as `http://localhost`, with a note to set it. An existing file with no `baseUrl` refuses a `--url` api and says to add `baseUrl: ${var.…}` to it. With `--url https://api.example.com/users?q=ann` instead of `--path`, the origin becomes a config var named after the file ID's last word (`searchUrl`), or the var that already holds it (never another var holding the `http://localhost` placeholder, which belongs to its own service), and the new file's `baseUrl`; when the config or a profile already sets that name to anything else it is refused rather than shadowed; the rest becomes the path and `query` (D177):

```
$ vc new api users#deactivate --method POST --path /users/{id}/deactivate
✓ added users#deactivate to services/users/users.api.yaml:84
```

```yaml
  - id: deactivate
    name: Deactivate
    method: POST
    path: /users/${var.id}/deactivate
```

From a curl command, copied from browser dev tools or API docs, Vibranium also fills in what it can read:

```
$ vc new api users#rename --from-curl 'curl -X PATCH https://users.staging.shop.example/api/v1/users/u_7f3k \
    -H "content-type: application/json" -H "authorization: Bearer eyJ…" -d "{\"name\":\"Ada\"}"'
✓ added users#rename to services/users/users.api.yaml:91
  PATCH /api/v1/users/${var.userId}: header authorization: a credential is never written; add it with `vc new auth`
```

- The origin is matched against the config's vars, as with `--url`: a var that already holds it is used, otherwise a new `<last word>Url` var is added.
- IDs in the path become vars (`u_7f3k` → `${var.userId}`); the fields of a JSON body become api vars with the curl's values as defaults, so tests can override them. The query, form data and the content type are kept too.
- Credentials (`authorization`, cookies, `-u`, secret-named headers, query and body fields) are never written to YAML: each is named in a note, never its value, and a body field is written empty. An api file's own `auth` still applies.
- One command per call; for a file of them, use `vc import curl <file>`.

### Generated tests

`vc new test <fileId>#<testId> --from <apiId>` writes a happy-path test you can run straight away; `--negative` and `--auth` add the usual failure cases. `vc import openapi --tests` does the same for every operation.

| Flag | Adds | Expects |
| --- | --- | --- |
| (always) | One happy-path test | The spec's success status, and the response schema when the spec has one |
| `--negative` | One test with an example row per invalid body: each required field missing, each field with the wrong type, each enum given a value outside it | `400` (or the spec's documented client-error status) |
| `--auth` | One test calling with `auth: none` | `401` |

What the generator fills in for you:

- **Required inputs.** Vars the api needs get generators matched to the spec's type and format (`email` → `${fake.internet.email}`, `uuid` → `${fake.string.uuid}`), written as the api's defaults so every test benefits.
- **Path vars.** When a path needs a var (`/users/{id}/deactivate`) that neither the config, a profile, the api file nor the test file sets, the test sets it under `vars:` so the file passes `vc check` (D262): to the api's own value when it has one, written as the api writes it so its type is kept. For an imported api that value comes from the spec's schema: `${fake.number.int(…)}` for an integer, `${fake.string.uuid}` for a uuid, `true` for a boolean. The api syntax declares no type for a var, so one with no value anywhere gets `replace-me`. A `# replace` comment marks the line either way. To create the resource first instead, add `before: - call: users#create` by hand; that api's cleanup removes it afterwards.
- **Schemas.** Response schemas are referenced straight from the spec snapshot, so nothing is copied: `schema: {$file: ../../openapi/users.yaml#/components/schemas/User}`. The `schemas/` folder is for schemas you write yourself.

```yaml
# generated by: vc new test users.crud#deactivatesUser --from users#deactivate --negative --auth
  - id: deactivatesUser
    name: Deactivate a user
    call: users#deactivate
    vars:
      id: replace-me  # replace with a real id
    expect:
      status: 200
      body:
        schema:
          $file: ../../openapi/users.yaml#/components/schemas/User

  - id: rejectsInvalidDeactivate
    name: Deactivate rejects an invalid body
    vars:
      id: replace-me  # replace with a real id
    examples:
      - name: reason missing
        body: {notify: true}
      - name: notify is not a boolean
        body: {reason: left company, notify: "yes"}
    call: users#deactivate
    body: ${example.body}
    expect:
      status: 400

  - id: rejectsAnonymousDeactivate
    name: Deactivate needs a signed-in caller
    call: users#deactivate
    auth: none
    vars:
      id: replace-me  # replace with a real id
    expect:
      status: 401
```

A `name` in an example row labels that instance in reports, so a failure reads "rejectsInvalidDeactivate › notify is not a boolean".

`vc import openapi` writes these tests for every operation into `services/<fileId>/openapi.test.yaml` (file ID `<fileId>.openapi`), the same three cases with these differences:

- **Names and IDs.** A test is named after the operation's summary without its full stop, else the api's ID as a sentence (`deletePetsPetId` → "Delete pets pet id"). IDs are the api ID as a third-person verb (`createUser` → `createsUser`, `login` → `logsIn`), `rejectsInvalid<Api>` and `rejects<Api>WithoutAuth`; the negative and auth names add ": rejects an invalid body" and ": needs a signed-in caller".
- **Status and schema.** The happy test expects the operation's first documented 2xx (`201` for a create that lists only `201`) and, when that response has a JSON schema, `body.schema` pointing into the snapshot: at `#/components/schemas/<Name>` for a `$ref`, else at the schema inside the operation, with what a URI fragment cannot hold percent-encoded (`/items/{id}` is `~1items~1%7Bid%7D`). A percent-encoded `$ref` in the spec is followed decoded. The negative test expects `400`, or `422` when the spec lists that and not `400`.
- **What a call needs.** For each `{param}` in the path, the `POST` on the collection above it (`POST /users` for `/users/{id}`) is the creator when its success response has a property named like the parameter, or `id`. The creator gets `save: {<resource>Id: ${response.body.<property>}}`, named after the collection's last word in the singular (`/users` saves `userId`, `/line_items` `lineItemId`; a second creator in the file whose name clashes saves `userId2`, then `userId3`, in route order), and a `cleanup` calling the `DELETE` on the item's path when the spec has one, its `vars` keyed by that `DELETE`'s own parameter. Each test calls the creators in `before`, outermost first, and passes each parameter whose name differs from the save its creator's value on the call: `vars: {id: ${var.userId}}` for `/users/{id}`; a `/users/{userId}` needs no `vars`. When the test's call is the `DELETE` of the item its last `before` creates, that `before` has `keep: true`, as the call itself removes the item; a `DELETE` of something else (`DELETE /users/{id}/orders`) keeps nothing, so the user is still cleaned up. When nothing creates an item, the test is written with `skip: {reason: "needs an existing <item>: set vars.<param> to one and remove this skip"}` instead.
- **Invalid bodies.** The rows start from the api's drafted JSON body, its vars filled in, and break one field it sends at a time: missing when required, a value of the wrong type, a value outside its enum. The wrong value is one no type the field allows accepts, counting each type in a `type` list and each `anyOf` or `oneOf` alternative (an integer passes as a number), and the row is named after them (`flag is not a boolean or integer`); a field the row cannot break that way, such as `type: [string, number]`, gets no such row.

From the mock users spec with `--tests --negative`, the api file's `createUser` gains

```yaml
    save:
      userId: ${response.body.id}
    cleanup:
      call: deleteUser
      vars:
        id: ${response.body.id}
```

and the test file reads:

```yaml
id: users.openapi

tests:
  - id: listsUsers
    name: List users
    call: users#listUsers
    expect:
      status: 200
      body:
        schema:
          $file: ../../openapi/users.yaml#/paths/~1users/get/responses/200/content/application~1json/schema
  - id: createsUser
    name: Create user
    call: users#createUser
    expect:
      status: 201
      body:
        schema:
          $file: ../../openapi/users.yaml#/components/schemas/User
  - id: rejectsInvalidCreateUser
    name: "Create user: rejects an invalid body"
    call: users#createUser
    examples:
      - name: email missing
        body:
          name: Ada
      - name: email is not a string
        body:
          email: 123
          name: Ada
      - name: name missing
        body:
          email: ada@example.com
      - name: name is not a string
        body:
          email: ada@example.com
          name: 123
    body: ${example.body}
    expect:
      status: 400
  - id: getsUser
    name: Get user
    before:
      - call: users#createUser
    call: users#getUser
    vars:
      id: ${var.userId}
    expect:
      status: 200
      body:
        schema:
          $file: ../../openapi/users.yaml#/components/schemas/User
  - id: deletesUser
    name: Delete user
    before:
      - call: users#createUser
        keep: true
    call: users#deleteUser
    vars:
      id: ${var.userId}
    expect:
      status: 204
```

### Capturing a real call

`vc call` sends one request with the project's auth and active profile and prints the response. Adding `--save-as-test` writes it as a test, expecting what came back: the status, and the body fields you name with `--expect-field body.<path>` at the values they had (no schema is inferred, D176); the web and terminal UIs offer the same with checkboxes.

```
$ vc call users#deactivate --var id=u_7f3k29qp --profile staging --save-as-test users.crud#deactivatesUser
200 OK · 148 ms · api expect passed
✓ wrote users.crud#deactivatesUser to services/users/crud.test.yaml:72
```

Without `--save-as-test` it prints the status line, then the body (indented when it is JSON) and, when the api's `expect` fails, the failed check after `✗`; secrets are masked as in reports. `--var` values are strings and beat the vars the api and the config define. A cleanup that fails is a warning; it does not change the exit code.

### Importing from a HAR file

A HAR file is the recording a browser's dev tools save from the Network tab (Chrome, Firefox, Safari and Edge all export it, as do proxies like Charles and mitmproxy). Importing one turns real traffic into apis, and optionally into a test that replays the session in order. It works the same from `vc import har`, the terminal UI and the web UI's import dialog.

```
$ vc import har ~/Downloads/checkout-session.har --flow orders.checkout#recordedCheckout
```

What the import does, in order:

1. **Filters noise.** It drops static assets, third-party hosts (analytics, CDNs) and `OPTIONS` preflights; `--include-host` and `--keep-static` override this. A host is third-party when its site (its last two labels) is not the site of any api file's base URL (D206), so an api on a new domain is dropped too until `--include-host` names it. When nothing is left, the error names each host it dropped as third-party with the `--include-host` that keeps it.
2. **Maps hosts to api files.** Each host goes to the api file whose `baseUrl` renders to it under the active profile (`--profile`), the longest base path winning; an unknown host it kept creates a new api file named by the conventions (`api.shop.test` → `shop`), with a base-URL var such as `shopUrl` added to the config. A new host whose file ID already has an api file pointing elsewhere is refused: pass the `--profile` whose vars point that file there, or rename the file first.
3. **Turns requests into apis.**
   - IDs come from the method and path (`POST /carts/{id}/items` → `addItem`).
   - IDs and UUIDs in paths become vars (`/orders/ord_8Kq2Tz` → `/orders/${var.orderId}`).
   - Repeated calls are merged into one api.
   - A request with the method and path shape of an existing api is reused rather than duplicated; the rest are appended to the file, the base URL's path taken off theirs.
   - Body fields become api vars, with the recorded values as defaults.
4. **Removes secrets.** Authorization headers and cookies are never written. They map to the file's auth, or the line suggests one (`vc new auth payAuth --type bearer`). Query or body fields with secret-like names, and high-entropy values, become `${secret.<FILE>_<FIELD>}` references (`${secret.PAY_API_KEY}`), declared in `.env.schema` when the project has one; otherwise the line says what to add to it. A recorded value never reaches a written file, the output or the dry-run diff.
5. **Optionally writes a flow test** (`--flow <fileId>#<testId>`, or a bare test ID for `<firstFile>.recorded`). This is one test whose steps replay the session, with values chained: the cart ID saved from the create response feeds the next calls. Its calls point at the apis reused or added, with their var names. A test ID the file already has is refused.
6. **Checks before writing.** The result must compile, or nothing is written. `--dry-run` prints the same lines and the diff of every file without writing.

```
$ vc import har checkout-session.har --flow shop.checkout#replaysCheckout
✓ imported checkout-session.har
read 12 entries · filtered 3 static, 1 third-party, 1 OPTIONS → 7 requests
  api.shop.test → shop (existing)  1 matched, 4 new · auth shopToken
  pay.shop.test → pay (new)  1 new · no auth yet: `vc new auth payAuth --type bearer`
secrets  header authorization, cookie dropped → each file's auth · body field token → ${secret.PAY_TOKEN} · body field nonce → ${secret.PAY_NONCE} · query api_key → ${secret.PAY_API_KEY} · body field password → ${secret.SHOP_PASSWORD}
.env.schema  declares PAY_TOKEN, PAY_NONCE, PAY_API_KEY, SHOP_PASSWORD
flow shop.checkout#replaysCheckout (7 steps)
POST /v1/carts/${var.cartId}/items: recorded 2 times, merged into one api
```

The lines after the first are the plan: what was read and filtered, where each host went (`existing` or `new`, apis matched and added), what became a secret reference, what `.env.schema` gains, the flow test, then the importer's notes, one line per message (`GET /a and 2 more: …` when several places share it).

## OpenAPI import and sync

An api file can be generated from an OpenAPI spec and kept in step with it, while everything you add by hand (names, `expect`, `save`, `cleanup`, defaults) stays yours.

### Where the spec comes from

| Source | Works | Notes |
| --- | --- | --- |
| URL | `vc import openapi https://users.staging.shop.example/v3/api-docs` | The usual case for Spring, FastAPI, NestJS and others that serve their spec; private URLs take headers (below) |
| File in your repo | `vc import openapi specs/openapi.json` | For monorepos, or a spec checked in by the backend team; the path is from the working folder and must be inside the project (a file outside it is refused) |
| JSON or YAML | both | Detected from content, not the extension |
| OpenAPI 3.0, 3.1, or Swagger 2.0 | all | Swagger 2.0 is converted to OpenAPI 3.1 on import |
| A spec split over several files | yes | `$ref`s to other files are resolved and bundled |

The api file records where its spec lives, so `vc sync` can fetch it again:

```yaml
# services/users/users.api.yaml
id: users
baseUrl: ${var.usersUrl}/api/v1
auth: shopOauth
source:
  openapi: https://users.staging.shop.example/v3/api-docs
  headers:                          # only for a spec behind auth
    authorization: Bearer ${secret.SPEC_TOKEN}
  include: [users]                  # optional: only these tags, so one spec can feed several api files
```

### Do you need to keep a copy of the spec?

You don't keep one yourself, but Vibranium does: every import and sync writes `openapi/<fileId>.yaml`, a snapshot of the spec as it was at that moment. It is converted to OpenAPI 3.1 YAML with references bundled, so it diffs cleanly in pull requests. Commit it. It is the merge base that lets `vc sync` tell a change in the spec from a change you made, and it lets everyone on the team, and CI, sync the same way. Only `vc import` and `vc sync` write it; the tests `vc import openapi --tests` writes point their schemas into it. Both refuse (exit 2, nothing written) when the spec is kept in `openapi/` (keep it in `specs/`, say), when `openapi/<fileId>.yaml` is the spec another api file syncs from, or when it leads to another file: a link, or a file named in another case on a file system that ignores case. A snapshot is replaced, never written into, so a hard link there leaves the spec as it was. Runs never need the spec or the network to reach it.

### What sync does

`vc sync users` fetches the spec, compares it with the snapshot and with your api file, then updates the file and the snapshot:

| Spec since the snapshot | Your api file since the snapshot | Result |
| --- | --- | --- |
| unchanged | changed | Your edit is kept |
| changed | unchanged | The api is updated from the spec |
| changed | changed the same way | Nothing to do |
| changed | changed differently | Conflict: reported, the api is left as it is, until `--take spec`, `--take mine` or `--resolved` |
| new operation | — | New api added (its tests are not drafted yet) |
| operation removed | — | The api is kept and marked `deprecated: true`; `vc check` warns where tests still call it |

- **What sync owns:** `method`, `path`, each `query` entry and the `content-type` header. A value it takes from the spec that reads a var the api lacks (`${var.cursor}`) brings that var with the spec's default. A query param that is optional with no default is not sent, so an api never gains one: an optional param the spec adds, or drops while the file still sends it, is listed with `~` (`~ users#listUsers optional query limit added to the spec: not sent, add \`query.limit\` to send it`). It never touches IDs, names, `body`, `expect`, `save`, `cleanup`, hooks or the defaults you set, and does not yet write tests for new operations. A change it cannot make in the file as written (taking an entry out of a `{..}` map) is refused, naming the api, for you to make by hand.
- **Which files:** `vc sync` with no file IDs syncs every api file with a `source` key; a file named that has none is refused. An api file that does not parse is refused by every form, `--check` included, with exit 2 and nothing written, since nothing could be compared.
- **Which operations:** the ones the import took. `vc import --include` and `--only` are kept in the file as `source.include` and `source.only`, and sync takes only the operations they match, so a new operation outside them is not added (D277).
- **Conflicts:** an api whose field both sides changed differently, or a new operation whose ID the file already uses, is left as it is and listed with how to settle it. `--take spec <apiId>` takes the spec's values; `--take mine <apiId>` keeps yours; after editing by hand, `--resolved <apiId>` keeps the file as it is. Until then its snapshot entry stays as it was, together with the old snapshot's components it may refer to, so the next sync shows the same conflict. A conflict does not change the exit code of `vc sync`; under `--check` it exits 4, like drift.
- **Matching:** apis are matched to operations by `operationId`, so you can rename an api ID freely. New IDs come from the operationId in camelCase; without one, from the method and path (`POST /users/{id}/deactivate` → `postUsersIdDeactivate`).
- **Previewing and CI:** `vc sync --dry-run` prints the changes and their diffs without writing anything. `vc sync --check` prints them, writes nothing and exits with code 4 when any spec has changed since its snapshot (D127), so a CI job can flag drift before tests start failing. Drift is never silent: a spec that changed only where sync takes nothing into the file (a description, a schema, an operation outside `source.include`) prints `~ users the spec changed outside what sync takes into the file: only the snapshot is updated`.

```
$ vc sync users
fetched https://users.staging.shop.example/v3/api-docs (8 operations)
  ↑ users#list query cursor none → ${var.cursor}
  ↑ users#get path /users/${var.id} → /users/${var.userId}
  + users#restoreUser new operation POST /users/${var.userId}/restore
  = users#create kept your content type
  ! users#update conflict: the spec and the file both changed method
    resolve: vc sync users --take spec users#update | --take mine users#update | edit, then --resolved users#update
  - users#legacyImport removed from the spec: kept, marked deprecated
✓ synced · snapshot openapi/users.yaml updated · 1 conflict left
```

## Skipping tests and files

Like JUnit's `@Disabled`, `skip` parks a test without deleting it. A skipped test never runs, and every report lists it as **skipped** with its reason, so it stays visible instead of silently disappearing. A reason is required; an end date is optional and recommended.

```yaml
# one test
  - id: appliesCoupon
    skip:
      reason: "coupon service flaky on staging, see #212"   # quoted: an unquoted # starts a YAML comment
      until: 2026-11-15
```

```yaml
# a whole file: put skip at the top of the file
id: orders.coupons
skip: coupon service is being rewritten
tests:
  …
```

- `skip: <reason>` is the short form; `skip: {reason, until}` adds an end date.
- After `until`, the test runs again on its own, and `vc check` warns that the skip has expired, so a forgotten skip turns back into a running test.
- A skipped file skips every test in it, and its `beforeAll` and `afterAll` don't run.
- A skipped test is never a lifecycle dependency of others. A `test:` action that reuses a skipped test still runs its calls, because skipping is about reporting, not about removing the flow.
- `vc run --include-skipped` runs skipped tests anyway, to check whether a fix worked.
- `vc skip <id> --reason … --until …` and `vc unskip <id>` write and remove the YAML for you; `vc list tests --skipped` shows every skip with its reason and date.

In reports, skipped tests are counted on their own (`9 passed · 1 failed · 1 skipped`), keep their reason and end date, and map to `skipped` in CTRF and `<skipped message="…"/>` in JUnit, so CI dashboards show them too.

## Errors and fixes

Nothing runs until every file compiles, so `vc check` (also run at the start of `vc run`, on save in the editors, and before any command writes a file) is where most mistakes surface. Every diagnostic has the same parts:

| Part | Example |
| --- | --- |
| Code and title | `error[V2003]: unknown api "orders#plcae"` |
| Location | `--> services/orders/checkout.test.yaml:41:15` |
| Source excerpt with the exact span underlined | the offending line, with `^^^^` under the name |
| Plain explanation | what was expected and what was found |
| `help:` | what to do, with the likely options: "did you mean orders#place?", or the three places a missing var could be set |
| `fix:` | when the fix is certain, what `vc check --fix` will change |

```
error[V2101]: ${var.couponCode} is used but never set
  --> services/orders/checkout.test.yaml:58:21
   |
58 |       couponCode: ${var.couponCode}
   |                   ^^^^^^^^^^^^^^^^^ no level sets couponCode for profile staging
   = note: profile prod sets it (vibranium.config.yaml:44); staging and dev don't
   = help: set it in this test's vars, in the file's vars, or under profiles.staging.vars
```

- **Fixes:** `vc check --fix` shows a diff of every certain fix and asks before applying. Certain fixes include a typo with one close match, a quoted number, an indentation error with one reading, a naming-convention rename (references included) and an expired skip removed. Anything with more than one reasonable answer stays a `help:`.
- **Explanations:** `vc explain <code>` prints a longer explanation of a diagnostic code, with a wrong and a right example. The same text appears on hover in editors and in the web UI's Problems panel.
- **Same diagnostics everywhere:** the terminal UI and web UI show these diagnostics live, as you type, with the same codes, and offer the fix as a button.

| Range | Covers |
| --- | --- |
| V1xxx | YAML and JSON syntax, file kinds, `syntaxVersion` |
| V2xxx | References and types: unknown IDs, unset vars, wrong value types, cycles, bad matchers, invalid templates and pipes |
| V3xxx | Lifecycle and runtime rules: expired skips, shared values written to, cache in per-test blocks, no active profile |
| V4xxx | Conventions: ID and file naming, folder placement |
| V5xxx | Failures reported at run time (expectations, CEL, custom checks) |

## Output, exit codes and CI

`vc run` prints a plan line, one line per test as it finishes, a detailed block for each failure, and a summary. The detailed block reads like a compiler error: the file, line and column of the failing check, expected and actual values, and where an expected value came from.

```
✕ orders.checkout#placesOrder                                  6.4s
  error: expect.body.match.total
    --> services/orders/checkout.test.yaml:33:11
     |
  33 |           total: ${var.orderTotal}
     |                  ^^^^^^^^^^^^^^^^^ expected 59.98 (saved by orders#place), got 64.78
  error: expect.cel “total equals sum of lines”  → 64.78 == 59.98 is false
    --> services/orders/checkout.test.yaml:35:9

9 passed · 1 failed · 1 blocked · 1 skipped · 1 flaky        14.2s · profile staging
reports  .vibranium/reports/483/{ctrf.json,junit.xml,report.html}
next     vc run --failed · vc report 483 --open
```

Every test instance ends as `passed`, `failed`, `blocked` (a setup it needs failed; never retried), `skipped`, `flaky` (failed, then passed when its `retry` re-ran the whole test) (D126) or `allowedFailure` (failed under `allowFailure`; shown as `⚠ … allowed to fail: <reason>` with its failure block, and counted as `N allowed to fail`) (D248).

A failed check or action in an `afterAll` or `afterRun` leaves every test's verdict as it was, but the run fails (exit 1) and the summary adds a line under the counts naming each block that failed, once: `teardown failed: afterAll in users.crud, afterRun` (D279). An action of either block cut off before it reports (its worker panicked or was aborted) counts as failed, unless the run was cancelled, which exits 130. `--reporter json`'s `summary` line and the HTML report carry `teardownFailures: [{block, file, message}]` (`file` is null for `afterRun`), CTRF the same list under `results.extra.teardownFailures`, and JUnit one `<system-err>` on `<testsuites>` per block; messages are masked and no count changes (D304).

Console formats: `spec` (above, the default in a terminal), `dots` (compact), `json` (one event per line, for tools). In CI, where there is no terminal, colour and progress bars switch off and output stays line-based. When the reader of stdout goes away (`vc run | head -1`), the rest of the output is dropped quietly and the command ends with the exit code it would have had.

| Exit code | Means |
| --- | --- |
| 0 | Every selected test passed or was skipped; flaky passes count as passed, and failures under `allowFailure` do not count (unless `--strict`) |
| 1 | At least one test failed or was blocked, or an `afterAll` or `afterRun` action failed (D279) |
| 2 | Nothing ran: `vc check` errors, an empty selection, or no active profile |
| 3 | A load threshold was breached |
| 4 | `vc sync --check` found drift or a conflict left unsettled; a plain `vc sync` that leaves conflicts exits 0 |
| 70 | Internal error in Vibranium: prints a "please report" hint and the path of a redacted debug bundle to attach (D127) |
| 130 | Cancelled by a signal (Ctrl-C); cleanups have run (D127) |

A typical CI job:

```yaml
# .github/workflows/api-tests.yml (excerpt)
- uses: vibranium/setup-action@v2
- run: vc check
- run: vc sync --check            # fail early when the backend's spec changed
- run: vc run --profile staging --report junit,html
- uses: actions/upload-artifact@v4
  with: { name: vibranium-report, path: .vibranium/reports }
```

## Editor and shell support

- **Autocomplete and validation in YAML.** `vc init` adds a JSON Schema hint to each file, so VS Code, JetBrains IDEs and Neovim (via the YAML language server) complete keys and flag mistakes as you type. A language server (`vc lsp`), which also completes api and test IDs and vars and jumps to definitions, follows in a later release.
- **Shell completion.** `vc completion zsh` (or bash, fish, powershell) completes commands, flags, and the project's own api, test, file and tag IDs, so `vc run users.c<Tab>` works.
- **Watch mode.** `vc watch` re-runs only the tests affected by a save: editing a test file re-runs its tests, editing an api file re-runs the tests that call it, and editing the config, `.env` or `.env.schema` re-runs them all. Saving a dataset, schema or hook file does not re-run anything yet.
- **One-keystroke loops in the TUI.** `vc tui` offers the same selectors with search, `r` to re-run, `f` for failures only, and inline editing with checks before saving.
- **Explain anything.** `vc explain orders.checkout#placesOrder` shows the test exactly as it will run for the active profile: URLs, every var with the level that set it, auth, the merged checks and the lifecycle work around it. It is the first thing to try when a value isn't what you expected.
