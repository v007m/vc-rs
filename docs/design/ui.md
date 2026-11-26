# Vibranium UI v2: design plan (#95, #98)

Status: approved by Sarath; the build is under way (§13). The decisions are from the 2026-11-23 grilling and the mockup reviews, logged as Rounds 88–92 (D470–D536) in `decision-log.md`. The mockups in `design/mockups/ui-v2/` follow this plan.

## 1. Goals

- **Five spaces in one workspace:**
  - **Client** is a fast API client like Postman or REST Client, for trying calls out.
  - **Tests** holds every api and load test to read, edit, check and commit.
  - **Runs** holds every run, schedule and report, with the Overview.
  - **Agents** and **Settings** hold agent access and every setting.
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
| L0 background | sidebar, header, breadcrumbs, section labels, secondary text, status line | Muted colour blobs blended into the near-black (or pale) base, drifting slowly under a visible film grain. It looks matte metallic, not neon. The sidebar, header and status line have no panel of their own: their text sits straight on L0 |
| L1 glass card | the main content card (one per view; a split shows two) | Liquid glass: a light frost over a tinted base (about 20% in dark, 25% in light), so the blobs clearly show through, with a strong blur and saturation, a soft specular top edge, an inner rim and a soft outer shadow (D505) |
| L2 raised control | buttons, segmented controls, chips, tabs | A smaller glass "pill" that is lifted on hover and debossed (inset shadow) when pressed or active |
| L3 overlay | command palette, popovers, dialogs, toasts | The strongest glass (about 62%), with a backdrop dim |

Inside a card:
- **One card level.** Nothing inside a card gets its own box. Group with spacing, section labels and hairline rules; tables are a header row plus row rules.
- **Only inputs are filled.** Text inputs, selects, the URL field, key/value cells and the YAML editor are debossed: darker than the card, with an inset top shadow. Read-only values are plain text.
- **Viewers are wells.** Read-only code, JSON, diffs and logs sit in a padded debossed well, like an input.
- Wrappers that are not cards, such as the URL bar row, have no background or border.
- **Selection** is a faint tint, never a filled box. No component has a leading-edge bar; a changed field shows a small dot (D506).
- **Focus rings** show for keyboard use only (`:focus-visible`).
- **Tabs** (D507) have one style everywhere: the selected tab is a raised L2 pill, with no underline.

Density (D497):
- Spacing comes from one scale: 4/8/12/16/24/32. Cards have 24 px padding; rows are 40–44 px.
- At most one chip per row and at most three visible actions per area; the rest go in a ⋯ menu.
- Paths, file IDs and other secondary detail live in tooltips. Detail sections start collapsed. Log and run rows are one line.
- An empty pane keeps its place and shows one quiet line, with at most one action.

### Tokens
- Every colour, radius, blur, shadow, space and type size is a CSS custom property in `tokens.css`. Components use tokens only.
- **Accent and buttons (D496):** primary buttons are monochrome: near-white in dark, near-black in light. The accent is a calm blue, used only for focus rings, links, progress and toggles. Profile dots: local blue, dev teal, staging amber, prod red.
- **Tint:** the blob palette is picked per project, and overridden per profile. Prod defaults to a red or amber tint, so a dangerous profile is visible at a glance. Tints are stored in UI prefs.
- **Themes:** dark is the default. Light is the alternative. Both are checked for AA on every layer pair, including text on L0 over the brightest blob.
- **Reduce motion:** the blobs stop drifting. **Reduce transparency** (or the in-app switch) makes L1 and L2 solid.
- **Type:** Geist and Geist Mono, self-hosted under the CSP. Their licence is OFL 1.1, which must be checked against JSLICENSE. The scale is 12/13/14/16/20/24 with tokens for each.
- **Grain:** visible at 100% zoom in both themes; a tiny tiled noise PNG, or an SVG `feTurbulence` baked to a data URI at build time (the CSP forbids inline styles but allows hashed CSS).

### Status language
These colours are shared with the CLI, TUI and HTML report: pass, fail, blocked, skipped, flaky and allowFailure. Every status has an icon as well as a colour, so status never relies on colour alone.

## 3. Shell and spaces

