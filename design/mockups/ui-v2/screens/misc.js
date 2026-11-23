/* History (#history), Welcome / onboarding (#welcome) and the pass-2 placeholders
   (#tests #runs #insights #agents #project-settings). */
(function () {
  'use strict';
  const VC = window.VC, D = VC.data, S = VC.state, A = VC.actions;
  const { $, esc, ic, method } = VC.h;

  /* ---- history ----------------------------------------------------------------- */
  const H = { q: '', src: 'all' };
  const SRC = { ui: 'UI', cli: 'vc call', tui: 'TUI' };
  function histBody() {
    const q = H.q.trim().toLowerCase();
    const days = D.history.map((d) => {
      const items = d.items.filter((it) => (H.src === 'all' || it.src === H.src) && (!q || `${it.m} ${it.path} ${it.s} ${it.api}`.toLowerCase().includes(q)));
      if (!items.length) return '';
      return `<section class="hist-day" aria-label="${esc(d.day)}"><h3 class="section-label">${esc(d.day)} <span class="subtle">· ${items.length}</span></h3>
        <div class="table-wrap glass-dense"><table class="table"><thead class="sr-only"><tr><th scope="col">Time</th><th scope="col">Request</th><th scope="col">Status</th><th scope="col">Duration</th><th scope="col">From</th><th scope="col">Api</th></tr></thead><tbody>
        ${items.map((it) => `<tr class="hist-row"><td class="mono muted">${esc(it.t)}</td>
          <td><button class="btn btn-ghost btn-sm" data-action="reopen" data-api="${esc(it.api)}" aria-label="Reopen ${esc(it.m + ' ' + it.path)}">${method(it.m)}<span class="mono">${esc(it.path)}</span></button></td>
          <td><span class="hs" data-class="${String(it.s)[0]}">${it.s}</span></td><td class="mono muted">${it.ms ? it.ms + ' ms' : esc(it.note || '—')}</td>
          <td><span class="chip">${esc(SRC[it.src])}</span></td><td class="mono muted hide-narrow">${esc(it.api)}</td></tr>`).join('')}
        </tbody></table></div></section>`;
    }).join('');
    return days || `<div class="empty">${ic('history')}<b>No calls match</b><span>Try a path, a status such as 4xx, or clear the source filter.</span></div>`;
  }
  VC.screens.history = {
    crumbs: () => [D.project.name, 'History'],
    status: () => `${ic('db', 'ico-sm')} history.db · 10 adhoc runs · kept 30 days`,
    render: () => `<div class="stage-one"><section class="glass stage-one" aria-label="Call history">
      <div class="card-head"><div class="grow"><h2 class="card-title">History</h2><div class="card-sub">Every call from the UI, <code>vc call</code> and the TUI, stored as adhoc runs. Bodies are kept per history.bodies; secrets are redacted.</div></div></div>
      <div class="card-tabs"><div class="row row-wrap">
        <div class="input-group grow">${ic('search')}<input class="input" id="hist-q" value="${esc(H.q)}" placeholder="Search path, status or api" aria-label="Search history" data-fk="hist-q"></div>
        <div class="seg seg-sm" role="group" aria-label="Source">${[['all', 'All'], ['ui', 'UI'], ['cli', 'CLI'], ['tui', 'TUI']].map(([k, l]) => `<button aria-pressed="${H.src === k}" data-action="hist-src" data-v="${k}" data-fk="hs-${k}">${l}</button>`).join('')}</div>
      </div></div>
      <div class="card-body" data-sk="hist"><div class="hist" id="hist-body">${histBody()}</div></div>
    </section></div>`
  };
  A['hist-src'] = (el) => { H.src = el.dataset.v; VC.render(); };
  document.addEventListener('input', (e) => { if (e.target.id === 'hist-q') { H.q = e.target.value; const b = $('#hist-body'); if (b) b.innerHTML = histBody(); } });

  /* ---- welcome / onboarding --------------------------------------------------- */
  VC.screens.welcome = {
    crumbs: () => [S.empty ? '~/dev/new-api' : D.project.name, 'Welcome'],
    status: () => (S.empty ? 'no vibranium.config.yaml in this folder' : 'shop · 14 apis · 12 test files'),
    render: () => `<div class="welcome"><section class="glass welcome-card" aria-labelledby="w-title">
      <div><p class="section-label">Vibranium</p><h1 id="w-title">${S.empty ? 'This folder has no Vibranium project yet' : 'Start something new'}</h1>
        <p class="lede">Tests are plain YAML in git. Start a project here, open the demo shop with its own mock server, or bring apis in from a spec you already have.</p></div>
      <div class="tiles">
        <button class="tile" data-action="init-wizard" data-fk="w-init"><span class="tile-ico">${ic('sparkle')}</span><span class="tile-title">Init a project</span><span class="tile-desc">Writes vibranium.config.yaml, a local profile and one example service. Asks three questions first.</span><span class="cmd">vc init</span></button>
        <button class="tile" data-action="open-demo" data-fk="w-demo"><span class="tile-ico">${ic('play')}</span><span class="tile-title">Open the demo</span><span class="tile-desc">The shop project: users, orders, payments, a GraphQL catalog and a WebSocket room, on a mock at :7421.</span><span class="cmd">vc demo</span></button>
        <button class="tile" data-action="go" data-route="import" data-fk="w-import"><span class="tile-ico">${ic('import')}</span><span class="tile-title">Import</span><span class="tile-desc">OpenAPI, HAR, Postman, Bruno, .http files or curl. Every file is shown as a diff first.</span><span class="cmd">vc import openapi ./openapi.yaml</span></button>
      </div>
      <div class="foot"><span>${ic('keyboard', 'ico-sm')} Press <span class="kbd">⌘K</span> for the command palette and <span class="kbd">?</span> for shortcuts.</span><span>${ic('terminal', 'ico-sm')} Everything here is also a <code>vc</code> command.</span></div>
    </section></div>`
  };
  const W = { step: 0, name: 'new-api', base: 'http://localhost:8080', auth: 'none', example: true };
  function initHtml() {
    const files = ['vibranium.config.yaml', 'services/example/example.api.yaml'].concat(W.example ? ['services/example/smoke.test.yaml'] : []).concat(['.gitignore (adds .vc/)', '.env.schema']);
    return `<div class="dlg">
      <div class="dlg-head"><div class="grow"><h2 id="dlg-title">Init a project</h2><div class="sub">Same as <code>vc init</code>. Nothing is written until you confirm.</div></div><button class="btn btn-ghost btn-icon" data-action="close-dialog" aria-label="Close">${ic('x')}</button></div>
      <div class="dlg-body">
        <div class="field"><label for="init-name">Project name</label><input class="input" id="init-name" value="${esc(W.name)}" autofocus><span class="hint">Lowercase; used for the tint and the history file.</span></div>
        <div class="field"><label for="init-base">Base URL for the local profile</label><input class="input mono" id="init-base" value="${esc(W.base)}"><span class="hint">Becomes <code>\${baseUrl}</code> under profile <b>local</b>.</span></div>
        <div class="field"><label for="init-auth">Auth</label><select class="select" id="init-auth"><option>none</option><option>bearer · token from \${secret.API_TOKEN}</option><option>oauth2ClientCredentials</option><option>apiKey</option></select></div>
        <label class="switch"><input type="checkbox" id="init-example" ${W.example ? 'checked' : ''}><span class="track"></span><span>Add an example smoke test</span></label>
        <section class="layer"><div class="layer-head"><h3 class="section-label">Files</h3><span class="subtle">${files.length} new</span></div>
          <ul class="stack-tight mono subtle">${files.map((f) => `<li>${ic('plus', 'ico-sm')} ${esc(f)}</li>`).join('')}</ul></section>
      </div>
      <div class="dlg-foot"><span class="note">Then runs vc check.</span><button class="btn" data-action="close-dialog">Cancel</button><button class="btn btn-primary" data-action="init-done">${ic('check')}Create project</button></div>
    </div>`;
  }
  A['init-wizard'] = () => VC.dialog(initHtml());
  A['init-done'] = () => { VC.closeDialog(); S.empty = false; VC.go('client-rest'); VC.toast('Project created', `${W.example ? 4 : 3} files · vc check · 0 errors`); };
  A['open-demo'] = () => { S.empty = false; VC.openApi('users#get', { toast: 'Demo opened · mock running on :7421' }); };
  document.addEventListener('change', (e) => { if (e.target.id === 'init-example') { W.example = e.target.checked; VC.dialog(initHtml()); $('#init-example').focus(); } });
  document.addEventListener('input', (e) => { if (e.target.id === 'init-name') W.name = e.target.value; if (e.target.id === 'init-base') W.base = e.target.value; });

  /* ---- pass-2 placeholders ------------------------------------------------------ */
  const P2 = {
    tests: ['Tests', 'runs', 'ui.md §5 · Explorer, Editor, Runner', ['Explorer: tree, facets and the query bar (tag:smoke status:failed api:orders)', 'Test detail card with the lifecycle it inherits and a history strip', 'Editor: Form, YAML and Split over one buffer', 'Run dialog, live run with drill-down, and Stop with live cleanups']],
    runs: ['Runs', 'play', 'ui.md §5 · Runs and reports', ['Runs list filtered by status, profile, branch, kind and agent', 'A stored run uses the live-run view', 'Compare two runs: newly failing, fixed, slower, faster']],
    insights: ['Insights', 'chart', 'ui.md §5 · Insights', ['Pass-rate and duration trends per profile and branch', 'Flaky list with Quarantine (writes allowFailure + until)', 'Slowest apis, coverage gaps and expiring skips']],
    agents: ['Agents', 'bot', 'ui.md §6 · Agents', ['MCP server status and connect snippets', 'Approval queue and per-tool, per-profile permissions', 'Activity log with diffs; agent runs carry a badge']],
    'project-settings': ['Settings', 'gear', 'ui.md §6 · Settings', ['Project (vibranium.config.yaml) and You-only (~/.config/vibranium) scopes', 'A form per section with a pending-changes diff bar', 'UI preferences: theme, tint, motion, transparency, density, tab idle time']]
  };
  Object.entries(P2).forEach(([route, [title, ico, ref, items]]) => {
    VC.screens[route] = {
      crumbs: () => [D.project.name, title],
      status: () => 'pass 2',
      render: () => `<div class="p2"><section class="glass p2-card" aria-labelledby="p2-title">
        <div class="row">${ic(ico)}<span class="chip">Pass 2</span></div>
        <h1 id="p2-title">${esc(title)}</h1>
        <p class="subtle">This screen is designed in the next pass. The shell, sidebar and navigation already work, so it plugs in as one more file under screens/.</p>
        <ul>${items.map((x) => `<li>${esc(x)}</li>`).join('')}</ul>
        <p class="ref">${esc(ref)}</p>
        <div class="row"><button class="btn" data-action="go" data-route="${esc(S.lastClient)}">${ic('arrow-left')}Back to Client</button></div>
      </section></div>`
    };
  });
})();
