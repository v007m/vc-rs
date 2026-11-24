/* Agents (#agents): the MCP server, its approval queue, what agents may do per profile,
   and an activity log. The agent token is never shown: snippets carry $VC_MCP_TOKEN. */
(function () {
  'use strict';
  const VC = window.VC, D = VC.data, A = VC.actions, SH = VC.sh;
  const { $, esc, ic, badge } = VC.h;
  const G = D.shared.agents;

  const M = { snip: 'claude', open: { q1: true }, logWho: 'all', logOpen: {}, edits: {}, prodUnlocked: false, serving: true };
  const PROFILES = ['local', 'dev', 'staging', 'prod'];
  const LEVEL = { allow: ['Allow', 'pass', 'check'], ask: ['Ask', 'blocked', 'info'], off: ['Off', 'skipped', 'x'] };

  /* ---- connection --------------------------------------------------------------- */
  const SNIPS = {
    claude: { label: 'Claude Code', note: 'Runs vc mcp over stdio. The agent acts under the profile you name here.', text: 'claude mcp add vibranium -- vc mcp --profile local' },
    cursor: { label: 'Cursor', note: 'Add to .cursor/mcp.json in the project. stdio, same as Claude Code.', text: '{\n  "mcpServers": {\n    "vibranium": {\n      "command": "vc",\n      "args": ["mcp", "--profile", "local"]\n    }\n  }\n}' },
    http: { label: 'HTTP', note: 'For agents that speak streamable HTTP. vc ui serves /mcp on 127.0.0.1 only; the token is in the macOS keychain and never shown here.', text: '{\n  "url": "http://127.0.0.1:7420/mcp",\n  "headers": { "authorization": "Bearer ${VC_MCP_TOKEN}" }\n}' }
  };
  function connection() {
    const s = SNIPS[M.snip];
    const clients = G.server.clients.map((c) => `<li class="ag-client"><span class="dot ${M.serving ? 'is-live' : ''}" aria-hidden="true"></span>${SH.agent(c.name)}<span class="subtle mono">${esc(c.ver)}</span><span class="subtle grow ellipsis">${esc(c.via)}</span><span class="subtle">${esc(c.last)}</span></li>`).join('');
    return `<section class="ag-panel" aria-labelledby="ag-conn-t">
      <div class="in-panel-head"><div class="grow"><h3 class="in-h" id="ag-conn-t">MCP server</h3><span class="subtle">${M.serving ? `Serving since ${G.server.since} · <span class="mono">${esc(G.server.http)}</span>` : 'Stopped. Agents cannot connect.'}</span></div>
        <label class="switch"><input type="checkbox" id="ag-serving" ${M.serving ? 'checked' : ''}><span class="track"></span><span>Serve</span></label></div>
      <ul class="ag-clients" role="list" aria-label="Connected agents">${M.serving ? clients : ''}</ul>${M.serving ? '' : SH.empty('', 'No agents connected.')}
      <div class="ag-snip">${SH.tabs('ag-sn', 'Connect from', Object.entries(SNIPS).map(([k, v]) => [k, v.label]), M.snip, 'ag-snip')}
        <div role="tabpanel" id="ag-sn-panel" aria-labelledby="ag-sn-${M.snip}" class="stack-tight">
          <div class="ag-snip-head"><span class="subtle grow">${esc(s.note)}</span><button class="btn btn-sm" data-action="ag-copy" data-fk="ag-copy">${ic('copy')}Copy</button></div>
          <pre class="code ag-code">${esc(s.text)}</pre>
          ${M.snip === 'http' ? `<div class="row row-wrap"><span class="secret-field grow"><span class="grow">VC_MCP_TOKEN = ••••</span><span class="chip-src chip" data-src="secret">keychain</span></span><button class="btn btn-sm" data-action="ag-rotate">${ic('refresh')}Rotate token</button></div>` : ''}</div></div>
    </section>`;
  }

  /* ---- approval queue ------------------------------------------------------------- */
  function qBody(q) {
    if (q.diff) return `${VC.diffHtml(q.diff, q.what)}<span class="subtle" title="digest ${esc(q.digest)}">${ic('lock', 'ico-sm')} Checked against the file on disk before writing.</span>`;
    if (q.body) return `<pre class="code glass-dense">${esc(q.what)}\ncontent-type: application/json\n\n${esc(q.body)}</pre><p class="callout" data-tone="warn">${ic('warn')}<span>This writes data on <b>${esc(q.profile)}</b>. The response goes back to the agent, redacted.</span></p>`;
    return `<p class="callout" data-tone="warn">${ic('terminal')}<span><b>The plan runs a shell command.</b> <code>${esc(q.cmd)}</code>. Runs that hold <code>command:</code> or <code>function:</code> always ask.</span></p>`;
  }
  function queue() {
    const items = G.queue.map((q) => {
      const open = !!M.open[q.id], prim = open ? 'btn-primary' : '';
      return `<li class="ag-q" data-open="${open}"><div class="ag-q-head">
        <button class="btn btn-ghost btn-icon btn-sm" data-action="ag-q-toggle" data-id="${q.id}" aria-expanded="${open}" aria-controls="agq-${q.id}" aria-label="${open ? 'Hide' : 'Show'} details" data-fk="agqt-${q.id}">${ic(open ? 'chev-down' : 'chev-right')}</button>
        ${SH.agent(q.agent)}<span class="mono ag-tool">${esc(q.tool)}</span><span class="mono grow ellipsis">${esc(q.what)}</span>
        <span class="subtle">on <span class="mono">${esc(q.profile)}</span> · ${esc(q.at)}</span></div>
        <div class="ag-q-why subtle">${ic('info', 'ico-sm')}Asks because ${esc(q.why)}</div>
        ${open ? `<div class="ag-q-body" id="agq-${q.id}">${qBody(q)}</div>` : ''}
        <div class="ag-q-actions"><button class="btn ${prim} btn-sm" data-action="ag-approve" data-id="${q.id}" data-fk="aga-${q.id}">${ic('check')}Approve once</button>
          ${q.cmd ? '' : `<button class="btn btn-sm" data-action="ag-always" data-id="${q.id}" title="Writes the ${esc(q.profile)} profile's agents policy">Always allow…</button>`}
          <span class="grow"></span><button class="btn btn-ghost btn-sm" data-action="ag-reject" data-id="${q.id}">${ic('x')}Reject</button></div></li>`;
    }).join('');
    return `<section class="ag-panel" aria-labelledby="ag-q-t"><div class="in-panel-head"><div class="grow"><h3 class="in-h" id="ag-q-t">Waiting for you <span class="count${G.queue.length ? ' is-bad' : ''}">${G.queue.length}</span></h3><span class="subtle">The agent's call blocks until you answer, or times out after 5 min.</span></div></div>
      ${G.queue.length ? `<ul class="ag-queue" role="list">${items}</ul>` : SH.empty('check', 'Nothing waiting', 'Agents only ask when a profile\'s policy says ask.')}</section>`;
  }

  /* ---- policy ---------------------------------------------------------------------- */
  const pol = (p) => G.policy[p];
  const locked = (p) => p === 'prod' && !M.prodUnlocked;
  function cell(p, key, cur, label) {
    const [l, st, i] = LEVEL[cur];
    return `<button class="ag-lv" data-lv="${cur}" data-action="ag-level" data-p="${p}" data-key="${key}" aria-haspopup="menu" aria-label="${esc(label)} on ${p}: ${l}" ${locked(p) ? 'disabled' : ''} data-fk="lv-${p}-${key}"><span class="dot st-${st === 'blocked' ? 'blocked' : st}" aria-hidden="true"></span>${l}${ic('chev-down', 'ico-sm')}</button>`;
  }
  function policy() {
    const head = PROFILES.map((p) => `<th scope="col"><span class="ag-ph">${esc(p)}${p === 'prod' ? `<label class="switch is-danger" title="Let agents act on prod"><input type="checkbox" id="ag-prod" ${M.prodUnlocked ? 'checked' : ''} aria-label="Let agents act on prod"><span class="track"></span></label>` : ''}</span></th>`).join('');
    const row = (label, sub, cells) => `<tr><th scope="row"><span class="mono">${label}</span>${sub ? `<span class="subtle">${sub}</span>` : ''}</th>${cells}</tr>`;
    const reads = row('6 read tools', G.tools.filter((t) => t[1] === 'read').map((t) => t[0]).join(', '), PROFILES.map(() => `<td><span class="ag-fixed">${ic('lock', 'ico-sm')}Always</span></td>`).join(''));
    const run = row('run_tests', 'runs that hold command: always ask', PROFILES.map((p) => `<td><label class="switch"><input type="checkbox" class="ag-run" data-p="${p}" ${pol(p).run ? 'checked' : ''} ${locked(p) ? 'disabled' : ''} aria-label="run_tests on ${p}"><span class="track"></span><span class="subtle">${pol(p).run ? 'Allow' : 'Off'}</span></label></td>`).join(''));
    const calls = G.methods.map((m) => row(`call_api · ${m}`, '', PROFILES.map((p) => `<td>${cell(p, 'm.' + m, pol(p).m[m], 'call_api ' + m)}</td>`).join(''))).join('');
    const write = row('write_definition', 'writes are always shown as a diff', PROFILES.map((p) => `<td>${cell(p, 'write', pol(p).write, 'write_definition')}</td>`).join(''));
    const n = Object.keys(M.edits).length;
    return `<section class="ag-panel ag-wide" aria-labelledby="ag-pol-t"><div class="in-panel-head"><div class="grow"><h3 class="in-h" id="ag-pol-t">What agents may do</h3><span class="subtle" title="profiles.&lt;id&gt;.agents in vibranium.config.yaml">Per profile. Without an agents block, a profile allows reads only.</span></div>
        ${n ? `<button class="btn btn-sm" data-action="ag-discard">Discard</button><button class="btn btn-primary btn-sm" data-action="ag-review" data-fk="ag-review">Review ${n} change${n > 1 ? 's' : ''}</button>` : ''}</div>
      ${locked('prod') ? `<p class="callout" data-tone="info">${ic('shield')}<span><b>prod is reads only.</b> Agents can list, search and explain, but cannot run, call or write. Turn on the switch in its column to change that.</span></p>` : `<p class="callout" data-tone="danger">${ic('warn')}<span><b>Agents may act on prod.</b> Keep every write and non-GET call on Ask.</span></p>`}
      <div class="table-wrap ag-matrix"><table class="table"><thead><tr><th scope="col">Tool</th>${head}</tr></thead><tbody>${reads}${run}${calls}${write}</tbody></table></div></section>`;
  }

  /* ---- activity --------------------------------------------------------------------- */
  const RES = { ok: ['pass', 'OK'], waiting: ['blocked', 'Waiting'], approved: ['pass', 'Approved'], refused: ['fail', 'Refused'] };
  function activity() {
    const rows = G.log.filter((l) => M.logWho === 'all' || l.agent === M.logWho).map((l, i) => {
      const [st, word] = RES[l.res], open = !!M.logOpen[i];
      return `<li class="ag-log-row"><div class="ag-log-line"><span class="mono subtle">${esc(l.t)}</span>${SH.by(l.agent)}<span class="mono ag-tool">${esc(l.tool)}</span>
        <span class="mono grow ellipsis" title="${esc(l.arg)}">${esc(l.arg)}</span><span class="subtle mono hide-narrow">${esc(l.profile)}</span>${badge(st, word + (l.by ? ' by ' + l.by : ''), true)}
        ${l.run ? `<button class="btn btn-ghost btn-sm" data-action="ag-run-open" data-n="${l.run}">#${l.run}</button>` : ''}
        ${l.diff ? `<button class="btn btn-ghost btn-sm" data-action="ag-log-toggle" data-i="${i}" aria-expanded="${open}" data-fk="agl-${i}">${ic(open ? 'chev-down' : 'chev-right')}Diff</button>` : ''}</div>
        ${l.note ? `<span class="subtle ag-log-note">${esc(l.note)}</span>` : ''}${open ? VC.diffHtml(l.diff, l.arg) : ''}</li>`;
    }).join('');
    return `<section class="ag-panel ag-wide" aria-labelledby="ag-log-t"><div class="in-panel-head"><div class="grow"><h3 class="in-h" id="ag-log-t">Activity</h3><span class="subtle" title="vc history --agent">Every tool call, redacted, kept with history</span></div>
        ${SH.seg('Agent', [['all', 'All'], ['claude-code', 'claude-code'], ['cursor', 'cursor']], M.logWho, 'ag-who')}</div>
      <ul class="ag-log" role="list">${rows}</ul></section>`;
  }

  /* ---- screen ------------------------------------------------------------------------- */
  VC.screens.agents = {
    crumbs: () => [D.project.name, 'Agents'],
    status: () => '',
    render: () => `<div class="stage-one sh-scope"><section class="glass stage-one" aria-label="Agents">
      <div class="card-head"><div class="grow"><h2 class="card-title">Agents</h2><div class="card-sub">AI agents work over MCP, and act only where a profile allows it.</div></div></div>
      <div class="card-body" data-sk="ag-body"><div class="ag-page">${connection()}${queue()}${policy()}${activity()}</div></div>
    </section></div>`
  };

  const level = (p, key) => (key === 'write' ? pol(p).write : pol(p).m[key.slice(2)]);
  A['ag-snip'] = (el) => { M.snip = el.dataset.v; VC.render(); };
  A['ag-copy'] = () => VC.copy(SNIPS[M.snip].text, `Copied the ${SNIPS[M.snip].label} snippet`, M.snip === 'http' ? 'It names $VC_MCP_TOKEN; the token itself is not copied.' : '');
  A['ag-rotate'] = () => VC.toast('Rotated the agent token', 'Connected HTTP agents must reconnect. stdio agents are unaffected.');
  A['ag-q-toggle'] = (el) => { M.open[el.dataset.id] = !M.open[el.dataset.id]; VC.render(); };
  const settle = (id, res, msg, sub) => {
    const q = G.queue.find((x) => x.id === id);
    G.queue = G.queue.filter((x) => x.id !== id);
    const l = G.log.find((x) => x.res === 'waiting' && x.tool === q.tool);
    if (l) { l.res = res; l.by = 'you'; if (q.diff && res === 'approved') l.diff = q.diff; }
    VC.toast(msg, sub, res === 'refused' ? 'warn' : undefined);
    VC.render();
  };
  A['ag-approve'] = (el) => {
    const q = G.queue.find((x) => x.id === el.dataset.id);
    settle(q.id, 'approved', `Approved ${q.tool}`, q.diff ? `Wrote ${q.what} · digest checked · vc check · 0 errors` : `${q.agent} continues`);
  };
  A['ag-reject'] = (el) => { const q = G.queue.find((x) => x.id === el.dataset.id); settle(q.id, 'refused', `Rejected ${q.tool}`, `${q.agent} is told it was refused by you`); };
  A['ag-always'] = (el) => {
    const q = G.queue.find((x) => x.id === el.dataset.id);
    const line = q.tool === 'write_definition' ? ['writeDefinitions: ask', 'writeDefinitions: true'] : ['approve: [POST, PUT, PATCH]', 'approve: [PUT, PATCH]'];
    VC.diffDialog({
      title: `Always allow ${q.tool} on ${q.profile}`, sub: 'Changes the profile\'s agents policy. The pending request is approved too.',
      file: 'vibranium.config.yaml', confirm: 'Write and approve',
      lines: [['@', '@@ -48,6 +48,6 @@'], [' ', `  ${q.profile}:`], [' ', '    agents:'], [' ', '      run: true'], ['-', `      ${line[0]}`], ['+', `      ${line[1]}`]],
      callout: 'The file is checked for changes on disk before writing.',
      after: () => { settle(q.id, 'approved', `Approved ${q.tool}`, 'and future ones like it'); }
    });
  };
  A['ag-level'] = (el) => {
    const { p, key } = el.dataset, cur = level(p, key);
    VC.menu(el, ['allow', 'ask', 'off'].map((v) => VC.menuItem(LEVEL[v][0], 'ag-set', `data-p="${p}" data-key="${key}" data-v="${v}"`, { radio: true, checked: v === cur })).join(''));
  };
  A['ag-set'] = (el) => {
    const { p, key, v } = el.dataset, was = level(p, key);
    if (key === 'write') pol(p).write = v; else pol(p).m[key.slice(2)] = v;
    M.edits[`${p}.${key}`] = M.edits[`${p}.${key}`] || [was];
    M.edits[`${p}.${key}`][1] = v;
    if (M.edits[`${p}.${key}`][0] === v) delete M.edits[`${p}.${key}`];
    VC.render();
    const b = $(`[data-fk="lv-${p}-${key}"]`); if (b) b.focus();
  };
  A['ag-discard'] = () => {
    Object.entries(M.edits).forEach(([k, [was]]) => { const [p, ...rest] = k.split('.'); const key = rest.join('.'); if (key === 'write') pol(p).write = was; else if (key === 'run') pol(p).run = was; else pol(p).m[key.slice(2)] = was; });
    M.edits = {}; VC.render();
  };
  A['ag-review'] = () => {
    const lines = [['@', '@@ profiles @@']];
    Object.entries(M.edits).forEach(([k, [was, now]]) => {
      const [p, ...rest] = k.split('.'), key = rest.join('.');
      const name = key === 'write' ? 'writeDefinitions' : key === 'run' ? 'run' : `call.${key.slice(2)}`;
      lines.push([' ', `  ${p}.agents:`], ['-', `    ${name}: ${was}`], ['+', `    ${name}: ${now}`]);
    });
    VC.diffDialog({ title: 'Save agent policy', sub: 'Agents pick up the new policy on their next tool call.', file: 'vibranium.config.yaml', lines, confirm: 'Write', callout: 'The file is checked for changes on disk before writing.', after: () => { M.edits = {}; VC.render(); } });
  };
  A['ag-who'] = (el) => { M.logWho = el.dataset.v; M.logOpen = {}; VC.render(); };
  A['ag-log-toggle'] = (el) => { M.logOpen[el.dataset.i] = !M.logOpen[el.dataset.i]; VC.render(); };
  A['ag-run-open'] = (el) => { const R = VC.runsState; R.view = 'run'; R.run = 482; VC.toast(`Run #${el.dataset.n}`, 'Sample detail: the mockup stores one full run (#482).'); VC.go('runs'); };
  document.addEventListener('change', (e) => {
    const t = e.target;
    if (t.id === 'ag-serving') { M.serving = t.checked; VC.toast(t.checked ? 'MCP server started' : 'MCP server stopped', t.checked ? 'vc ui serves /mcp on 127.0.0.1:7420' : 'Connected agents were disconnected'); VC.render(); }
    if (t.id === 'ag-prod') { M.prodUnlocked = t.checked; VC.render(); const s = $('#ag-prod'); if (s) s.focus(); }
    if (t.classList.contains('ag-run')) {
      const p = t.dataset.p, was = pol(p).run;
      pol(p).run = t.checked;
      const k = `${p}.run`;
      M.edits[k] = M.edits[k] || [was]; M.edits[k][1] = t.checked;
      if (M.edits[k][0] === t.checked) delete M.edits[k];
      VC.render();
      const s = $(`.ag-run[data-p="${p}"]`); if (s) s.focus();
    }
  });
})();