```
┌ rail ┬ L0: sidebar ─────────┬─ L0 top row: tabs, or title · status · primary ·· ENV ● local ▾  ⌕ Search ⌘K ┐
│ ●●●  │ space's views,       │ ┌ L1 glass card ─────────────────────────────────────────────────────────┐ │
│ ⇢ Cl │ filters and          │ │   the selected screen (fills to the status line)                      │ │
│ ▤ Te │ collapsible sections │ │                                                                       │ │
│ ▶ Ru │ (Requests / History, │ │                                                                       │ │
│ ⚇ Ag │  Tests / Changes)    │ └───────────────────────────────────────────────────────────────────────┘ │
│ ⚙ Se │                      │  L0 status line: version · branch · vc check                               │
└──────┴──────────────────────┴────────────────────────────────────────────────────────────────────────────┘
```

- **Five spaces** (D512): Client, Tests, Runs, Agents and Settings. The app opens in Client, and each space remembers its last screen.
  - **Client:** try requests. Open tabs, the api tree, call history, Save as test. Not for serious testing.
  - **Tests:** every api and load test, read and edit, check runs, and the git pane.
  - **Runs:** every run, live and stored, API and load; reports, compare, schedules and the Overview (the former Insights).
  - **Agents:** approvals, activity, usage, permissions and connection.
  - **Settings:** every setting, by scope.
- **Space switcher** (D513): a left icon rail on L0 by default, with the traffic lights on top, the five spaces with small labels (⌘1–⌘5) and Settings pinned at the bottom (⌘,). It stays when the sidebar is collapsed. Settings › This app › Appearance offers **Sidebar bottom** instead: the five icons in one row at the sidebar's foot.
- **One bar** (D514): the 44 px top row is the only bar above the cards. Client shows its tab strip there; other spaces show the screen title (the crumbs without the project), at most one status and one primary action. The env pill and search sit at its right in every space. There is no header band or sub-nav: a space's views live in its sidebar.
- **Sidebar sections** (D515): a sidebar stacks collapsible sections. A closed section keeps only its header; open sections share the height equally.
- **Pinned and temporary tabs.** Pinned tabs stay. Other tabs close themselves after 12 h idle; the time can be changed. A closed scratch request stays in history.
  - ⌘T opens a new scratch tab, ⌘W closes a tab, ⌃Tab cycles tabs.
  - Each open tab has a 1 px hairline border and no fill; the current tab has a stronger border and full-ink text (D524).
- **Env pill** (D504): an ENV caption, the profile dot, the name and layers, and a chevron. On prod it turns red and reads LIVE · PROD. Its popover holds the main profile, "add layer" to stack more in order, "Explain merged config" (the merged value of every key and where it was set) and "Edit profiles". The tint follows the profile.
- **Search** (⌘K), top right, has a filter icon inside the field that opens its own popover (All / REST / GraphQL / Streams); the icon gets a dot while a filter is on.
- **Rename** (D500): request, scratch and tab names are edited in place (pencil on hover, Enter saves, Esc cancels); double-click an open tab to rename it.
- **Responsive:**
  - Widths are those of the app, not the window: the shell uses container queries (D530).
  - Above 1100 px the sidebar is open, and hiding it is a saved pref.
  - At 1100 px and below, the sidebar is a drawer over the stage, opened from the top row. The rail stays, so a space is always one click away.
  - Below 900 px search collapses to an icon.
  - Below 760 px the margins tighten. This is web only and read-mostly.
  - Screens size their own layouts with `@container card`, the L1 card's width.
- **Desktop shell:** the traffic lights sit at the top of the rail, or of the sidebar with Sidebar bottom. The window can be dragged from the top row. Native menus mirror the palette commands.

## 4. Client space (R43)

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
- **Tabs:** Body, Headers, Cookies (read-only), Timing, Checks, Saved. The raw exchange is "View raw exchange" in the card's ⋯ menu.
- **Before the first send** (D501) the pane keeps its place and shows one quiet line, "Send the request to see the response ⌘↵", plus "Show last response" when history has one.
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
- The sidebar's History section groups calls by day, with an "All history" link. Each row shows the method, path and status.
- Clicking a row re-opens it in a tab. History can be searched.

### Import
A wizard with six steps: Source (OpenAPI, HAR, Postman, Bruno, .http, cURL paste), Filter, Map (services and names), Secrets (values found become `${secret}`), Also create (tests or checks), and Review (a diff of every file). It uses the existing `Import` RPC for every kind.

## 5. Tests space (R32, R44)

