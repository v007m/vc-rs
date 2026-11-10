---
name: vibranium
description: Write, run, check and fix HTTP API tests with Vibranium (the `vc` CLI and its MCP server). Use when the task involves `*.test.yaml`, `*.api.yaml`, `*.load.yaml` or `vibranium.config.yaml` files, `vc` commands (check, run, plan, explain, call, import, sync, load, history), Vibranium diagnostic codes such as V2003, or adding, debugging or load-testing API tests in a project that has a `vibranium.config.yaml`.
---

# Vibranium (`vc`)

Vibranium tests HTTP APIs from plain YAML files kept in git. One binary, `vc`, checks, runs, explains and load-tests them. `vc mcp` serves the same engine to agents over MCP.

## File kinds

| File | Where | Holds |
| --- | --- | --- |
| `vibranium.config.yaml` | Project root, one per project | `syntaxVersion: 2`, `project`, `vars`, `profiles` (each may have an `agents` block), `secrets`, `defaults`, `reports`, `history` |
| `*.api.yaml` | `services/<service>/<service>.api.yaml` | `baseUrl` and `apis:`, the requests: method, path, query, headers, body, vars, `expect`, `save`, `cleanup` |
| `*.test.yaml` | `services/<service>/<area>.test.yaml` | `tests:` that `call:` apis and add `expect`, `before`, `steps`, `examples` |
| `*.load.yaml` | `load/<service>/<name>.load.yaml` | A `mix` of existing tests, `stages`, `thresholds` |
| `.env.schema` / `.env` | Project root | Secret names (committed) and values (never committed) |

## IDs

- Every item has a camelCase `id` and a free-text `name`.
- A file's ID comes from its path: `services/users/crud.test.yaml` is `users.crud`, `services/users/users.api.yaml` is `users`, `load/users/browse.load.yaml` is `users.browse`. Deeper folders add words. Do not add an `id:` key to a file; if one is there, it must match the path (V2007).
- A full item ID is `<fileId>#<id>`: `users.crud#getsCreatedUser`, `users#createUser`. A bare dotted word is always a file ID.
- One example row: `users.crud#rejectsBadEmail:2` (rows count from 1).
- Test IDs are third-person verbs: `getsCreatedUser`, `rejectsBadEmail`, `listsUsers`. Api IDs are imperative: `createUser`, `getUser`.
- Var names end in `Id` or `Url` where that applies (`userId`, `usersUrl`). Profiles are lowercase. Tags are kebab-case.

## Cheat sheet

Every snippet below passes `vc check` and runs. For more, read `reference/syntax.md` next to this file.

```yaml
# vibranium.config.yaml
syntaxVersion: 2
project: shop

vars:
  usersUrl: http://localhost:7531

profiles:
  - id: local
    default: true
    agents:
      run: true
      call: [GET]
      approve: [POST, PUT, PATCH, DELETE]
      writeDefinitions: ask
  - id: staging
    vars:
      usersUrl: https://users.staging.example

secrets:
  provider: env
  schema: .env.schema
```

```yaml
# services/users/users.api.yaml  (file ID: users)
name: Users service
baseUrl: ${var.usersUrl}

apis:
  - id: listUsers
    name: List users
    path: /users
    expect:
      status: 200

  - id: createUser
    name: Create a user
    method: POST
    path: /users
    vars:
      name: ${fake.person.fullName}
      email: ${fake.internet.email}
    body:
      name: ${var.name}
      email: ${var.email}
    expect:
      status: 201
    save:
      userId: ${response.body.id}
    cleanup:
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
      status: [204, 404]
```

```yaml
# services/users/crud.test.yaml  (file ID: users.crud)
name: User CRUD
tags: [users, smoke]

tests:
  - id: listsUsers
    name: Lists users
    call: users#listUsers
    expect:
      body:
        match:
          "[0].id":
            type: string
          "[*].name":
            every:
              type: string

  - id: getsCreatedUser
    name: Returns the user that was just created
    before:
      - call: users#createUser
        vars:
          email: ada@example.com
    call: users#getUser
    expect:
      status: 200
      body:
        match:
          id: ${var.userId}
      timing:
        total:
          lt: 500ms

  - id: rejectsBadEmail
    name: Rejects a user without an email
    examples:
      - name: No Email
      - name: Also No Email
    call: users#createUser
    body:
      name: ${example.name}
    expect:
      replace: true
      status: 400
```

```yaml
# services/users/flow.test.yaml  (file ID: users.flow)
name: User flows
tests:
  - id: createsThenReadsUser
    name: Creates a user, then reads it back
    headers:
      authorization: Bearer ${secret.USERS_TOKEN}
    steps:
      - id: create
        call: users#createUser
        vars:
          email: grace@example.com
      - id: read
        call: users#getUser
        vars:
          userId: ${create.body.id}
        expect:
          body:
            match:
              id: ${create.body.id}
```

Key points:

