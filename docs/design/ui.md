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

The UI is built on the Strata design system (D590), adopted at Sarath's request on 2026-11-26. It replaces the earlier Arc-style graphite look (Rounds 88–92). Where Sarath's rules and Strata differ, Sarath's rules win, and the difference lives in one file.

### Sources
- `web/src/lib/styles/strata/tokens.css` and `strata.css` are Strata 1.0.0, vendored verbatim. Each has a three-line header with the upstream file's SHA-256, and Prettier skips them. They are never edited; an update is a plain copy (D592).
- `strata/vibranium.css` holds everything Vibranium changes or adds: token overrides, the `--vc-*` tokens (status, methods, YAML, diffs, waterfall), recipe overrides and the recipes Strata lacks. Its part 5 holds Sarath's Strata 1.1 deltas until upstream 1.1 tokens land (D624). `shell.css` and `sidebar.css` lay out the shell with Strata recipes.
- Components use tokens only: `--st-*` from Strata, `--vc-*` from Vibranium.

### Layers
| Layer | What sits on it | Treatment |
|---|---|---|
| Canvas (`.st-canvas`) | rail, sidebar, top row, section labels, status line | Four soft colour blobs over the base, under a matte grain (a local `grain.svg`, since the CSP blocks `data:`; D602). Pastel in light; charcoal with deep blue, slate and burnt orange in dark. Chrome has no boxes: no panel, border or fill on the sidebar, tabs, labels, icons or toolbar |
| Card (`.st-card`) | the main content (one per view; a split shows two) | Glass that shows the canvas (white at 42 % in light, the slate at 45 % in dark; D608), solid as a viewer option (`data-cards="solid"`) or with reduced transparency. Dark cards are a slate (`#253137` solid), never black or neutral grey (D601). Never a card inside a card: a surface inside a card is a wash with no border (D609) |
| Overlay (`.st-overlay`) | menus, popovers, dialogs, toasts, the palette | The strongest surface, as native `<dialog>` and `[popover]` elements. It grows out of what opened it (D618) |

### Spaces
Each space sets `data-space` on the app, which picks the blobs and the accent (D591): Tests lilac, Client tide, Runs ember, Agents grove, Settings graphite. The primary button, focus rings, selected controls and native checkboxes take the space's accent.

### Layout and shape
- Cards sit 8 px from the window on the top, right and bottom (10 px above 1600 px wide), and 0 on the sidebar side. A split has an 8 px gap with its handle in the gap (D594).
- Corners are concentric, inner = outer − gap: window 20 px, cards 12, overlays 18 with 12 px items, the palette 22 with 14 px items, dialogs 28; a surface on a card takes 8 (D610).
- Every single-line control is a capsule: fields, selects (with a gradient-drawn chevron, D621), the Env control and search. Multi-line fields are 8 px rounded rects. Controls are 28, 32 (default) or 40 px, border-box, and a row's items share one height; badges never sit in a control row (D611).
- Empty panes show the empty state: icon, Title3 title and caption, centred in a wash (D609).
- In a row every item has the same height, with icon and text centred; icons have no border or background (D596).
- Buttons have one emboss, and a long label wraps instead of clipping. The primary button is the space's accent, one per view, last at the bottom right with Cancel-like actions before it; each Settings section ends in a footer on the card's own surface. Fields are capped in width, not stretched (D597, D616).
- The switch and slider are iOS liquid glass: a capsule track and a glassy debossed thumb. The segmented control has one glass lens that springs between options (D598, D615). Badges are capsules, larger than 18 px.
- Focus: buttons and toggles get a 2 px accent outline at 2 px offset; a text field gets a 1.5 px accent ring on itself (D612).
- Settings rows are left aligned, at least 44 px tall. Separators stop 12 px short of both ends everywhere (D603, D614).
- Dialogs have a Title2 heading and actions at the bottom right, Cancel first; focus stays inside and Esc cancels. Toasts are capsules at the bottom centre for about 3 s, with an optional Undo (D604).
- The search capsule in the top row is embossed, shows its shortcut and opens the command palette (D595, D620).

