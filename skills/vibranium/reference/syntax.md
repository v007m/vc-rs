# Vibranium syntax reference (for agents)

Short, checked examples of the forms used most. Each block passed `vc check` and ran against a demo users service. The full reference is at https://v007m.github.io/vc-rs/ (`/vc-rs/llms-full.txt` has the whole site as text).

## Api file

```yaml
# services/users/users.api.yaml  (file ID: users)
name: Users service
baseUrl: ${var.usersUrl}

apis:
  - id: listUsers            # GET is the default method
    name: List users
    path: /users
    expect:
      status: 200

  - id: createUser
    name: Create a user
    method: POST
    path: /users
    vars:                    # defaults; a test or `vc call --var` overrides them
      name: ${fake.person.fullName}
      email: ${fake.internet.email}
    body:
      name: ${var.name}
      email: ${var.email}
    expect:
      status: 201
    save:                    # becomes ${var.userId} for what runs next in the test
      userId: ${response.body.id}
    cleanup:                 # runs after the test that created it
      call: deleteUser       # an api in the same file
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
      status: [204, 404]     # any of these

  - id: echo
    name: Echo a body
    method: POST
    path: /echo
    body:
      ok: true
```

## Test file

```yaml
# services/users/crud.test.yaml  (file ID: users.crud)
name: User CRUD
tags: [users, smoke]         # every test in the file gets these

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
    before:                  # runs first; its `save` sets ${var.userId}
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
    examples:                # one instance per row: users.crud#rejectsBadEmail:1, :2
      - name: No Email
      - name: Also No Email
    call: users#createUser
    body:                    # replaces the api's body
      name: ${example.name}
    expect:
      replace: true          # drop the api's `status: 201`
      status: 400

  - id: echoesBody
    name: Echoes the body it gets
    call: users#echo
    expect:
      body:
        match:
          method: POST
          body.ok: true      # dotted path into the JSON
```

More checks:

```yaml
# services/users/extra.test.yaml  (file ID: users.extra)
name: Extra checks
tags: [users]

tests:
  - id: returnsJson
    name: Returns JSON
    tags: [smoke]
    call: users#listUsers
    expect:
      headers:
        content-type:
          contains: application/json
      body:
        match:
          "[0].name":
            contains: Ada

  - id: findsAdaByQuery
    name: Finds Ada by query
    call: users#listUsers
    query:
      name: Ada
    expect:
      status: 200

  - id: skipsForNow
    name: Skips for now
    skip: not ready yet      # shown as skipped; `vc run --include-skipped` runs it
    call: users#listUsers
```

Whole-body equality:

```yaml
expect:
  status: 200
  body:
    equals:
      id: "2"
      name: Grace Hopper
```

## Steps and secrets

```yaml
# services/users/flow.test.yaml  (file ID: users.flow)
name: User flows
tests:
  - id: createsThenReadsUser
    name: Creates a user, then reads it back
    headers:                 # on the test, not at the top of the file
      authorization: Bearer ${secret.USERS_TOKEN}
    steps:
      - id: create
        call: users#createUser
        vars:
          email: grace@example.com
      - id: read
        call: users#getUser
        vars:
          userId: ${create.body.id}   # a step's response, by step id
        expect:
          body:
            match:
              id: ${create.body.id}
    save:
      readId: ${response.body.id}
```

Secrets come from the provider in the config (`secrets.provider: env` reads the environment and `.env`). Declare each name in `.env.schema` (`USERS_TOKEN=`) and keep the value in `.env`, which is not committed. A missing value fails the test at run time with V5111 "`secret.USERS_TOKEN` is missing".

## Load scenario

```yaml
# load/users/browse.load.yaml  (scenario ID: users.browse)
name: Users browse mix
mix:
  - test: users.crud#listsUsers
    weight: 3
  - test: users.crud#getsCreatedUser
    weight: 1
stages:
  - duration: 3s
    users: 5
  - duration: 2s
    users: 0
thresholds:
  p95:
    lt: 200ms
  errorRate:
    lt: 1%
abortOn: [errorRate]
```

`vc load users.browse` exits 3 when a threshold is breached. `--json` prints one result object.

## Template sources

| Form | Value |
| --- | --- |
| `${var.name}` | A var. The most specific wins: the test's `vars`, then the api's, then the active profile's, then the config's |
| `${secret.NAME}` | A secret from the configured provider |
| `${fake.person.fullName}`, `${fake.internet.email}`, `${fake.number.int(1, 100)}`, `${fake.lorem.word}` | Generated data, repeatable with `--seed` |
| `${response.body.id}` | The current response, in `save` and `cleanup` |
| `${<stepId>.body.id}` | An earlier step's response |
| `${example.name}` | The current `examples` row |

## Selectors

| Selector | Picks |
| --- | --- |
| `users.crud` | Every test in a file |
| `users.crud#getsCreatedUser` | One test |
| `users.crud#rejectsBadEmail:2` | One example row (rows count from 1) |
| `-t smoke` | Tests with a tag; `!` excludes, `,` means or |
| `-g created` | Tests whose ID or name contains the text |
| `--changed [ref]` | Tests in files changed since `ref`, or uncommitted; changing an api file picks every test that calls it |
| `--failed` | Tests that failed or were blocked in the last run |

They work with `vc run`, `vc plan` and `vc list tests`.