- A test's `expect` is merged over the api's `expect`. `replace: true` drops the api's checks (needed above, since the api expects 201).
- `${var.x}`, `${secret.X}`, `${fake.person.fullName}`, `${response.body.id}`, `${example.name}`, `${<stepId>.body.id}` are the template sources. A `save:` in a `before` call makes the var available to the test.
- `body.match` paths are quoted when they start with `[`: `"[0].id"`, `"[*].name"`. Do not prefix them with `$`.
- Matchers seen above: a plain value (equals), `type`, `every`, `contains`, `lt`. `body.equals` compares the whole body.
- `headers:` belongs on a test, an api or `defaults.headers` in the config. A test file has no top-level `headers:` (V1004).
- `skip: <reason>` on a test skips it; `vc run --include-skipped` runs it anyway.

## The loop: check, plan, run, explain

Run every command from inside the project. Prefer the JSON forms when you need to parse output.

1. **Check** after every edit: `vc check` (or `vc check --json`). Exit 2 means errors. Fix every error before running.
2. **Plan** to see what a selector picks without sending anything: `vc plan users.crud`, `vc plan --changed`, `vc plan -t smoke --json`.
3. **Run**: `vc run users.crud`, `vc run 'users.crud#getsCreatedUser'`, `vc run -t smoke`, `vc run --changed main`. Use `--reporter json` for an NDJSON event stream. Exit 0 pass, 1 fail, 2 nothing ran.
4. **Explain** a failure:
   - `vc history show <run> --json` gives each test's status and message, including the body diff.
   - `vc explain users.crud#getsCreatedUser` shows the request as the run sees it, with the source of every var.
   - `vc call users#getUser --var userId=1` sends one api and prints the response, so you can compare it with the expectation.
   - `vc run --failed` reruns what failed in the last run. A `vc call` counts as the last run, so after one, name the tests instead.
5. A failing run prints `seed N`; `vc run --seed N` replays the same generated data.

Other commands: `vc list tests|apis|files [--json]`, `vc import openapi <spec> --id <service> --tests`, `vc sync [<service>] [--check|--dry-run]`, `vc load <scenario> [--json]` (exit 3 on a breached threshold), `vc history [--json]`.

## Diagnostics

Every problem has a code (V1 syntax, V2 references and types, V3 lifecycle, V4 conventions, V5 run time), a file, line and column, `help:` text and sometimes a `fix:`.

- Read the `help:` line first. It usually names the right ID or key ("did you mean `people#getPerson`?").
- `vc explain V2003` explains any code with a wrong and a right example.
- `vc check --fix` shows the certain fixes as a diff and asks `Apply N fix? [y/N]`. Without a terminal, pipe the answer: `echo y | vc check --fix`. Read the diff first: a certain fix can rename a reference to a different api when only one name is close.
- `vc check --json` gives each fix as byte-range `edits`, if you prefer to apply them yourself.
- V4 warnings (such as V4001, a test ID that is not camelCase) appear only once there are no errors.

## Using the MCP server

When the `vibranium` MCP server is connected, prefer its tools over the shell:

| Tool | Use it to |
| --- | --- |
| `list_definitions`, `search`, `get_definition` | Find tests and apis and read how one resolves |
| `check` | Check the project or some paths |
| `run_tests` | Run tests. Select them with `ids` (`["users.crud"]`), `tags` or `grep`. With no selection it runs every test |
| `call_api` | Send one api, with `vars` |
| `write_definition` | Write a whole definition file (`path`, `text`). It compiles first and writes nothing if the change adds an error |
| `query_history`, `explain_failure` | Read past runs and why a test failed |

- Use the exact argument names from the tool schema. Unknown arguments are ignored, not rejected.
- The project's `agents` block decides what you may run, call and write. A refusal says which rule applied; do not try to work around it. If a call waits for approval, tell the user to run `vc approvals` and `vc approve <id>` in a terminal.
- `write_definition` never writes `vibranium.config.yaml`. Ask the user to change the config.
- MCP has no import, sync or load tool. Use the shell for those.

## Do

- Run `vc check` after every change, and before every run.
- Reuse apis: tests `call:` an api and override `vars`, `body` and `expect`. Add a new api only for a new endpoint.
- Give every created resource a `cleanup` on the api that creates it.
- Put environment differences in profile `vars`, not in tests.
- Keep secret values in `.env` (gitignored) and declare their names in `.env.schema`. Refer to them as `${secret.NAME}`.
- Use camelCase IDs, third-person verbs for test IDs, and a `name` that says what the test proves.

## Don't

- Don't write secrets, tokens or passwords into any YAML file or into `.env.schema`.
- Don't edit `openapi/<service>.yaml`: it is the snapshot `vc import` and `vc sync` maintain. Keep source specs elsewhere (such as `specs/`).
- Don't add `id:` keys to files, or `$` to `body.match` paths.
- Don't widen the `agents` block, or approve your own requests, to get past a refusal.
- Don't parse console text when a `--json` form exists.