### Sidebar and read view (D516)
- **Sidebar:**
  - an All / API / Load filter;
  - a filter popover (status, tags, service, changed, saved views), shown as removable chips;
  - the **Tests** section: one tree of api and load tests, with status dots from the last run, topped by "All tests";
  - the **Changes** section (the git pane, below).
- **Query syntax** in the table and the popover: `tag:smoke status:failed api:orders`, with OR, `-` to exclude, and saved views.
- **All tests** opens the table, with bulk actions: Run, Skip until, Tag, Quarantine, each with Undo.
- **Test detail** (one L1 card at full width):
  - id, `path:line`, tags;
  - the lifecycle it inherits (beforeAll/afterAll and cleanups);
  - the apis it calls;
  - a **per-test history strip** of the last N results, with a duration sparkline and flake rate; both fill the card (D525);
  - Run (a check run), Edit, and Quarantine in ⋯.
- A **file** shows its lifecycle and its tests. A **load test** shows its scenario, stages and thresholds, with Edit and a Quick check.
- **Quarantine** writes `allowFailure: true` plus an `until:` date into the test's YAML, shown as a diff first. There is no new model concept.

### Check run (D517)
- Run in Tests checks one test inline: status, time, each check and the exchange, labelled "Check run, not saved to Runs".
- A load test offers a **Quick check**: 1 user for 10 s, showing req/s, p95 and errors.
- "Run in Runs…" in ⋯ starts a recorded run.

### Changes: the git pane (D518)
- The branch, ahead and behind, and the changed test files (M, A, D) with +/- counts; a file opens its diff.
- Pick files, write a message, Commit. Push, Pull and Commit & push sit in a menu.
- The server does this through the `Vcs` port, which gains status, diff, commit, push and pull. Agents cannot commit or push.

### Editor
- **Edit** on a test, file or load test opens its editor in place; Done or Esc returns to the read view. There is no separate Editor page.
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

## 6. Runs space (D519)

- **Sidebar:** Overview, All runs (filters: kind, status, profile), Schedules, and the recent runs with a live run first.
- **Run…** is the top row's primary action (⌘R).

### Run dialog and live run
- **Run dialog** (or ⌘R on a selection; D502): a 560 px form.
  - Visible: selector, profile, concurrency, bail and a one-line plan ("17 tests · 4 files · ~2 s").
  - Under a collapsed "More options": layers, "since commit" for `--changed`, seed, repeat, strict, reporters and the command preview.
  - Footer: Cancel and Run.
- **Live run view:**
  - plan progress, a now-running list, and pass/fail counters;
  - an event log with filters and pause;
  - **per-test drill-down while running**, using the same renderer as a stored run: exchanges, checks with expected, actual and the source span, and a body diff.
- **Stop** ends the run. The cleanups then run live, and a "Skip cleanups" button appears with a warning that created data will remain. This needs a two-level `Cancel` RPC.
- **After a run:** Re-run failed, Run again, Open report, Compare with previous.

### Load runs (`*.load.yaml`)
- The load editor (stages, ramp and hold, executors, thresholds and scenarios) lives in Tests; its runs open here.
- **Live run:** RPS, latency p50/p95/p99 and the error rate as hand-built SVG charts, with threshold lines and breach markers. A **Hosts panel** shows the load generator's own CPU, memory, open connections and dropped iterations. The engine must collect this; it's a new `load_tick` field.
- **Report:** breaches, a latency histogram, the per-scenario split, and a comparison with earlier load runs.
- **RPC:** load fields on `RunRequest`, or a `RunLoad` stream. The plan picks `RunLoad`, because the event shape differs.

### Runs and reports
- **Runs list:** filter by status, profile, branch, kind (test, load, adhoc) and agent.
- **Stored run** (D509): a summary header (status, counts, duration, profile, branch @ commit, start time) with Open report, an Export menu (HTML, CTRF JSON, JUnit XML, NDJSON, copy `vc report`) and a ⋯ (Re-run failed, Compare, Copy run command); below it, the same view as a live run. Each runs-list row has these actions in its ⋯.
- **Compare two runs:** newly failing, fixed, slower and faster tests (with a threshold), and changed response shapes.

### Overview (the former Insights)
- A profile and branch scope, which the port already supports.
- KPIs: the pass rate trend, the duration trend, a flaky list with a Quarantine action, the slowest apis (p50/p95), api coverage gaps, skips that are expiring, and per-service cards.

