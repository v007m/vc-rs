# Vibranium v2 mockups — shared brief (read fully before building)

You are building design artboards for **Vibranium v2**, an open-source, git-native API testing tool (CLI `vibranium`, alias `vc`; terminal UI; web UI; HTML report). The owner is Sarath. Artboards live in a Design canvas; you only WRITE FILES on disk. Do NOT publish, do NOT call the Artifact tool, do NOT edit `project/canvas.json` (the coordinator does both). Report back the file names and each artboard's preview width × height.

Working folder (the canvas root): `/tmp/claude-0/-home-claude/80b4900a-8223-5c77-874b-c1dda2eeca25/scratchpad/vibmock/` — artboards go in `project/`.

## 1. File format (mandatory)
Read these first, completely:
- `/tmp/claude-0/-home-claude/80b4900a-8223-5c77-874b-c1dda2eeca25/scratchpad/designref/SKILL.md` (section "One artboard: the skeleton" and "Rules that bite")
- `/tmp/claude-0/-home-claude/80b4900a-8223-5c77-874b-c1dda2eeca25/scratchpad/designref/artifact-type/reference/format.md` (events, state, sc-if, sc-for, links)
- `/tmp/claude-0/-home-claude/80b4900a-8223-5c77-874b-c1dda2eeca25/scratchpad/designref/artifact-type/reference/craft.md` (accessibility, page rules)
Key rules that fail silently: keep `<script src="./support.js"></script>` exactly; close every element, quote every attribute; `{{hole}}` is a dotted lookup only — compute everything in `renderVals()`; events are `onClick="{{ fn }}"` with functions returned from `renderVals()`; per-item handlers are attached to each item in `renderVals()`; inputs use `value="{{x}}"` + `onInput="{{ fn }}"` (the handler receives the DOM event: `e.target.value`); no `innerHTML`, no script-built DOM, no `window.X` of your own, no network except the uploaded files below and a Google Fonts `css2` link; no emoji, icons are inline stroke SVG with `aria-hidden="true"`; real `<button>`, `<a href>`, `<input>`+`<label>`; text contrast ≥4.5:1. A cross-artboard link is `<a href="WebEditor.dc.html">` styled as the button (no button inside it).

Web screens and reports are PAGES: fluid root (no px width), they must use the full window width and work down to phone width (sidebar stacks above the panel, toolbars wrap, wide tables scroll inside an `overflow-x:auto` box, multi-column panes collapse). Their canvas frame will be 1440 wide; declare `$preview` width 1440 and the page's natural height at that width. Terminal screens are fixed-size (see their own brief).

## 2. The web look (both the web UI and the HTML report)
Inspired by the Arc / Dia browser layout pattern (original design, no copying of their assets): the whole window is a soft gradient with a fine grain; the header and side navigation sit FLAT on that background — no cards, no borders — only hovered/selected rows get a translucent white pill; exactly ONE surface is elevated: the main content panel, a large rounded white card with a soft shadow that fills the remaining space. Inside the panel there are no cards-in-cards: use hairline dividers, whitespace, tabs and light `--panel2` wells. Dense, calm, professional; uses all the space.

A shared stylesheet implements this. Link it in `<head>` right after the support.js line, together with Google Fonts in `<helmet>`:
```html
<head>
<meta charset="utf-8">
<title>…</title>
<script src="./support.js"></script>
<link rel="stylesheet" href="/_blob/1e69a6d642232c67587a282225e98669">
</head>
…
<helmet>
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Geist:wght@400;500;600;700&amp;family=Geist+Mono:wght@400;500;600&amp;display=swap">
<style>body{margin:0}</style>
</helmet>
```
The stylesheet source is at `vibmock/assets/vib-web.css` — read it; use its classes and CSS variables rather than re-inventing styles (you may add page-specific rules in your `<helmet><style>`). Main classes: `.app` (gradient+grain root, min-height 100vh), `.topbar`, `.brand`, `.crumbs`, `.search`, `.frame` (row holding side + panel, wraps on phone), `.side`, `.navlabel`, `.navitem` (+`.on`, `.count`), `.panel` (the elevated card), `.panel-head`, `.panel-body`, `.section`, `.split`/`.pane`/`.vr`, `.well`, `.btn` (+`.primary` `.accent` `.ghost` `.sm`), `.iconbtn`, `.tabs`/`.tab.on`, `.seg` (segmented control), `.field`/`.input`/`.select`/`.textarea` (+`.err`), `.help`, `.tbl`, `.num`, `.scrollx`, `.pill` (+`.pass .fail .block .skip .flaky .accent`), `.method` (+`.m-get .m-post .m-put .m-patch .m-delete`), `.dot`, `.code` with `.ln`/`.no`/`.hl`/`.err` lines and YAML token classes `.y-key .y-str .y-num .y-tpl .y-com .y-punc`, `.squig`, `.diag.error/.warn/.info`, `.popover`/`.opt`, `.scrim`/`.modal`, `.kbd`, `.mono`, `.muted`, `.subtle`.
Palette (variables): ink #1B1830, ink2 #4F4B66, accent violet #5440D6, statuses pass green, fail red, blocked amber, skipped grey, flaky blue. Fonts Geist + Geist Mono.

