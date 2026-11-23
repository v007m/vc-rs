# Vibranium UI v2: design plan (#95, #98)

Status: draft for Sarath's review. The decisions are from the 2026-11-23 grilling and are logged as Round 88 (D470–D494) in `decision-log.md`. The mockups in `design/mockups/ui-v2/` follow this plan, and the build starts only after Sarath reviews both.

## 1. Goals

- **Two modes in one workspace:**
  - **Client** is a fast API client like Postman or REST Client.
  - **Tests** is a full test workbench: explorer, editor, runner, reports, load and insights.
- **Everything runs on the engine:** profiles, vars, auth, checks, history, diagnostics and fixes. The UI never re-implements engine logic.
- **One Svelte UI, two shells:**
  - Tauri 2 desktop (#98) comes first: a frameless window, with the engine in process over IPC.
  - The web `vc ui` uses the same layout in a browser tab, over Connect at `/api`.
  - The transport sits behind the existing per-feature ports, so the shell swaps it and nothing else changes.
- **Keyboard first, WCAG 2.1 AA, dark and light themes, fast with 10k tests.**

## 2. Visual system

The style follows the Arc browser.

### Layers
| Layer | What sits on it | Treatment |
|---|---|---|
| L0 background | sidebar, header, breadcrumbs, section labels, secondary text, status line | Vivid blended colour blobs that drift slowly, under a fine film grain. It looks matte metallic. Text sits directly on it with an AA-checked scrim |
| L1 glass card | the main content card (one per view; a split shows two) | Frosted glass, about 75% opaque with a strong blur. A 1 px inner top highlight, a soft outer shadow and a faint inner deboss rim |
| L2 raised control | buttons, segmented controls, inputs, chips, tabs | A smaller glass "pill" that is lifted on hover and debossed (inset shadow) when pressed or active |
| L3 overlay | command palette, popovers, dialogs, toasts | The strongest glass, with a backdrop dim |

### Tokens
- Every colour, radius, blur, shadow, space and type size is a CSS custom property in `tokens.css`. Components use tokens only.
- **Tint:** the blob palette is picked per project, and overridden per profile. Prod defaults to a red or amber tint, so a dangerous profile is visible at a glance. Tints are stored in UI prefs.
- **Themes:** dark is the default. Light is the alternative. Both are checked for AA on every layer pair, including text on L0 over the brightest blob.
- **Reduce motion:** the blobs stop drifting. **Reduce transparency** (or the in-app switch) makes L1 and L2 solid.
- **Type:** Geist and Geist Mono, self-hosted under the CSP. Their licence is OFL 1.1, which must be checked against JSLICENSE. The scale is 12/13/14/16/20/24 with tokens for each.
- **Grain:** a tiny tiled noise PNG, or an SVG `feTurbulence` baked to a data URI at build time (the CSP forbids inline styles but allows hashed CSS).

### Status language
These colours are shared with the CLI, TUI and HTML report: pass, fail, blocked, skipped, flaky and allowFailure. Every status has an icon as well as a colour, so status never relies on colour alone.

## 3. Shell and navigation

```
┌ L0: sidebar ──────────┬─ L0: header (breadcrumb · profile · run status) ────┐
│ ● ● ●  (traffic)      │ ┌ L1 glass card ─────────────────────────────────┐ │
│ [ Client | Tests ]    │ │                                                │ │
│ profile: dev ▾  tint  │ │   view for the selected item                   │ │
│ ⌕ Search      ⌘K      │ │                                                │ │
│ ── pinned tabs ──     │ │                                                │ │
│ ── today's tabs ──    │ │                                                │ │
│ ── tree ──            │ │                                                │ │
│ ── history ──         │ └────────────────────────────────────────────────┘ │
│ Runs Insights Agents ⚙│  L0 status line: engine · branch · last check      │
└───────────────────────┴────────────────────────────────────────────────────┘
```

- **Mode switch** (⌘1 Client, ⌘2 Tests) sits at the top of the sidebar.
  - Client mode's tree: services, then apis, then call history.
  - Tests mode's tree: services, then test files, then tests, with status dots from the last run.
- **Tabs follow Arc.** Pinned tabs stay. Other tabs close themselves after 12 h idle; the time can be changed. A closed scratch request stays in history.
  - ⌘T opens a new scratch tab, ⌘W closes a tab, ⌃Tab cycles tabs.
- **Profile picker:** one main profile, with "add layer" to stack more in order. A "config explain" popover shows the merged value of every key and where it was set. The tint follows the profile.
- **Shared screens** at the sidebar's foot: Runs, Insights, Agents and Settings.
- **Responsive:**
  - Above 1100 px the sidebar is open.
  - From 760 to 1100 px the sidebar collapses to an icon rail.
  - Below 760 px the sidebar is a drawer. This is web only and read-mostly.
- **Desktop shell:** the traffic lights sit in the sidebar. The window can be dragged from the L0 header. Native menus mirror the palette commands.

## 4. Client mode (R43)

### Request card
- The request and response sit side by side, with a draggable divider. Below 1100 px they stack. A toggle sets the orientation, and the choice is remembered.
- **URL bar:**
  - method or type pill;
  - the URL, with `${var}` tokens highlighted; hovering one shows its value and where it was set;
  - Send (⌘↵);
  - a ▾ menu with Send and keep, Copy as cURL and Copy as `vc call`.
- **Tabs:** Params, Headers, Auth, Body, Vars, Expect, Options.
  - Each shows **Inherited**, read-only and labelled with its source (api, file, config or profile), apart from **This request**.
- **Ad hoc requests:**
  - A new tab is a scratch request against the project's profiles, vars and auth.
  - **Save as api** writes `services/<svc>/<svc>.api.yaml` through a diff preview.
  - **Save as test** prefills checks from the response.
- **Body modes:** none, JSON, text, form, multipart (with `$file`), binary or `$base64`, and GraphQL.
  - GraphQL has a query editor, a variables pane, a schema explorer from introspection or a `.graphql` file, and an errors tab.
- **Auth tab:** pick any auth from the registry.
  - Shipped: basic, bearer, apiKey, oauth2 (client credentials, password, auth code with PKCE), flow, awsSigV4 and mTLS clientCert.
  - Greyed as coming: JWT, Digest and device code.
  - Test auth runs the token flow and shows the redacted token's claims.
- **Request-type registry:** http and GraphQL are fully drawn; SSE and WebSocket are live; gRPC and JSON-RPC are greyed as coming.

### Response card
- **Status line:** status, time, size, the protocol (h1 or h2), and retried or auth-refreshed badges.
- **Tabs:** Body, Headers, Cookies (read-only), Timing, Checks, Saved, Raw.
- **Body:**
  - tree view for JSON and XML, raw text, and a preview for HTML and images;
  - large bodies are virtualised;
  - a **JSONPath/CEL filter box** above the body uses the engine's evaluator;
  - **click-to-check:** select any value, then add a check (equals, type, exists, regex, oneOf, between, matches schema) or save the value as a var. This builds the `expect` and `save` blocks.
- **Timing waterfall:** DNS, connect, TLS, first byte and download, plus a list of the engine's own events (redirects, retries, auth refreshes, signing).
- **Copy as cURL:** credentials become `${secret.NAME}` placeholders by default. An "Include credentials" toggle needs a warning, and it resets after each copy.

### Streams (SSE and WebSocket)
- Connect and Disconnect.
- A live message list with time, direction, size and type. JSON frames can be expanded. The list can be filtered and paused.
- WebSocket has a composer for text, JSON or binary, with saved snippets.
- **Turn into an exchange check** converts the transcript into the api's `exchange` block.
- This needs a server-streaming `CallStream` RPC.

### History
- Every call, from the UI, the CLI's `vc call` or the TUI, is stored as an `adhoc` run in the project's `history.db`.
  - Bodies follow `history.bodies`.
  - Values are redacted as usual.
  - Retention follows the history config.
- The sidebar groups history by day. Each row shows the method, path, status and time.
- Clicking a row re-opens it in a tab. History can be searched.

### Import
A wizard with six steps: Source (OpenAPI, HAR, Postman, Bruno, .http, cURL paste), Filter, Map (services and names), Secrets (values found become `${secret}`), Also create (tests or checks), and Review (a diff of every file). It uses the existing `Import` RPC for every kind.

## 5. Tests mode (R32, R44)

### Explorer
- The tree and facets (service, tag, status, changed).
- Query syntax: `tag:smoke status:failed api:orders`, with OR, `-` to exclude, and saved views.
- Bulk actions: Run, Skip until, Tag, Quarantine, each with Undo.
- **Test detail** (one L1 card):
  - id, `path:line`, tags;
  - the lifecycle it inherits (beforeAll/afterAll and cleanups);
  - the apis it calls;
  - a **per-test history strip** of the last N results, with a duration sparkline and flake rate;
  - Run, Edit and Quarantine actions.
- **Quarantine** writes `allowFailure: true` plus an `until:` date into the test's YAML, shown as a diff first. There is no new model concept.

### Editor
- **Form view is the default. YAML (CodeMirror 6) is the alternative, and Split shows both.** All three edit one buffer, so comments and order survive a switch, because the form edits through the parser's minimal `TextEdit`s.
- **Form editors** exist for test files (tests, steps, examples grid, expect builder, save, lifecycle), api files, config (profiles, vars, http, tls, auth, alerts, plugins), load files (stages and thresholds) and datasets (a grid).
- **YAML view:**
  - highlighting;
  - autocomplete of ids, vars, matchers, pipes and keys;
  - hover shows the type, the value under the current profile and the docs;
  - live diagnostics as squiggles;
  - a quick fix for each certain `fix:`;
  - "Fix all" and "Explain Vxxxx" open the explain page in a side panel.
- **Save** (D227 is kept): saving always works.
  - If there are errors, the save shows a red badge and asks for confirmation with "Save anyway".
  - Saves are guarded by a digest, with a conflict banner (Reload or Keep mine).
  - PRD R44's text will be updated to match.
- **File operations:**
  - New service, api file or test file, from the same templates as `vc new`.
  - Rename uses `vc rename`, with a diff preview of every file it changes.
  - Format uses `vc fmt`.
  - Delete moves the file to the OS trash, after a confirmation that lists the tests that reference it.
- **Plan preview** (`vc plan --explain`) for the selection shows the instances, the lifecycle order and the merged expects.

### Runner
- **Run dialog** (or ⌘R on a selection):
  - selector, profiles, seed, repeat, bail, concurrency, strict and reporters;
  - "since commit" for `--changed`.
- **Live run view:**
  - plan progress, a now-running list, and pass/fail counters;
  - an event log with filters and pause;
  - **per-test drill-down while running**, using the same renderer as a stored run: exchanges, checks with expected, actual and the source span, and a body diff.
- **Stop** ends the run. The cleanups then run live, and a "Skip cleanups" button appears with a warning that created data will remain. This needs a two-level `Cancel` RPC.
- **After a run:** Re-run failed, Run again, Open report, Compare with previous.

### Load tests (`*.load.yaml`)
- A form editor for stages (ramp and hold), executors, thresholds and scenarios.
- **Live run:** RPS, latency p50/p95/p99 and the error rate as hand-built SVG charts, with threshold lines and breach markers. A **Hosts panel** shows the load generator's own CPU, memory, open connections and dropped iterations. The engine must collect this; it's a new `load_tick` field.
- **Report:** breaches, a latency histogram, the per-scenario split, and a comparison with earlier load runs.
- **RPC:** load fields on `RunRequest`, or a `RunLoad` stream. The plan picks `RunLoad`, because the event shape differs.

### Runs and reports
- **Runs list:** filter by status, profile, branch, kind (test, load, adhoc) and agent.
- **Stored run:** the same view as a live run.
- **Compare two runs:** newly failing, fixed, slower and faster tests (with a threshold), and changed response shapes.

### Insights
- A profile and branch scope, which the port already supports.
- KPIs: the pass rate trend, the duration trend, a flaky list with a Quarantine action, the slowest apis (p50/p95), api coverage gaps, skips that are expiring, and per-service cards.

## 6. Shared screens

### Agents
- MCP server status, and the snippets to connect Claude Code, Cursor and others.
- The approval queue.
- **What agents may do:** a toggle for each tool and each profile.
- An activity log that shows each run or edit with its diff.
- Runs made by agents carry an agent badge in Runs.

### Settings
- **Project** (`vibranium.config.yaml`):
  - general, paths, defaults, profiles, vars per profile, http, tls, auth (with Test), secrets (`.env.schema`), reports and history, alerts (with Send test alert), agent access and plugins.
  - Each has a form, plus a pending-changes bar with a diff.
- **You only** (`~/.config/vibranium`): the same form, clearly badged.
- **UI preferences:** theme, tint for each project and profile, motion and transparency, density and the tab idle time.

### Command palette (⌘K)
- Jump to any api, test, file, run or setting.
- Run any `vc` command with typed arguments and a preview: check, run with a selector, plan, fmt, rename, import, sync, explain a code, and new.
- Recent items come first.
- Every action has a shortcut, and `?` opens a cheat sheet.

### Onboarding
- If there's no project, offer an **Init wizard**, which runs `vc init` (name, first service, base URL, profile).
- Or offer **Open the demo**: `vc demo`'s mock API and project.
- Or offer **Import**.

## 7. Security
- **Session cookie:** a launch link is exchanged for a cookie that is HttpOnly, SameSite=Strict and, new, **Secure**. This works on localhost.
- **The CSP stays strict.** Fonts and CodeMirror are bundled, there's no CDN, and there are no inline styles.
- **Secrets are never rendered.** They show as `••••` with their source, and Reveal is never offered for `secret` values.
- The redactor applies to history, the copy actions and the agent log.
- **Every write is review first** (a diff) and digest guarded. Deletes go to the trash.
- **Desktop:** the Tauri IPC allow-list holds only the engine commands. There is no shell or fs plugin, and no remote content.

## 8. Engine work this needs (new RPCs and fields)

| # | RPC or field | For |
|---|---|---|
| 1 | Ad hoc `Call` shape (method, URL, headers, body, auth reference, with no api) | Scratch requests |
| 2 | `CallStream` (server streaming) and `StreamSend` | Live SSE and WS |
| 3 | `ListRuns(kind=adhoc)` and a history write for each UI call | Call history |
| 4 | `Complete`, `Hover`, `ApplyFixes`, `Explain` (already in the LLD table) | YAML editor |
| 5 | `Plan` | Plan preview |
| 6 | `CreateFile` (templates), `Rename`, `Format`, `TrashFile` | File operations |
| 7 | `Cancel {level: stop, abandon}` | Stop, then Skip cleanups |
| 8 | `RunLoad` stream; `load_tick` host stats | Load tests |
| 9 | `CompareRuns`, `TestHistory` | Compare runs, per-test strip |
| 10 | `Init`, `Demo` | Onboarding |
| 11 | `TestAlert`, `PluginList`/`Add`/`Remove` | Settings |
| 12 | User-config read and write (scoped) | Settings, the "You only" scope |
| 13 | `RunRequest`: strict and reporters | Run dialog |

## 9. Decisions (Round 88)

| D | Decision |
|---|---|
| D470 | One UI with two shells: Tauri 2 first (#98), and the web `vc ui`. The transport sits behind the ports |
| D471 | Arc-style layers L0–L3, tokens only; dark default plus light; a tint per project, overridden per profile; reduce motion and reduce transparency honoured |
| D472 | A sidebar mode switch (Client and Tests); Runs, Insights, Agents and Settings shared at the foot |
| D473 | Ad hoc requests are allowed, with Save as api (a new `Call` shape) |
| D474 | Form view by default; YAML with CodeMirror 6 as the alternative; Split shows both; one buffer |
| D475 | D227 is kept: a save with errors is allowed after a warning. PRD R44 is reworded |
| D476 | Live SSE and WS in the client (`CallStream`) |
| D477 | Load tests get full UI: an editor, a live run with a Hosts panel, and a report |
| D478 | Call history lives in `history.db` as `adhoc` runs |
| D479 | Quarantine = `allowFailure: true` + `until:`, written as a diff |
| D480 | Profile picker: one main profile plus optional stacked layers |
| D481 | File operations include delete to the OS trash |
| D482 | Settings edit both the project config and the user config |
| D483 | Calm glass over a rich background |
| D484 | ⌘K covers navigation and every `vc` command |
| D485 | Two-level stop (`Cancel` RPC) |
| D486 | Future kinds show as greyed registry slots |
| D487 | Insights add Compare runs, per-test history, and the load Hosts panel. "Commits between runs" is dropped |
| D488 | Onboarding offers the init wizard, the demo and import |
| D489 | Copy as cURL uses placeholders, with an opt-in toggle to include credentials |
| D490 | Agents stay a shared screen |
| D491 | Request and response sit side by side and stack automatically |
| D492 | The response viewer has tree, raw and preview, a filter, click-to-check, and a waterfall |
| D493 | Arc-style pinned and temporary tabs |
| D494 | The session cookie gets the `Secure` flag |

## 10. Mockups (`design/mockups/ui-v2/`)

- Static HTML, CSS and JS with fake data. There's no framework or CDN, and the CSP-style rules apply: no inline `style=`.
- One shared `tokens.css`, `shell.css` and `glass.css`. A theme and tint switcher sits in a dev corner.
- Published as a private artifact for review.

**Screens**, each with its key states:
1. Shell and empty or onboarding (Init wizard, demo, import).
2. Client: a REST request and response (tree, filter, click-to-check, waterfall).
3. Client: GraphQL (query, variables, schema explorer, errors).
4. Client: WebSocket live (messages, composer) and SSE.
5. Client: the auth tab (registry, greyed future entries), copy as cURL, history.
6. Client: the Import wizard.
7. Tests: explorer with facets, query, bulk actions, and test detail with its history strip.
8. Tests: editor in Form, YAML (diagnostics, autocomplete popup, hover) and Split views; Save anyway; rename diff.
9. Tests: the run dialog, the live run with drill-down, and Stop then Skip cleanups.
10. Tests: the load editor, the live load run with charts and the Hosts panel, and the load report.
11. Runs: the list, a stored run, and Compare.
12. Insights.
13. Agents.
14. Settings: project and "You only", with the pending-changes bar.
15. Command palette, the shortcut sheet, and toasts.
16. Light theme and a prod tint, shown on three screens.
17. Narrow widths: 1280, 1024 and 760 px.

**Review:** an artifact-design pass, an AA contrast check of every token pair, a keyboard walk, and screenshots at each width.

## 11. After the review: the build plan (outline)

1. Token, shell and glass refactor of `web/`, with fonts bundled and #114 fixed.
2. Client mode: ad hoc Call and history.
3. Response viewer extras.
4. Streams in the client.
5. Editor: forms and CodeMirror with Complete, Hover and Fix.
6. File operations and plan.
7. Runner: drill-down and Cancel.
8. Load UI.
9. Compare runs and per-test history.
10. Settings scopes, onboarding and the palette.
11. The Tauri shell (#98).

Each step is one or two PRs, and each RPC gets its own small engine PR first.