### Schedules (D520)
- Stored in `vibranium.config.yaml` under `schedules:` (name, selector, profile, cron, notify), so they are in git.
- The vc server runs them while it is up (`vc ui` locally, or `vc serve` on a shared machine) and records each in run history.
- **Export to CI** writes a GitHub Actions cron workflow, for machines that sleep.
- The list shows the cron in words, the selector, profile, runner, next run and last result, with an on/off switch. New schedule saves through a diff.
- The mockup proposes two more keys per schedule, `runner: vcServer | githubActions` and `enabled: false`. Their exact syntax goes into `syntax-reference.md` when the server is built (D520).

## 7. Agents and Settings spaces

### Agents (D521)
Five views in the sidebar, each opening with one sentence on what it is for:
- **Approvals:** requests that wait for an answer; each row expands to a diff with Approve and Reject.
- **Permissions:** a switch for each tool and each profile (prod off by default); edits apply through "Review N changes".
- **Connection:** MCP server status, connected clients, and the snippets to connect Claude Code, Cursor or HTTP.
- **Activity:** a one-line log of each run or edit, with a filter.
- **Usage:** requests, tool calls, runs started and time per agent over 7 days, with a table per agent and tool.
- Runs made by agents carry an agent badge in Runs.

### Settings (D522)
- Scopes and sections sit in the sidebar, grouped by scope; the main area shows one section at full width.
- **Project** (`vibranium.config.yaml`):
  - general, paths, defaults, profiles, vars per profile, http, tls, auth (with Test), secrets (`.env.schema`), reports and history, alerts (with Send test alert), agent access and plugins.
  - Each has a form, plus a pending-changes bar with a diff.
- **You only** (`~/.config/vibranium`): the same form, clearly badged.
- **This app:** theme, tint for each project and profile, motion and transparency, density, the tab idle time and the space switcher (Icon rail or Sidebar bottom).

## 8. Shared overlays

### Command palette (⌘K)
- Jump to any api, test, file, run or setting.
- Run any `vc` command with typed arguments and a preview: check, run with a selector, plan, fmt, rename, import, sync, explain a code, and new.
- Recent items come first.
- Every action has a shortcut, and `?` opens a cheat sheet.

### Onboarding
- If there's no project, offer an **Init wizard**, which runs `vc init` (name, first service, base URL, profile).
- Or offer **Open the demo**: `vc demo`'s mock API and project.
- Or offer **Import**.

## 9. Security
- **Session cookie:** a launch link is exchanged for a cookie that is HttpOnly, SameSite=Strict and, new, **Secure**. This works on localhost.
- **The CSP stays strict.** Fonts and CodeMirror are bundled, there's no CDN, and there are no inline styles.
- **Secrets are never rendered.** They show as `••••` with their source, and Reveal is never offered for `secret` values.
- The redactor applies to history, the copy actions and the agent log.
- **Every write is review first** (a diff) and digest guarded. Deletes go to the trash.
- **Desktop:** the Tauri IPC allow-list holds only the engine commands. There is no shell or fs plugin, and no remote content.

## 10. Engine work this needs (new RPCs and fields)

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
| 14 | `Vcs` status, diff, commit, push and pull (UI only, never agents) | The git pane |
| 15 | Schedules: list, write (through the config diff), the server's scheduler, `ExportSchedule` (GitHub Actions) | Schedules |
| 16 | `AgentUsage` (per agent and tool, over a window) | Agents › Usage |
| 17 | A check run: `Run` with `record: false` | Check run in Tests |

## 11. Decisions (Round 88)

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

Round 89 (the second and third mockup reviews):

| D | Decision |
|---|---|
| D495 | Clearly translucent frost glass; overlays about 72%; AA kept |
| D496 | Monochrome primary buttons; a calm blue accent; purple retired |
| D497 | Density: one spacing scale, 24 px card padding, one chip per row, three visible actions, the rest in ⋯ |
| D498 | The sidebar holds a profile popover and a search filter popover; pinned tiles and duplicate nav are removed |
| D499 | A fixed 48 px header band for the sub-nav, one status and one action |
| D500 | Names are renamed in place |
| D501 | A quiet empty response state with "Show last response" |
| D502 | The run dialog is a short form with "More options" |

