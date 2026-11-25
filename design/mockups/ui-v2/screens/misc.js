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
      return `<section class="hist-day" aria-label="${esc(d.day)}"><h3 class="section-label">${esc(d.day)}</h3>
        <div class="table-wrap"><table class="table"><colgroup><col class="c-t"><col><col class="c-s"><col class="c-d"><col class="c-f"></colgroup><thead class="sr-only"><tr><th scope="col">Time</th><th scope="col">Request</th><th scope="col">Status</th><th scope="col">Duration</th><th scope="col">From</th></tr></thead><tbody>
        ${items.map((it) => `<tr class="hist-row"><td class="mono muted">${esc(it.t)}</td>
          <td><button class="btn btn-ghost btn-sm" data-action="reopen" data-api="${esc(it.api)}" aria-label="Reopen ${esc(it.m + ' ' + it.path)}" title="${esc(it.api)}">${method(it.m)}<span class="mono">${esc(it.path)}</span></button></td>
          <td><span class="hs" data-class="${String(it.s)[0]}">${it.s}</span></td><td class="mono muted">${it.ms ? it.ms + ' ms' : esc(it.note || '—')}</td>
          <td class="muted">${esc(SRC[it.src])}</td></tr>`).join('')}
        </tbody></table></div></section>`;
    }).join('');
    return days || `<div class="empty empty-quiet" role="status"><span>No calls match</span><button class="link" data-action="hist-clear" data-fk="hist-clear">Clear search and filter</button></div>`;
  }
  VC.screens.history = {
    crumbs: () => [D.project.name, 'History'],
    status: () => '',
    render: () => `<div class="stage-one"><section class="glass stage-one" aria-label="Call history">
      <div class="card-head"><div class="grow"><h2 class="card-title">History</h2><div class="card-sub" title="Stored as adhoc runs in history.db for 30 days; secrets are redacted">Every call from the UI, <code>vc call</code> and the TUI.</div></div></div>
      <div class="card-tabs"><div class="row row-wrap">
        <div class="input-group grow">${ic('search')}<input class="input" id="hist-q" value="${esc(H.q)}" placeholder="Search path, status or api" aria-label="Search history" data-fk="hist-q"></div>
        <div class="seg seg-sm" role="group" aria-label="Source">${[['all', 'All'], ['ui', 'UI'], ['cli', 'CLI'], ['tui', 'TUI']].map(([k, l]) => `<button aria-pressed="${H.src === k}" data-action="hist-src" data-v="${k}" data-fk="hs-${k}">${l}</button>`).join('')}</div>
      </div></div>
      <div class="card-body" data-sk="hist"><div class="hist" id="hist-body">${histBody()}</div></div>
    </section></div>`
  };
  A['hist-clear'] = () => { H.q = ''; H.src = 'all'; VC.render(); };
  A['hist-src'] = (el) => { H.src = el.dataset.v; VC.render(); };
  document.addEventListener('input', (e) => { if (e.target.id === 'hist-q') { H.q = e.target.value; const b = $('#hist-body'); if (b) b.innerHTML = histBody(); } });

  /* ---- welcome / onboarding --------------------------------------------------- */
  VC.screens.welcome = {
    crumbs: () => [S.empty ? '~/dev/new-api' : D.project.name, 'Welcome'],
    status: () => '',
    render: () => `<div class="welcome"><section class="glass welcome-card" aria-labelledby="w-title">
      <div><p class="section-label">Vibranium</p><h1 id="w-title">${S.empty ? 'This folder has no Vibranium project yet' : 'Start something new'}</h1>
        <p class="lede">Tests are plain YAML in git. Pick a way to start.</p></div>
      <ul class="start-list" aria-label="Ways to start">
        <li><button class="start-row" data-action="init-wizard" data-fk="w-init">${ic('sparkle')}<span class="grow"><span class="start-title">Init a project</span><span class="start-desc">A config, a local profile and one example service.</span></span><code class="cmd">vc init</code></button></li>
        <li><button class="start-row" data-action="open-demo" data-fk="w-demo">${ic('play')}<span class="grow"><span class="start-title">Open the demo</span><span class="start-desc">A sample shop with REST, GraphQL and WebSocket on a local mock.</span></span><code class="cmd">vc demo</code></button></li>
        <li><button class="start-row" data-action="go" data-route="import" data-fk="w-import">${ic('import')}<span class="grow"><span class="start-title">Import</span><span class="start-desc">OpenAPI, HAR, Postman, Bruno, .http or curl.</span></span><code class="cmd">vc import</code></button></li>
      </ul>
      <div class="foot"><span>Press <span class="kbd">⌘K</span> for the command palette</span></div>
    </section></div>`
  };
  const W = { step: 0, name: 'new-api', base: 'http://localhost:8080', auth: 'none', example: true };
  function initHtml() {
    const files = ['vibranium.config.yaml', 'services/example/example.api.yaml'].concat(W.example ? ['services/example/smoke.test.yaml'] : []).concat(['.gitignore (adds .vc/)', '.env.schema']);
    return `<div class="dlg dlg-form">
      <div class="dlg-head"><div class="grow"><h2 id="dlg-title">Init a project</h2><div class="sub">Same as <code>vc init</code>. Nothing is written until you confirm; then it runs <code>vc check</code>.</div></div><button class="btn btn-ghost btn-icon" data-action="close-dialog" aria-label="Close">${ic('x')}</button></div>
      <div class="dlg-body">
        <div class="field"><label for="init-name">Project name</label><input class="input" id="init-name" value="${esc(W.name)}" autofocus><span class="hint">Lowercase</span></div>
        <div class="field"><label for="init-base">Base URL for the local profile</label><input class="input mono" id="init-base" value="${esc(W.base)}"><span class="hint">Becomes <code>\${baseUrl}</code> under <b>local</b></span></div>
        <div class="field"><label for="init-auth">Auth</label><select class="select" id="init-auth"><option>none</option><option>bearer · token from \${secret.API_TOKEN}</option><option>oauth2ClientCredentials</option><option>apiKey</option></select></div>
        <label class="switch"><input type="checkbox" id="init-example" ${W.example ? 'checked' : ''}><span class="track"></span><span>Add an example smoke test</span></label>
        <details class="disc"><summary>${ic('chev-down', 'chev')}Files it writes<span class="meta">${files.length} new</span></summary><div class="disc-body">
          <ul class="stack-tight mono subtle">${files.map((f) => `<li>${ic('plus', 'ico-sm')} ${esc(f)}</li>`).join('')}</ul></div></details>
      </div>
      <div class="dlg-foot"><button class="btn" data-action="close-dialog">Cancel</button><button class="btn btn-primary" data-action="init-done">${ic('check')}Create project</button></div>
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