### Type, motion and preferences
- **Type:** Strata's SF Pro-first stack, with Inter (variable, OFL 1.1) bundled as the fallback off Apple platforms; the mono stack is Strata's (SF Mono, then the system monospace). Roles are Strata's (`--st-type-*`): micro, caption, label, body, Title3, Title2 (D599).
- **Themes:** the OS setting by default; `data-theme` forces light or dark.
- **Motion** uses two springs as `linear()` curves, 480 ms with a little overshoot and 280 ms critically damped for closing (D617). Overlays grow out of their anchor with a translate and a clip, never a scale (D618); symbols are written on, rotate, bounce or wiggle like SF Symbols (D619). Reduced motion stops the blobs and transitions and shows end states. `data-transparency="reduced"` makes cards and overlays solid; `data-density="compact"` tightens rows. The Appearance settings that set these come later (D605).
- **Keyboard hints** are plain tertiary text, never keycaps: `⌘K` on Apple platforms, `Ctrl+K` elsewhere. Holding the modifier alone for 450 ms shows each space's shortcut beside its rail button. F6 and Shift+F6 move between the regions: rail, sidebar, top row and main (D600).

### Status language
These colours are shared with the CLI, TUI and HTML report: pass, fail, blocked, skipped, flaky and allowFailure. Every status has an icon as well as a colour, so status never relies on colour alone.

## 3. Shell and spaces

```
┌ rail ┬ canvas: sidebar ─────┬─ top row: tabs, or title · status · primary ·· ENV ● local ▾  ⌕ Search ⌘K    ┐
│ ●●●  │ space's views,       │ ┌ glass card ────────────────────────────────────────────────────────────┐ │
│ ⇢ Cl │ filters and          │ │   the selected screen (fills to the status line)                      │ │
│ ▤ Te │ collapsible sections │ │                                                                       │ │
│ ▶ Ru │ (Requests / History, │ │                                                                       │ │
│ ⚇ Ag │  Tests / Changes)    │ └───────────────────────────────────────────────────────────────────────┘ │
│ ⚙ Se │                      │  canvas status line: version · branch · vc check                           │
└──────┴──────────────────────┴────────────────────────────────────────────────────────────────────────────┘
```

- **Five spaces** (D512): Client, Tests, Runs, Agents and Settings. The app opens in Client, and each space remembers its last screen.
  - **Client:** try requests. Open tabs, the api tree, call history, Save as test. Not for serious testing.
  - **Tests:** every api and load test, read and edit, check runs, and the git pane.
  - **Runs:** every run, live and stored, API and load; reports, compare, schedules and the Overview (the former Insights).
  - **Agents:** approvals, activity, usage, permissions and connection.
  - **Settings:** every setting, by scope.
- **Space switcher** (D513): a left icon rail on the canvas by default, with the traffic lights on top, the five spaces with small labels (⌘1–⌘5) and Settings pinned at the bottom (⌘,). It stays when the sidebar is collapsed. Settings › This app › Appearance offers **Sidebar bottom** instead: the five icons in one row at the sidebar's foot.
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
  - Screens size their own layouts with `@container card`, the card's width.
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
- **Test detail** (one card at full width):
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
  - Delete moves the file to the project's trash, `.vibranium/trash/` (D683), after a confirmation that lists the tests that reference it.
- **Plan preview** (`vc plan --explain`) for the selection shows the instances, the lifecycle order and the merged expects.
- **Built so far (Round 98):** the YAML view only; the form view and Split come with the form editors. The view lives in a shadow root so its styles pass the CSP (D568). The editor bar holds Diff, Revert, Fix all, Check, Run file (Stop while it runs), Save and the file menu (D573). The plan preview covers the open file, not yet a selection (D576). Run file is a check run, `record: false`, so it leaves nothing in Runs (D645).

## 6. Runs space (D519)

- **Sidebar:** Overview, All runs (filters: kind, status, profile), Schedules, and the recent runs with a live run first.
- **Run…** is the top row's primary action (⌘R).

### Run dialog and live run
- **Run dialog** (or ⌘R on a selection; D502, D632): a 560 px form. ⌘R opens it anywhere in the Runs space (D635).
  - Visible: selector, profile, concurrency, bail and a one-line plan ("17 tests · 4 files · 1 skipped", from ListDefinitions; no time estimate yet, D642).
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
- **Stored run** (D509, D636): a summary header (status, counts, duration, profile, branch @ commit, start time), an Export menu that copies `vc report N --format html|ctrf|junit`, `vc history show N --json` and the HTML report's path (the server serves no report files, so there is no Open report), and a ⋯ (Re-run failed, Compare, Copy run command); below it, the same view as a live run (D634). Each runs-list row has Compare, Run again and Copy run command in its ⋯. Routes: `/runs/<n>`, `/runs/load/<n>` and `/runs/compare/<a>/<b>` (D644).
- **Compare two runs** (D637): newly failing, fixed, slower and faster tests (20 % and 50 ms or more), and changed response shapes.
- **Per-test history strip** (D638): the open test's newest 30 results, a sparkline of its time, and its pass rate, flake rate and median.