Round 90 (the fourth mockup review):

| D | Decision |
|---|---|
| D503 | Open tabs, the env pill and search move to a top row; the sidebar gets a Requests / History switch |
| D504 | The env pill; red LIVE · PROD on prod |
| D505 | Clearer glass: about 20% / 25%, overlays 62% |
| D506 | No leading-edge bars; keyboard-only focus rings |
| D507 | One pill tab style, no underline |
| D508 | Cards and large editors fill to the status line; a 36 px request bar |
| D509 | Stored runs get a report header with Open report and Export |
| D510 | Agents: Approvals, Permissions, Connection, Activity |
| D511 | History strips fill their column |

Round 91 (the fifth mockup review: five spaces):

| D | Decision |
|---|---|
| D512 | Five spaces: Client, Tests, Runs, Agents, Settings; opens in Client |
| D513 | A left icon rail switches spaces; "Sidebar bottom" is the option |
| D514 | The top row is the only bar; no header band or sub-nav |
| D515 | Collapsible sidebar sections that share the height |
| D516 | Tests: kind filter, filter popover, one tree, full-width read view, Edit in place |
| D517 | Check runs in Tests are not saved to Runs; load Quick check |
| D518 | The git pane in Tests |
| D519 | Runs: Overview, All runs, Schedules, recent runs, Run… |
| D520 | Schedules in config, run by the vc server, Export to CI |
| D521 | Agents' views in the sidebar, plus Usage |
| D522 | Settings' sections in the sidebar |
| D523 | No purple: blue, graphite and orange |
| D524 | Open tabs have a hairline border, no fill |
| D525 | Graphs fill their card |

## 12. Mockups (`design/mockups/ui-v2/`)

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
7. Tests: the sidebar (kind filter, filter popover, tree, Changes), the all-tests table with bulk actions, and the test, file and load read views with a check run.
8. Tests: editor in Form, YAML (diagnostics, autocomplete popup, hover) and Split views; Save anyway; rename diff.
9. Runs: the run dialog, the live run with drill-down, and Stop then Skip cleanups.
10. Runs: the live load run with charts and the Hosts panel, and the load report.
11. Runs: the list, a stored run, Compare, the Overview and Schedules (new, Export to CI).
12. Agents: Approvals, Activity, Usage, Permissions, Connection.
13. Settings: project and "You only", with the pending-changes bar; the space switcher option.
14. The git pane: changes, diff, commit and push.
15. Command palette, the shortcut sheet, and toasts.
16. Light theme and a prod tint, shown on three screens.
17. Narrow widths: 1280, 1024 and 760 px.

**Review:** an artifact-design pass, an AA contrast check of every token pair, a keyboard walk, and screenshots at each width.

## 13. After the review: the build plan (outline)

1. Token, shell and glass refactor of `web/`, with fonts bundled and #114 fixed.
2. The five-space shell (rail, sidebar sections, top row), then Client: ad hoc Call and history.
3. Response viewer extras.
4. Streams in the client.
5. Editor: forms and CodeMirror with Complete, Hover and Fix.
6. File operations and plan.
7. Runner: drill-down and Cancel.
8. Load UI.
9. Compare runs, per-test history, check runs and the git pane.
10. Schedules and Agents usage.
11. Settings scopes, onboarding and the palette.
12. The Tauri shell (#98).

Each step is one or two PRs, and each RPC gets its own small engine PR first.

**Done so far (batch A, Round 92):**

- **Step 1.** The tokens, the L0 background, the glass and the controls are in `web/src/lib/styles/` (`tokens`, `glass`, `controls`, `status`, `shell`, `sidebar`). `legacy.css` maps the v1 token names onto v2 ones until each screen is redesigned. Geist and Geist Mono are bundled (D526), and #114 is fixed (D534).
- **Step 2, shell part.** The five-space shell is in `web/src/lib/shell/`: the rail, sidebars, top row and status line, saved UI prefs, and the shortcuts. The existing screens sit inside it at the routes in D527. Each sidebar lists only the views that exist today, one section per space.
- **Not built yet.** These come in later steps:
  - the Client tab strip and the per-space designs (the Tests tree in its sidebar, the git pane, the Runs sidebar and schedules)
  - the env pill popover and the search palette
  - the Appearance setting for the tint and the switcher
  - the branch in the status line (D532)