Uploaded files you may reference (verbatim URLs): the stylesheet above; grain texture `/_blob/9ea3af80b64659e41e7d942696a9a0d2` (already used by `.app`); js-yaml 4.1.0 `/_blob/5809f53fc2a29e786d4c63f8d536045b` — load it with `<script src="/_blob/5809f53fc2a29e786d4c63f8d536045b"></script>` in `<head>` after the stylesheet; it defines the global `jsyaml` (`jsyaml.load`, `jsyaml.dump`). Guard its use (`typeof jsyaml !== 'undefined'`) because it may load after first render.

### The web app shell (every web UI screen uses exactly this structure)
```
.app
  header.topbar: brand (hexagon logo SVG + "Vibranium") · project switcher button "shop ▾" · profile selector (Profile: staging) · .grow · .search ("Search tests, apis, files…" + kbd ⌘K) · Run button (.btn.primary) · avatar button "S"
  .frame
    nav.side: global nav — Tests (WebExplorer.dc.html), API client (WebClient.dc.html), Editor (WebEditor.dc.html), Runs (WebRun.dc.html), Insights (WebInsights.dc.html), Agents (WebAgents.dc.html); current one `.on`; each a real `<a class="navitem" href=…>` with a 16px stroke icon. Below it, a context section specific to the screen (file tree, api catalog, saved requests, filters…), also flat.
    main.panel: the screen's content.
```
Logo SVG (reuse): `<svg width="22" height="22" viewBox="0 0 28 28" aria-hidden="true"><path d="M14 2 25 8.5v11L14 26 3 19.5v-11Z" fill="none" stroke="#5440D6" stroke-width="2"/><path d="M9 10.5 14 18l5-7.5" fill="none" stroke="#1B1830" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>`

## 3. Product facts the content must respect
Files: `vibranium.config.yaml` (root), `*.api.yaml`, `*.test.yaml`, `*.load.yaml`, plus folders `schemas/` (JSON Schema), `datasets/` (CSV/JSON/YAML rows), `hooks/` (TS/JS functions), `openapi/` (committed spec snapshots written by import/sync), `.vibranium/` (reports, history.db, cache.db; git-ignored). Every item has camelCase `id` + free-text `name`; full ID `<fileId>.<id>` (api `users.create`, test `users.crud.getsCreatedUser`).

Template syntax: `${var.x}`, `${env.X}`, `${secret.X}`, `${response.body.id}`, `${<actionId>.body.total}`, `${example.x}`, `${item.x}`, `${iteration.index}`, `${fake.person.fullName}`, `${dataset.capitals}`, `${run.id}`; pipes `${var.price | number}`, `| int | bool | string | json | jsonString | base64 | lower | trim | default('x') | random | take(10)`. `$cel: <expr>` computes a value. Body: nested YAML/JSON, string, `$file`, `$base64`, `$extend` (+`$remove`).

Api file example:
```yaml
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
    method: DELETE
    path: /users/${var.userId}
    expect:
      status: [200, 204, 404]
```
Api keys: id, name, method, path, vars, query, headers, body, auth, timeout, expect, save, cleanup, operationId, deprecated, beforeRequest, afterResponse. File keys: id, name, baseUrl, auth, headers, vars, profiles, source (openapi, headers, include), beforeRequest, afterResponse, apis.

