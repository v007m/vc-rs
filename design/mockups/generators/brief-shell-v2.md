# Web shell v2 — changes on top of brief-common.md (read that first)

Feedback from Sarath on the first redesign: "There's a lot of text getting clipped or not responsive inside the container. The side navigation should be collapsible so the space can be used if needed. The UI isn't using the available space: if the width is slightly smaller, elements should rearrange, resize or wrap. Some dropdowns took ~1/5 of the width on the left of a row with the rest of the row empty. Why is there a user profile / signed-in avatar button? There's no sign-in in this app. A global Run button adds no value except inside a test screen, the run screen or the API client. The project dropdown makes no sense: this web UI is for one project. It's not clear where I update the main config file, create a profile, create an auth mechanism and test it. The Agents page is confusing."

## 1. Stylesheet
Replace the stylesheet link in every web artboard with the v2 file (a superset of v1, same classes):
`<link rel="stylesheet" href="/_blob/81dcd3b549d078b575adb6aed82e1861">` (v1 was `/_blob/1e69a6d642232c67587a282225e98669`; no artboard may keep v1). Source: `/tmp/claude-0/-home-claude/80b4900a-8223-5c77-874b-c1dda2eeca25/scratchpad/vibmock/assets/vib-web2.css` — read the "v2 additions" block. New: `.app.side-collapsed` (icon rail), `.side-head`, `.navitem .label`, `.side .ctx` (context section, hidden when collapsed), `.project`, `.chip`, `.formgrid` (+`.span-all`), `.cluster`, `.panes`, `.ellipsis`, `.wrap`, `.panel` is a size container (`@container` queries allowed in page CSS).

## 2. The shell, exactly
```
div.app (+ class "side-collapsed" when state.sideCollapsed)
  header.topbar
    a.brand (logo + "Vibranium")
    span.project: <b>shop</b> + span.chip "main" (git branch) — plain text, NOT a dropdown or switcher
    profile select: label "Profile", options dev / staging / prod (this is the project's config profile, not a user)
    .grow
    .search ("Search tests, apis, files…" + kbd ⌘K)
    (NO global Run button, NO avatar/user button, NO sign-in, NO project switcher)
  div.frame
    nav.side
      div.side-head: (nothing or a small label) + button.iconbtn aria-label "Collapse sidebar"/"Expand sidebar" toggling state.sideCollapsed (chevron icon flips)
      global nav: <a class="navitem"> icon + <span class="label">…</span> (+ optional .count): Tests (WebExplorer.dc.html), API client (WebClient.dc.html), Editor (WebEditor.dc.html), Runs (WebRun.dc.html), Insights (WebInsights.dc.html), AI agents (WebAgents.dc.html); then, pushed to the bottom of the nav or after a divider, Settings (WebSettings.dc.html). Each navitem also has title="<label>" so the collapsed rail shows a tooltip.
      div.ctx: the screen's context section (file tree, api list, filters…), hidden when collapsed
    main.panel
```
Run buttons appear only where they act on something specific: Tests (run selected / run this test), the Editor (run this test / file), the API client (Send, Run flow), Runs (re-run failed, stop). Accounts and sign-in exist only on the optional self-hosted team server, which is out of scope for these screens; the local web UI (`vc ui`) has no user.

## 3. Responsiveness rules (audit every element)
- No fixed px widths on inputs/selects/buttons inside forms: put form fields in `.formgrid` so fields share the row and wrap; a lone select in a row must stretch (`width:100%`) or sit in a `.cluster` beside related controls — never a narrow control stranded on the left of an empty row.
- Two-pane layouts use `.panes` or `flex-wrap` with sensible minimums so they stack before they squeeze; side-by-side only when each pane gets ≥ ~420px.
- Text never clips: labels/ids that may be long get `.ellipsis` + a `title` with the full text where truncation is acceptable (table cells of ids, tab titles), or `.wrap` where the full text matters (messages, paths in diagnostics, descriptions). Buttons never cut their text: let toolbars wrap (`.cluster`). Remove `white-space:nowrap`/`overflow:hidden` combos that hide content without ellipsis.
- Toolbars (`panel-head`, filter rows) wrap with `.cluster`; secondary actions can collapse into a "More" menu below ~700px.
- Check widths 1440, 1180, 960, 760 and 390 (phone) — with sidebar expanded and collapsed. If a render harness is available (see `/tmp/claude-0/-home-claude/80b4900a-8223-5c77-874b-c1dda2eeca25/scratchpad/client-src/render.js`, `shoot.py`, `overflow.py` from the API-client build — they statically render a template state with Chromium and detect horizontal overflow and clipped text), use it to screenshot these widths and fix overflow; look at the PNGs yourself.
- Keep the template-hole check passing (`/tmp/claude-0/-home-claude/80b4900a-8223-5c77-874b-c1dda2eeca25/scratchpad/check-dc.js` or `checkholes.js`).

## 4. Facts for the new Settings and AI agents content
- Settings is the form view over `vibranium.config.yaml` (and `.env.schema`): every change shows a pending-changes bar "Changes to vibranium.config.yaml · View diff · Discard · Save" and an "Open in editor" link (WebEditor.dc.html). Nothing is stored anywhere else.
- AI agents: coding agents (Claude Code, Cursor, Windsurf, any MCP client) connect to `vc mcp` (stdio, started by the agent) or to the running web UI's MCP endpoint (`http://localhost:7420/mcp` when `vc ui` is running). Tools they get: list/search definitions, read history, check, run tests, explain failures, call apis, write definitions. Agent access is set per profile in `vibranium.config.yaml` → `profiles[].agents`: `run` (true/false), `call` (methods allowed without asking), `approve` (methods that need a person), `writeDefinitions` (true/false/ask). A call needing approval pauses the agent; the person approves in the web UI's AI agents page (or in the terminal where `vc mcp` prints an approval prompt if no UI is open). Everything agents do is logged in history with tool name and arguments; secrets are redacted.