### Overview (the former Insights)
- A profile and branch scope (D643): the project's profiles and the branches in the history.
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
- **This app:** theme, cards (glass or solid), motion and transparency, density, the tab idle time and the space switcher (Icon rail or Sidebar bottom).

## 8. Shared overlays

### Command palette (⌘K)
Built so far (D620): ⌘K or the top row's search opens it, grown out of the search field; it lists every screen grouped by space, filtered by word prefix, with arrows, Enter and Esc. The rest of this list comes later.
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
| 1 | Done (D540, D541): `CallRequest.adhoc`, an `AdhocCall` of `type` (`http`, `sse`, `websocket`), `method`, `url` (whole, or starting with `${`), `headers`, `query`, `body` (JSON text with a map or list goes as JSON, anything else as text) and `auth` (a config auth ID or `none`), sent in place of `api` and `draft`. It compiles as the in-memory api `vc-adhoc#call`, held to no convention lint (D545); a diagnostic of it names `services/vc-adhoc/vc-adhoc.api.yaml`, which the Client shows as "this request" | Scratch requests |
| 2 | Done (D543, D544): `CallStream(CallRequest)` streams `opened` (the stream ID for `StreamSend`, the run ID, the masked head exchange), then each `message` (`in` or `out`, `at_ms`, the masked fields as JSON), then `ended` (reason, `failed`, the close code). `opened.sends` says whether the stream takes a text. `StreamSend{stream, text}` sends a WebSocket text (on an `sse` stream it is `failed_precondition` and the stream stays open); `StreamSend{stream, close: true}` closes with 1000. Closing `CallStream` closes the stream | Live SSE and WS |
| 3 | Done (D542): every `Call` and `CallStream` is recorded as an `adhoc` run with its masked exchange (a stream: its head only); `ListRuns{kind: "adhoc"}` lists them and `Run.call` (`CallSummary`: method, masked URL, status) is each History row | Call history |
| 4 | Done (D679–D681): `Explain{target, profiles, sets}` (a code's page as Markdown, `markdown: true`; a test or api as `vc explain` gives it, masked), `ApplyFixes{codes, paths, write, expected_digest}` (`vc check --fix` for those codes, all when empty; a preview with `files`, `titles` and `digest`, written only with that digest), `Complete{path, text, offset}` (`items`: each `label`, `kind` key, value, reference, var or matcher, `detail`, `doc`) and `Hover{path, text, offset, profiles, sets}` (`markdown`: the key's docs, then the api or test a `call:` or `test:` names, explained and masked). `offset` counts UTF-16 code units, as a browser does; the text is the editor's, saved or not | YAML editor |
| 5 | Done (D682): `Plan{selection, profiles, sets, repeat}` is `vc plan` for the selection: `line` (the plan line), `tree` (lifecycle blocks and instances in run order, as `vc plan` prints them), `plan_json` (`vc plan --json`, D130, as text), `instances` and `notices`. Rows are labelled offline and masked; nothing is sent and no secret is loaded (D316). A bad profile, setting or selector is `invalid_argument` | Plan preview |
| 6 | Done (D683, D684): `CreateFile{kind, id, write, expected_digest}` (`vc new file`'s template for `api`, `test` or `load`; `path` and a preview with `files`, `titles` and `digest`, written only with that digest), `Rename{from, to, write, expected_digest}` (`vc rename`, every file it changes, references and a moved file's `moved_from` included, previewed then written the same way), `Format{path, text, strip_comments}` (the editor's unsaved text in `vc fmt`'s house style, `text` and `changed`; never written), `FileReferences{path}` (each `call:` or `test:` in another file naming one of its items: `path`, `line`, `from`, `target`, plus the `unread` files that do not parse) and `TrashFile{path, expected_digest}` (moves an api, test or load file to `.vibranium/trash/<stamp>/<path>` while it is the one read, `trashed_to`; not the OS trash, D683) | File operations |
| 7 | Done (D551–D553): `Cancel{run, level}`, where `run` is the `handle` the client gave `RunRequest` (its own pick, such as a UUID: at most 64 ASCII letters, digits, `-` and `_`) and `level` is `CANCEL_LEVEL_STOP` (one Ctrl-C: grace period, then cleanups; a second `stop` ends the grace period) or `CANCEL_LEVEL_ABANDON` (the third Ctrl-C: the cleanups still to run are skipped). It answers at once; the run's stream ends with its report (exit 130). A handle a running run holds is `already_exists`; one not running is `not_found` | Stop, then Skip cleanups |
| 8 | Done (D674–D678): `RunLoad{scenario, profiles, sets, seed, handle}` streams a `LoadTick` as each second closes (users, RPS, p50/p90/p95/p99, errors, the breaching thresholds, and `host`: the generator's CPU percent of one core, resident memory, the HTTP connections it holds open and the iterations that started late, each unset when the platform cannot tell), then the `LoadReport` `vc load --json` gives; closing the stream interrupts the run once and a `handle` lets `Cancel` stop it | Load tests |
| 9 | Done (D672, D673): `CompareRuns{a, b, slower_threshold}` (0 for a run is the last; the threshold is a share, 0.2 for 20%, the default when 0) gives `b`'s tests newly failing, newly blocked, fixed, still failing, slower and faster than in `a`, and each api and status whose JSON response shape changed (paths added, removed and retyped; never values; only from the bodies the runs kept). `TestHistory{test, limit}` gives the test in each of the newest 20 (at most 100) test runs, newest first: its worst status (unspecified when not run), time and instances | Compare runs, per-test strip |
| 10 | `Init`, `Demo` | Onboarding |
| 11 | `TestAlert`, `PluginList`/`Add`/`Remove` | Settings |
| 12 | User-config read and write (scoped) | Settings, the "You only" scope |
| 13 | Done (D671): `RunRequest` carries `strict`, `reports`, `out` and `single_file`, as `vc run --strict --report --out --single-file`. `out` must be a folder inside the project (relative, no `..`, no link out); anything else is `invalid_argument` with V5132. An unknown format ends the stream with an error before the run starts | Run dialog |
| 14 | Done (D546–D550): `VcsStatus` (`branch`, `upstream`, `ahead`, `behind`, `files`: each `path`, `change` (`added`, `modified`, `deleted`, `untracked`, `conflicted`), `added`, `removed`, `binary`), `VcsDiff{path}` (a masked unified diff, empty for an unchanged file), `VcsCommit{paths, message}` (the short ID; only those files are committed), `VcsPush` and `VcsPull` (fast-forward only; what git said, masked). UI only, never agents. A git failure is `failed_precondition` with git's message, no URL's credentials in it | The git pane |
| 15 | Schedules: list, write (through the config diff), the server's scheduler, `ExportSchedule` (GitHub Actions) | Schedules |
| 16 | `AgentUsage` (per agent and tool, over a window) | Agents › Usage |
| 17 | Done (D670): a check run is `Run` with `record: false` (`true` when absent). It writes no history entry, no report files and calls no reporter plugin or alert; its events still stream, and its report comes last with no `run` | Check run in Tests |

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
- The mockups predate the Strata restyle (D590); the built UI and §2 are the reference for the look.
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

- **Step 1.** (Replaced by the Strata restyle, Round 99.) The tokens, the L0 background, the glass and the controls were in `web/src/lib/styles/` (`tokens`, `glass`, `controls`, `status`, `shell`, `sidebar`). `legacy.css` mapped the v1 token names onto v2 ones. Geist and Geist Mono were bundled (D526), and #114 is fixed (D534).
- **Step 2, shell part.** The five-space shell is in `web/src/lib/shell/`: the rail, sidebars, top row and status line, saved UI prefs, and the shortcuts. The existing screens sit inside it at the routes in D527. Each sidebar lists only the views that exist today, one section per space.
- **Not built yet.** These come in later steps:
  - the Client tab strip and the per-space designs (the Tests tree in its sidebar, the git pane, the Runs sidebar and schedules)
  - the env pill popover and the search palette
  - the Appearance setting for the tint and the switcher
  - the branch in the status line (D532)

**Done so far (batch B, Round 93):**

- **Step 2, Client part.** The Client fills the shell's slots (D537): the tab strip in the top row (D539), and a sidebar with Requests (the api tree, New request, Import HAR), Flows and History (D565). The stage has the request bar over the request and response cards with a divider, and a Layout menu for the orientation (D561). The code sits in `web/src/lib/client/` behind `ClientPort` and `ClientSeams` (D538).
- **Step 3.** The response card has Body as Tree, Raw and Preview with a JSONPath filter (D560), Headers, Cookies, Timing, Checks, Saved and Output, and click-to-check from the tree (D562). Send has its menu, with Copy as cURL (D563).
- **Step 4.** SSE and WebSocket scratch tabs connect through CallStream; the message card filters, pauses and (WebSocket) sends (D564).
- **Needs the engine.** Ad hoc calls, streams and history use the RPCs of D540–D542; against an older engine each shows "needs a newer engine".
- **Not built yet:** save as var, CEL in the filter, GraphQL, Test auth, the engine events list, the full Import wizard, WebSocket snippets and the exchange check (listed in Round 93).

**Done so far (batch C, Round 98):**

- **Step 5, YAML part.** The editor at `/tests/editor` has the YAML view on CodeMirror 6 in a shadow root, so the CSP holds (D568, D569); squiggles with their certain fixes, a live check of the unsaved text, Fix all and Save anyway (D570–D572); and its bar with Check and Run file (D573, D574). Completion and hover sit behind `EditorAssist` (D567).
- **Step 6.** New file, Rename or move with its preview, Format, Move to trash and the plan preview are built behind `FileOps` and `PlanPort` (D575, D576).
- **Step 7, Cancel part.** Stop, Stop now and Skip cleanups cancel a run by its handle (D577).
- **Step 9, git part.** The Changes section of the Tests sidebar commits, pushes and pulls through the Vcs RPCs, with each file's diff (D578–D580), and the status line shows the branch (D581).
- **Needs the engine.** Completion, hover, the file operations and the plan wait for `Complete` and `Hover`, `CreateFile`, `Rename` (with its preview), `Format`, the references list, `TrashFile`, `Plan` and `ApplyFixes` (§10, rows 4–6); against today's engine each says "needs a newer engine". Run file is recorded until `Run` takes `record: false` (row 17).
- **Not built yet:** the form view and Split, Explain in a side panel, the Tests table's bulk actions and saved views, test detail, Quarantine and the per-test history strip (listed in Round 98).

**Done so far (batch D, Round 100):**

- **Runs space.** The layout fills the shell's sidebar, primary and title slots (D630): Overview, All runs and Recent with the live run first, and Run… (⌘R) at the top right (D631, D635). All runs filters by kind, status, profile, branch and a search, and compares two ticked runs.
- **Run dialog.** The 560 px form with the plan line and More options, Strict and Report files included (D632, D642, D671).
- **Step 7, drill-down part.** The live run and a stored run share `RunDetail` (D634); a stored run has its header, Export and ⋯ (D636).
- **Step 8.** The load screen's charts with threshold lines and breach markers (D641), the Hosts panel and the report (D639), at `/runs/load` and `/runs/load/<n>`.
- **Step 9, compare and check parts.** Compare at `/runs/compare/<a>/<b>` (D637), the per-test history strip (D638), and Run file as a check run (D645). The Overview has its profile and branch scope (D643).
- **Needs the engine.** Compare and the history strip wait for `CompareRuns` and `TestHistory` (§10 row 9); load runs wait for `RunLoad` and `load_tick` (row 8). Against today's engine each says "needs a newer engine".
- **Not built yet:** Schedules (batch E), the Agent filter, starting a load run from Tests (with the load editor), a duration estimate in the plan line, and Quarantine from the Overview.

**Strata restyle (Round 99):**

- The whole UI moved to Strata (D590–D607): the vendored `strata/` files plus `vibranium.css`, the space colours, chrome without boxes, one card level, capsule fields, keyboard hints and F6 regions. The old `tokens`, `legacy`, `glass`, `controls` and `status` styles and `Background.svelte` are gone, and Inter replaces Geist.
- The HTML report keeps its own look (D531); `web/src/report/shared-names.css` gives the components it shares with the app their Strata names (D606).
- **Not built yet:** the Appearance settings (cards, transparency, density), the command palette and toasts in use, and custom space colours from OKLCH seeds.
