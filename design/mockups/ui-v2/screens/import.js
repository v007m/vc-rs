/* Import wizard (#import): Source → Filter → Map → Secrets → Also create → Review. */
(function () {
  'use strict';
  const VC = window.VC, D = VC.data, S = VC.state, A = VC.actions;
  const { $, esc, ic, method, chipSrc } = VC.h;

  const STEPS = ['Source', 'Filter', 'Map', 'Secrets', 'Also create', 'Review'];
  const SOURCES = [
    ['openapi', 'OpenAPI', 'file', '3.0 / 3.1, YAML or JSON, file or URL', 'vc import openapi ./openapi.yaml'],
    ['har', 'HAR', 'globe', 'A browser recording. Requests become apis; responses seed expects.', 'vc import har ./session.har'],
    ['postman', 'Postman', 'folder', 'Collection v2.1 and its environments.', 'vc import postman ./shop.postman.json'],
    ['bruno', 'Bruno', 'folder', 'A Bruno collection folder (.bru files).', 'vc import bruno ./bruno/'],
    ['http', '.http files', 'code', 'JetBrains or VS Code REST Client files.', 'vc import http ./requests.http'],
    ['curl', 'cURL', 'terminal', 'Paste one or more curl commands.', 'vc import curl -']
  ];
  const I = S.imp = {
    step: 0, source: 'openapi', path: './openapi.yaml', q: '',
    off: new Set(['admin#reindex', 'admin#purgeCache']), also: { tests: true, sync: true, load: false, profile: false }, sel: 0
  };
  const opKey = (tag, op) => `${tag}#${op[2]}`;
  const allOps = () => D.importOps.flatMap((g) => g.ops.map((o) => ({ tag: g.tag, o, k: opKey(g.tag, o) })));
  const chosen = () => allOps().filter((x) => !I.off.has(x.k));
  const apiId = (op) => op.replace(/^(get|list|create|update|delete|purge)(\w)/, (m, v, c) => v + c).replace(/User|Order/, '').replace(/s$/, '') || op;

  /* ---- steps ----------------------------------------------------------------- */
  function stepSource() {
    const s = SOURCES.find((x) => x[0] === I.source);
    return `<div class="tiles" role="group" aria-label="Source format">${SOURCES.map(([k, l, i, d]) => `<button class="tile" aria-pressed="${I.source === k}" data-action="imp-source" data-v="${k}" data-fk="src-${k}"><span class="tile-ico">${ic(i)}</span><span class="tile-title">${l}</span><span class="tile-desc">${d}</span></button>`).join('')}</div>
      ${I.source === 'curl'
        ? `<div class="field"><label for="imp-paste">Paste curl commands</label><textarea class="textarea mono paste" id="imp-paste" spellcheck="false">curl -X POST 'https://api.shop.example/users' -H 'content-type: application/json' -H 'authorization: Bearer sk_live_…' -d '{"name":"Ada"}'</textarea><span class="hint">Tokens in headers are moved to secrets in step 4. Nothing is sent.</span></div>`
        : `<div class="field"><label for="imp-path">File or URL</label><div class="row"><input class="input mono grow" id="imp-path" value="${esc(I.path)}" data-fk="imp-path"><button class="btn" data-action="toast" data-msg="Pick a file" data-sub="The real app opens the system file picker">${ic('folder')}Browse</button></div></div>`}
      <p class="callout" data-tone="ok">${ic('pass')}<span><b>Read ${esc(I.source === 'openapi' ? 'openapi.yaml · OpenAPI 3.1' : s[1])}.</b> 10 operations in 3 tags, 2 servers, 1 security scheme. Same as <code>${esc(s[4])}</code>.</span></p>`;
  }
  function stepFilter() {
    const q = I.q.toLowerCase();
    const groups = D.importOps.map((g) => {
      const ops = g.ops.filter((o) => !q || (o.join(' ') + ' ' + g.tag).toLowerCase().includes(q));
      if (!ops.length) return '';
      const on = g.ops.filter((o) => !I.off.has(opKey(g.tag, o))).length;
      return `<div class="op-row is-tag"><input type="checkbox" id="tag-${g.tag}" data-imp-tag="${g.tag}" ${on === g.ops.length ? 'checked' : ''}${on > 0 && on < g.ops.length ? ' data-mixed="1"' : ''}><label for="tag-${g.tag}">${esc(g.tag)}</label><span class="meta">${on} of ${g.ops.length}</span></div>`
        + ops.map((o) => { const k = opKey(g.tag, o); return `<div class="op-row"><input type="checkbox" id="op-${k}" data-imp-op="${k}" ${I.off.has(k) ? '' : 'checked'}><label for="op-${k}" class="row">${method(o[0])}<span class="path">${esc(o[1])}</span></label><span class="meta mono">${esc(o[2])}</span></div>`; }).join('');
    }).join('');
    return `<div class="row"><div class="input-group grow">${ic('search')}<input class="input" id="imp-q" value="${esc(I.q)}" placeholder="Filter by path, tag or operationId" aria-label="Filter operations" data-fk="imp-q"></div><span class="subtle" id="imp-count">${chosen().length} of ${allOps().length} selected</span></div>
      <div class="filter-tree glass-dense">${groups || '<div class="empty"><b>No operations match</b></div>'}</div>
      <p class="subtle">Tip: the same filter on the CLI is <code>--tag users --tag orders</code> or <code>--path '/users/**'</code>.</p>`;
  }
  function stepMap() {
    const rows = chosen().map(({ tag, o }) => {
      const id = `${tag}#${apiId(o[2])}`;
      const exists = !!D.apis[id];
      return `<tr><td>${method(o[0])}</td><td class="mono">${esc(o[1])}</td><td class="mono">${esc(id)}</td><td class="mono muted">services/${esc(tag)}/${esc(tag)}.api.yaml</td>
        <td>${exists ? '<select class="select input-sm" aria-label="On conflict"><option>Merge, keep my expect</option><option>Replace</option><option>Skip</option></select>' : '<span class="chip">new</span>'}</td></tr>`;
    }).join('');
    return `<div class="row-wrap row"><div class="field grow"><label for="m-base">Server → var</label><select class="select" id="m-base"><option>servers[0] http://localhost:7421 → \${baseUrl}</option><option>servers[1] https://api.shop.example → \${baseUrl} (prod)</option></select></div>
      <div class="field grow"><label for="m-svc">Group by</label><select class="select" id="m-svc"><option>Tag → service</option><option>First path segment → service</option><option>One service: shop</option></select></div></div>
      <div class="table-wrap glass-dense"><table class="table"><thead><tr><th scope="col">Method</th><th scope="col">Path</th><th scope="col">Api id</th><th scope="col">File</th><th scope="col">If it exists</th></tr></thead><tbody>${rows}</tbody></table></div>
      <p class="subtle">Ids follow the conventions: camelCase from operationId, without the service name. Edit any id here; V4001 would flag one that breaks the rule.</p>`;
  }
  function stepSecrets() {
    return `<p class="callout" data-tone="warn">${ic('shield')}<span><b>2 secrets found in the source.</b> They are never written to a file. Each becomes a <code>\${secret.NAME}</code> reference, and the value goes to the store you pick.</span></p>
      <div class="table-wrap glass-dense"><table class="table"><thead><tr><th scope="col">Found at</th><th scope="col">Value</th><th scope="col">Name</th><th scope="col">Store in</th></tr></thead><tbody>
      ${D.importSecrets.map((s, i) => `<tr><td class="mono">${esc(s.where)}</td><td class="mono">${esc(s.found.slice(0, 7))}<span class="redacted">••••</span></td><td><input class="input input-sm mono" value="${esc(s.name)}" aria-label="Secret name for ${esc(s.where)}"></td>
        <td><select class="select input-sm" aria-label="Store for ${esc(s.name)}"><option${s.src === 'keychain' ? ' selected' : ''}>macOS keychain</option><option${s.src === 'env' ? ' selected' : ''}>Environment variable</option><option>Skip (leave a placeholder)</option></select></td></tr>`).join('')}
      </tbody></table></div>`;
  }
  function stepAlso() {
    const sw = (k, t, d) => `<label class="switch"><input type="checkbox" data-imp-also="${k}" ${I.also[k] ? 'checked' : ''}><span class="track"></span><span><b>${t}</b><br><span class="subtle">${d}</span></span></label>`;
    return `<div class="stack">
      ${sw('tests', 'Smoke tests', 'One test per selected operation: status and response schema. Files: <code>services/&lt;service&gt;/smoke.test.yaml</code>.')}
      ${sw('sync', 'Keep in sync', 'Adds <code>source.openapi</code>, so <code>vc sync</code> reports drift (exit 4) when the spec changes.')}
      ${sw('load', 'Load test template', 'A <code>shop.load.yaml</code> with a 30 s ramp over the GET operations.')}
      ${sw('profile', 'Profile per server', 'Adds a profile for each server in the spec (prod is tinted red).')}
    </div>`;
  }
  function files() {
    const c = chosen();
    const by = (t) => c.filter((x) => x.tag === t);
    const out = [];
    const users = by('users'), orders = by('orders'), admin = by('admin');
    if (users.length) out.push({ path: 'services/users/users.api.yaml', lines: [['@', '@@ -1,6 +1,8 @@'], [' ', 'service: users'], ['+', 'source:'], ['+', '  openapi: ../../openapi.yaml#/tags/users'], [' ', 'baseUrl: ${baseUrl}'], [' ', 'apis:'], [' ', '  - id: list'], ['@', '@@ -57,7 +59,9 @@'], [' ', '  - id: update'], [' ', '    method: PATCH'], ['-', '    body: { name: ${name} }'], ['+', '    body:'], ['+', '      schema: openapi#/components/schemas/UserPatch'], [' ', '    expect: { status: 200 }']] });
    if (orders.length) out.push({ path: 'services/orders/orders.api.yaml', lines: [['@', '@@ -1,4 +1,6 @@'], [' ', 'service: orders'], ['+', 'source:'], ['+', '  openapi: ../../openapi.yaml#/tags/orders'], [' ', 'apis:'], [' ', '  - id: list']] });
    if (admin.length) out.push({ path: 'services/admin/admin.api.yaml', isNew: true, lines: [['@', `@@ -0,0 +1,${3 + admin.length * 3} @@`], ['+', 'service: admin'], ['+', 'baseUrl: ${baseUrl}'], ['+', 'apis:']].concat(admin.flatMap(({ o }) => [['+', `  - id: ${o[2]}`], ['+', `    method: ${o[0]}`], ['+', `    path: ${o[1]}`]])) });
    if (I.also.tests) ['users', 'orders', 'admin'].filter((t) => by(t).length).forEach((t) => out.push({ path: `services/${t}/smoke.test.yaml`, isNew: true, lines: [['@', `@@ -0,0 +1,${1 + by(t).length * 3} @@`], ['+', 'tests:']].concat(by(t).flatMap(({ o }) => [['+', `  - id: ${o[0] === 'GET' ? 'reads' : o[0] === 'POST' ? 'creates' : o[0] === 'DELETE' ? 'deletes' : 'updates'}${o[2].replace(/^[a-z]+/, '')}`], ['+', `    call: ${t}#${apiId(o[2])}`], ['+', '    expect: { status: 2xx, body: { schema: openapi } }']])) }));
    if (I.also.load) out.push({ path: 'shop.load.yaml', isNew: true, lines: [['@', '@@ -0,0 +1,6 @@'], ['+', 'load:'], ['+', '  ramp: { from: 1, to: 50, over: 30s }'], ['+', '  mix:'], ['+', '    - { call: users#list, weight: 3 }'], ['+', '    - { call: orders#list, weight: 1 }'], ['+', '  thresholds: { p95: 300ms, errors: 1% }']] });
    out.push({ path: 'vibranium.config.yaml', lines: [['@', '@@ -30,3 +30,6 @@ secrets:'], [' ', 'secrets:'], [' ', '  API_TOKEN: { from: keychain }'], ['+', '  PAYMENTS_KEY: { from: env }'], ['+', '# imported 2026-11-23 from openapi.yaml'], ['+', '# vc sync compares apis with it']].concat(I.also.profile ? [['+', 'profiles:'], ['+', '  prod: { baseUrl: https://api.shop.example, tint: red }']] : []) });
    return out;
  }
  function stepReview() {
    const fs = files();
    I.sel = Math.min(I.sel, fs.length - 1);
    const f = fs[I.sel];
    const cnt = (l, s) => l.filter((x) => x[0] === s).length;
    return `<p class="subtle">${fs.length} files change. Nothing is written until you import; vc fmt and vc check run right after.</p>
      <div class="review"><div class="diff-files glass-dense" role="tablist" aria-orientation="vertical" aria-label="Files">${fs.map((x, i) => `<button class="diff-file" role="tab" aria-selected="${i === I.sel}" tabindex="${i === I.sel ? 0 : -1}" data-action="imp-file" data-i="${i}" data-fk="if-${i}"><span class="ellipsis grow mono">${esc(x.path)}</span>${x.isNew ? '<span class="chip">new</span>' : ''}<span class="add">+${cnt(x.lines, '+')}</span><span class="del">−${cnt(x.lines, '-')}</span></button>`).join('')}</div>
      <div role="tabpanel" aria-label="${esc(f.path)}">${VC.diffHtml(f.lines, f.path)}</div></div>`;
  }
  const BODY = [stepSource, stepFilter, stepMap, stepSecrets, stepAlso, stepReview];

  function render() {
    const steps = STEPS.map((s, i) => `${i ? '<li class="sep" aria-hidden="true"></li>' : ''}<li><button class="step${i < I.step ? ' is-done' : ''}"${i === I.step ? ' aria-current="step"' : ''} data-action="imp-go" data-i="${i}" data-fk="step-${i}"><span class="n">${i < I.step ? ic('check', 'ico-sm') : i + 1}</span>${s}</button></li>`).join('');
    const last = I.step === STEPS.length - 1;
    return `<div class="stage-one"><section class="glass wizard" aria-label="Import">
      <div class="card-head"><div class="grow"><h2 class="card-title">Import</h2><div class="card-sub">Bring apis in from a spec or a recording. Every file is shown as a diff before anything is written.</div></div>
        <button class="btn btn-ghost btn-sm" data-action="go" data-route="${S.lastClient}">Cancel</button></div>
      <div class="card-tabs"><ol class="stepper" aria-label="Steps">${steps}</ol></div>
      <div class="card-body" data-sk="imp-body"><h3 class="section-label">Step ${I.step + 1} of ${STEPS.length} · ${STEPS[I.step]}</h3>${BODY[I.step]()}</div>
      <div class="card-foot"><span>${chosen().length} of ${allOps().length} operations · ${esc(SOURCES.find((x) => x[0] === I.source)[1])}</span><span class="grow"></span>
        ${I.step ? `<button class="btn" data-action="imp-go" data-i="${I.step - 1}" data-fk="imp-back">${ic('arrow-left')}Back</button>` : ''}
        <button class="btn btn-primary" data-action="${last ? 'imp-done' : 'imp-go'}" data-i="${I.step + 1}" data-fk="imp-next">${last ? `${ic('import')}Import ${files().length} files` : `Next${ic('arrow-right')}`}</button></div>
    </section></div>`;
  }
  A['imp-go'] = (el) => { I.step = Math.max(0, Math.min(STEPS.length - 1, +el.dataset.i)); VC.render(); const v = $('[data-sk="imp-body"]'); if (v) v.scrollTop = 0; };
  A['imp-source'] = (el) => { I.source = el.dataset.v; VC.render(); };
  A['imp-file'] = (el) => { I.sel = +el.dataset.i; VC.render(); };
  A['imp-done'] = () => { const n = files().length; I.step = 0; VC.openApi('users#list'); VC.toast(`Imported ${n} files`, `${chosen().length} apis · vc check · 0 errors · history records the import`); };
  document.addEventListener('change', (e) => {
    const t = e.target;
    if (t.dataset.impOp) { if (t.checked) I.off.delete(t.dataset.impOp); else I.off.add(t.dataset.impOp); }
    else if (t.dataset.impTag) { const g = D.importOps.find((x) => x.tag === t.dataset.impTag); g.ops.forEach((o) => (t.checked ? I.off.delete(opKey(g.tag, o)) : I.off.add(opKey(g.tag, o)))); }
    else if (t.dataset.impAlso) { I.also[t.dataset.impAlso] = t.checked; }
    else return;
    VC.render();
    const f = $(`#${CSS.escape(t.id || '')}`) || $(`[data-imp-also="${t.dataset.impAlso}"]`); if (f) f.focus();
  });
  document.addEventListener('input', (e) => { if (e.target.id === 'imp-q') { I.q = e.target.value; VC.render(); } if (e.target.id === 'imp-path') I.path = e.target.value; });

  VC.screens.import = {
    crumbs: () => [D.project.name, 'Import', STEPS[I.step]],
    status: () => 'nothing written yet',
    render,
    mount: (v) => { v.querySelectorAll('[data-mixed]').forEach((c) => { c.indeterminate = true; }); }
  };
})();