Test file example (services/orders/checkout.test.yaml):
```yaml
id: orders.checkout
name: Checkout
tags: [orders, checkout]
beforeAll:
  - id: catalog
    call: catalog.seedProducts
    cache:
      for: 12h
beforeEach:
  - call: users.create
  - call: carts.create
  - call: carts.addItem
    forEach: ${var.catalog.productIds}
    vars:
      sku: ${item}
tests:
  - id: placesOrder
    name: A cart with two items becomes a paid order
    steps:
      - id: place
        call: orders.place
      - id: fetch
        call: orders.get
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
          total: ${var.orderTotal}
      cel:
        total equals sum of lines: body.total == body.items.map(i, i.price * i.quantity).sum()
  - id: appliesCoupon
    skip:
      reason: coupon service flaky on staging, see #212
      until: 2026-11-15
    call: orders.place
    vars:
      couponCode: WELCOME10
```
Test keys: id, name, tags, before, after, call | steps, vars, auth, body, expect, save, poll, examples, repeat, retry, timeout, skip, standalone. Test-file keys: id, name, tags, vars, profiles, skip, beforeAll, afterAll, beforeEach, afterEach, tests. Actions (in lifecycle blocks and steps): `call: <apiId>` (normal), `test: <testId>` (reuse a multi-step flow), `function: path#export`, `command: <shell>`; action keys id, vars, auth, body, expect, save, poll, retry, keep, cache {for, check}, repeat, forEach, parallel.
expect keys: status, headers.<name>, body.schema, body.match.<path>, body.equals, size, timing.<phase>, cel.<label>, custom. Matchers: literal, eq ne gt gte lt lte between in notIn type exists empty length contains startsWith endsWith regex format every some none not allOf anyOf.
Config keys: syntaxVersion, project, paths, defaults (timeout, concurrency, perHost, retry, headers), vars, profiles (id, name, default, vars, agents), auth (basic, bearer, apiKey, oauth2ClientCredentials, oauth2AuthorizationCode, oauth2Refresh, flow, plugin), secrets (provider varlock, schema .env.schema), generators, datasets, reports, history, alerts, plugins, redact, beforeRun, afterRun.
Load file keys: id, name, mix (test, weight, thresholds), stages (duration, users), thinkTime, checks (api|full), thresholds (p50…p99, max, errorRate, rps), abortOn.
Profiles are mandatory (default profile `default`; the shop project has dev, staging, prod). Statuses: passed, failed, blocked (a lifecycle action it needs failed), skipped (with reason, optional until), flaky (passed on retry).

## 4. Naming conventions and folder structure (every "create" UI must follow and teach these)
| Thing | Convention | Example |
|---|---|---|
| Api file | `services/<service>/<service>.api.yaml`, file id = service, lowercase | `services/users/users.api.yaml`, id `users` |
| Test file | `services/<service>/<area>.test.yaml`, file id `<service>.<area>`, lowercase words joined by dots | `services/orders/checkout.test.yaml`, id `orders.checkout` |
| Load file | `load/<name>.load.yaml`, id `<area>.<scenario>` | `load/checkout.load.yaml`, id `checkout.peak` |
| Api id | camelCase, verb first, no service prefix (the file id provides it) | `create`, `get`, `list`, `startExport` → `users.startExport` |
| Test id | camelCase behaviour in third person, subject implied | `createsMember`, `rejectsBadEmail`, `getsCreatedUser` |
| Test name | a sentence | "Rejects malformed email addresses" |
| Vars / saved values | camelCase nouns; ids end in `Id`, URLs in `Url` | `userId`, `usersUrl`, `orderTotal` |
| Profiles | lowercase single words | `dev`, `staging`, `prod`, `eu` |
| Auth ids | camelCase nouns | `shopOauth`, `partnerLogin` |
| Tags | lowercase, kebab-case for two words | `smoke`, `checkout`, `slow-network` |
| Datasets | `datasets/<kebab-name>.csv`, id = camelCase of the stem | `world-cities.csv` → `worldCities` |
| Schemas | `schemas/<service>/<thing>.json`, kebab-case | `schemas/orders/order.json` |
| Hooks | `hooks/<purpose>.ts`, camelCase exports | `hooks/sign.ts#addUploadSignature` |
Creators (CLI, TUI, web) derive the id from the name the user types (`"Rejects a declined card"` → `rejectsDeclinedCard`), show the target file path, refuse invalid ids with a suggestion, and `vc check` warns on violations (`--fix` renames and updates every reference; `vc rename <old> <new>` does it explicitly).

## 5. Sample project "shop" (keep data consistent)
Services: users, orders, catalog, carts, geo, payments. Apis include users.create/get/list/update/delete/startExport/exportStatus/deactivate/rename, orders.place/get/cancel/refund, carts.create/addItem/get, catalog.seedProducts/getProduct/search, geo.createCity/deleteCity/createLocationGroup, payments.authorize. Test files: users.crud, users.reports, orders.checkout, orders.history, catalog.admin, catalog.browse, catalog.search, geo.groups, carts.pricing. Profiles dev/staging/prod; URLs like https://users.staging.shop.example. Auth `shopOauth` (oauth2 client credentials), `partnerLogin` (flow). Datasets: datasets/capitals.csv (name,country,population). Schemas: schemas/users/user.json, schemas/orders/order.json. Hooks: hooks/checks.ts (currencyIsConsistent), hooks/sign.ts (addUploadSignature), hooks/notify.ts (postSummary). Last full run #482 on staging: 412 tests, 396 passed, 6 failed, 4 blocked (beforeRun `catalog` → catalog.seedProducts 503), 3 flaky, 3 skipped, 48.2 s; failure root cause: orders now return a `tax` field and `total` 64.78 vs expected 59.98 (commit a1c9e4f "orders: add tax line" by priya). No lorem ipsum, no invented marketing copy.
