/* Agents (#agents): four views, one job each.
   Approvals (act) · Permissions (configure) · Connection (read) · Activity (read).
   The agent token is never shown: snippets carry $VC_MCP_TOKEN. */
(function () {
  'use strict';
  const VC = window.VC, D = VC.data, A = VC.actions, SH = VC.sh;
  const { $, esc, ic, badge } = VC.h;
  const G = D.shared.agents;

  const M = { view: 'approvals', snip: 'claude', open: {}, logWho: 'all', logRes: 'all', logOpen: {}, edits: {}, serving: true };
  const VIEWS = [['approvals', 'Approvals'], ['permissions', 'Permissions'], ['connection', 'Connection'], ['activity', 'Activity']];
  const PROFILES = ['local', 'dev', 'staging', 'prod'];
  const lead = (text, actions) => `<div class="ag-lead"><p class="grow">${text}</p>${actions || ''}</div>`;

  /* ---- approvals: what is waiting on you ------------------------------------------ */
  function qBody(q) {
    if (q.diff) return VC.diffHtml(q.diff, q.what);
    if (q.body) return `<pre class="code ag-code">${esc(q.what)}\ncontent-type: application/json\n\n${esc(q.body)}</pre>`;
    return `<pre class="code ag-code">${esc(q.cmd)}</pre>`;
  }
  function request(q) {
    const open = !!M.open[q.id];
    return `<li class="ag-q" data-open="${open}">
      <button class="ag-line ag-q-line" data-action="ag-q-toggle" data-id="${q.id}" aria-expanded="${open}" aria-controls="agq-${q.id}" data-fk="agqt-${q.id}">
        ${ic(open ? 'chev-down' : 'chev-right', 'ico-sm ag-chev')}${SH.by(q.agent)}<span class="mono ag-tool">${esc(q.tool)}</span>
        <span class="mono grow ellipsis">${esc(q.what)}</span><span class="mono subtle">${esc(q.profile)}</span><span class="subtle ag-age" title="asked at ${esc(q.at)}">${esc(q.age)} ago</span></button>
      ${open ? `<div class="ag-q-body" id="agq-${q.id}"><p class="subtle">Asks because ${esc(q.why)}.${q.diff ? ' The file is checked on disk before writing.' : ''}</p>${qBody(q)}
        <div class="ag-q-actions"><button class="btn btn-primary btn-sm" data-action="ag-approve" data-id="${q.id}" data-fk="aga-${q.id}">${ic('check')}Approve</button>
          <button class="btn btn-sm" data-action="ag-reject" data-id="${q.id}" data-fk="agr-${q.id}">${ic('x')}Reject</button>
          ${q.cmd ? '' : VC.moreBtn('ag-q-more', 'More approval options', `agm-${q.id}`).replace('data-action', `data-id="${q.id}" data-action`)}</div></div>` : ''}
    </li>`;
  }
  const approvals = () => lead('Requests from agents that wait for your answer. Each one times out after 5 minutes.')
    + (G.queue.length ? `<ul class="ag-list" role="list">${G.queue.map(request).join('')}</ul>`
      : SH.empty('check', 'Nothing is waiting for you.', 'Agents ask only where a profile is set to Ask first.'));

  /* ---- permissions: what agents may do, per profile ------------------------------- */
  const pol = (p) => G.policy[p];
  const TOOLS = [['run', 'run_tests', 'Runs tests. Plans with command: still ask']]
    .concat(G.methods.map((m) => [`m.${m}`, `call_api ${m}`, '']))
    .concat([['write', 'write_definition', 'Edits test and api files']]);
  const levelOf = (p, key) => (key === 'run' ? (pol(p).run ? 'allow' : 'off') : key === 'write' ? pol(p).write : pol(p).m[key.slice(2)]);
  function setLevel(p, key, v) {
    const was = levelOf(p, key);
    if (key === 'run') pol(p).run = v !== 'off'; else if (key === 'write') pol(p).write = v; else pol(p).m[key.slice(2)] = v;
    const k = `${p}.${key}`;
    M.edits[k] = M.edits[k] || [was];
    M.edits[k][1] = v;
    if (M.edits[k][0] === v) delete M.edits[k];
  }
  function cell(p, key, tool) {
    const v = levelOf(p, key), on = v !== 'off';
    const word = !on ? `<span class="ag-lv-off">Off</span>` : key === 'run' ? '<span class="ag-lv-txt">Allow</span>'
      : `<button class="ag-lv" data-action="ag-level" data-p="${p}" data-key="${key}" aria-haspopup="menu" aria-label="${esc(tool)} on ${p}: ${v === 'ask' ? 'Ask first' : 'Allow'}" data-fk="lv-${p}-${key}">${v === 'ask' ? 'Ask first' : 'Allow'}${ic('chev-down', 'ico-sm')}</button>`;
    return `<td><span class="ag-cell"><label class="switch"><input type="checkbox" class="ag-perm" data-p="${p}" data-key="${key}" ${on ? 'checked' : ''} aria-label="${esc(tool)} on ${p}"><span class="track"></span></label>${word}</span></td>`;
  }
  function permissions() {
    const n = Object.keys(M.edits).length;
    const acts = n ? `<button class="btn btn-ghost btn-sm" data-action="ag-discard" data-fk="ag-discard">Discard</button><button class="btn btn-primary btn-sm" data-action="ag-review" data-fk="ag-review">Review ${n} change${n > 1 ? 's' : ''}</button>` : '';
    const head = PROFILES.map((p) => `<th scope="col">${p === 'prod' ? `<span class="ag-ph" title="Off unless you turn a tool on here">${esc(p)}${ic('shield', 'ico-sm')}</span>` : esc(p)}</th>`).join('');
    const rows = TOOLS.map(([key, tool, sub]) => `<tr><th scope="row"><span class="mono">${esc(tool)}</span>${sub ? `<span class="subtle">${esc(sub)}</span>` : ''}</th>${PROFILES.map((p) => cell(p, key, tool)).join('')}</tr>`).join('');
    return lead('Which tools agents may use on each profile. Reads are always allowed; prod starts off.', acts)
      + `<div class="table-wrap ag-matrix"><table class="table"><thead><tr><th scope="col">Tool</th>${head}</tr></thead><tbody>${rows}</tbody></table></div>`;
  }

  /* ---- connection: how agents reach the project ------------------------------------- */
  const SNIPS = [
    ['claude', 'Claude Code', 'Run once in the project folder.', 'claude mcp add vibranium -- vc mcp --profile local'],
    ['cursor', 'Cursor', 'Add to .cursor/mcp.json.', '{\n  "mcpServers": {\n    "vibranium": { "command": "vc", "args": ["mcp", "--profile", "local"] }\n  }\n}'],
    ['http', 'HTTP', 'The token stays in your keychain; the snippet names it.', '{\n  "url": "http://127.0.0.1:7420/mcp",\n  "headers": { "authorization": "Bearer ${VC_MCP_TOKEN}" }\n}']
  ];
  function connection() {
    const s = G.server;
    const status = `<div class="ag-status"><span class="dot ${M.serving ? 'st-pass' : ''}" aria-hidden="true"></span>
      <span class="grow">${M.serving ? `Serving at <span class="mono">${esc(s.http)}</span> <span class="subtle">since ${esc(s.since)}</span>` : 'Stopped. Agents cannot connect.'}</span>
      ${VC.moreBtn('ag-conn-more', 'Server actions', 'ag-conn-more')}</div>`;
    const clients = M.serving ? s.clients.map((c) => `<li class="ag-line">${SH.by(c.name)}<span class="subtle mono">${esc(c.ver)}</span><span class="subtle grow ellipsis">${esc(c.via)}</span><span class="subtle">${esc(c.last)}</span></li>`).join('') : '';
    const [k, , note, text] = SNIPS.find((x) => x[0] === M.snip);
    const snip = `<div class="ag-snip-head"><h3 class="section-label grow" id="ag-snips-t">Connect a client</h3>${SH.seg('Client', SNIPS.map(([v, l]) => [v, l]), M.snip, 'ag-snip')}</div>
      <div class="ag-snip-head"><span class="subtle grow">${esc(note)}</span><button class="btn btn-sm" data-action="ag-copy" data-k="${k}" data-fk="ag-copy">${ic('copy')}Copy</button></div>
      <pre class="code ag-code">${esc(text)}</pre>`;
    return lead('How agents connect to this project over MCP.')
      + `<section class="ag-sec" aria-label="Server">${status}${M.serving ? `<ul class="ag-list" role="list" aria-label="Connected agents">${clients}</ul>` : ''}</section>
      <section class="ag-sec" aria-labelledby="ag-snips-t">${snip}</section>`;
  }

  /* ---- activity: every tool call, read only ------------------------------------------ */
  const RES = { ok: ['pass', 'OK'], waiting: ['blocked', 'Waiting'], approved: ['pass', 'Approved'], refused: ['fail', 'Refused'] };
  const shown = () => G.log.map((l, i) => [l, i]).filter(([l]) => (M.logWho === 'all' || l.agent === M.logWho) && (M.logRes === 'all' || l.res === M.logRes));
  function logRow([l, i]) {
    const [st, word] = RES[l.res], open = !!M.logOpen[i];
    const end = l.run ? `<button class="btn btn-ghost btn-sm" data-action="ag-run-open" data-n="${l.run}">Run #${l.run}</button>`
      : l.diff ? `<button class="btn btn-ghost btn-sm" data-action="ag-log-toggle" data-i="${i}" aria-expanded="${open}" data-fk="agl-${i}">${ic(open ? 'chev-down' : 'chev-right', 'ico-sm')}Diff</button>` : '';
    return `<li class="ag-log-row"><div class="ag-line"><span class="mono subtle ag-time">${esc(l.t)}</span>${SH.by(l.agent)}<span class="mono ag-tool">${esc(l.tool)}</span>
      <span class="mono grow ellipsis" title="${esc(l.note ? `${l.arg} · ${l.note}` : l.arg)}">${esc(l.arg)}</span><span class="subtle mono hide-narrow">${esc(l.profile)}</span>
      ${badge(st, word + (l.by ? ' by ' + l.by : ''), true)}<span class="ag-end">${end}</span></div>${open ? VC.diffHtml(l.diff, l.arg) : ''}</li>`;
  }
  function activity() {
    const nf = (M.logWho !== 'all') + (M.logRes !== 'all'), rows = shown();
    const filter = `<button class="btn btn-sm" data-action="ag-filter" aria-haspopup="menu" data-fk="ag-filter">${ic('filter')}Filter${nf ? `<span class="count">${nf}</span>` : ''}</button>`;
    return lead('Every tool call agents made, newest first. Arguments and responses are redacted.', filter)
      + (rows.length ? `<ul class="ag-list" role="list">${rows.map(logRow).join('')}</ul>` : SH.empty('', 'No calls match the filter.'));
  }

  /* ---- screen ------------------------------------------------------------------------- */
  const BODY = { approvals, permissions, connection, activity };
  VC.screens.agents = {
    crumbs: () => [D.project.name, 'Agents'],
    status: () => '',
    view: (v) => { M.view = BODY[v] ? v : 'approvals'; },
    /* the views sit in the header band, like Tests (round 4) */
    subnavLabel: 'Agents views',
    subnav: () => VIEWS.map(([k, l]) => `<button class="tab" data-action="ag-view" data-v="${k}"${M.view === k ? ' aria-current="page"' : ''} data-fk="ag-${k}">${esc(l)}${k === 'approvals' ? `<span class="count${G.queue.length ? '' : ' is-good'}">${G.queue.length}</span>` : ''}</button>`).join(''),
    render: () => `<div class="stage-one sh-scope"><section class="glass stage-one" aria-labelledby="ag-title">
      <div class="card-head"><div class="grow"><h2 class="card-title" id="ag-title">${esc(VIEWS.find(([k]) => k === M.view)[1])}</h2></div></div>
      <div class="card-body" id="ag-panel"><div class="ag-view" data-view="${M.view}">${BODY[M.view]()}</div></div>
    </section></div>`
  };

  A['ag-view'] = (el) => { M.view = el.dataset.v; VC.render(); };
  A['ag-snip'] = (el) => { M.snip = el.dataset.v; VC.render(); };
  A['ag-copy'] = (el) => {
    const [k, label, , text] = SNIPS.find((x) => x[0] === el.dataset.k);
    VC.copy(text, `Copied the ${label} snippet`, k === 'http' ? 'It names $VC_MCP_TOKEN; the token itself is not copied.' : '');
  };
  A['ag-conn-more'] = (el) => VC.menu(el, VC.menuItem(M.serving ? 'Stop the MCP server' : 'Start the MCP server', 'ag-serve', '', { ico: M.serving ? 'stop' : 'play' })
    + VC.menuItem('Rotate the HTTP token', 'ag-rotate', '', { ico: 'refresh' }));
  A['ag-serve'] = () => {
    M.serving = !M.serving;
    VC.toast(M.serving ? 'MCP server started' : 'MCP server stopped', M.serving ? 'vc ui serves /mcp on 127.0.0.1:7420' : 'Connected agents were disconnected');
    VC.render();
  };
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
  const qOf = (el) => G.queue.find((x) => x.id === el.dataset.id);
  A['ag-approve'] = (el) => { const q = qOf(el); settle(q.id, 'approved', `Approved ${q.tool}`, q.diff ? `Wrote ${q.what} · digest checked · vc check · 0 errors` : `${q.agent} continues`); };
  A['ag-reject'] = (el) => { const q = qOf(el); settle(q.id, 'refused', `Rejected ${q.tool}`, `${q.agent} is told you refused it`); };
  A['ag-q-more'] = (el) => VC.menu(el, VC.menuItem(`Always allow on ${esc(qOf(el).profile)}…`, 'ag-always', `data-id="${el.dataset.id}"`, { ico: 'check' }));
  A['ag-always'] = (el) => {
    const q = qOf(el);
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
    const { p, key } = el.dataset, cur = levelOf(p, key);
    VC.menu(el, [['allow', 'Allow'], ['ask', 'Ask first']].map(([v, l]) => VC.menuItem(l, 'ag-set', `data-p="${p}" data-key="${key}" data-v="${v}"`, { radio: true, checked: v === cur })).join(''));
  };
  A['ag-set'] = (el) => {
    const { p, key, v } = el.dataset;
    setLevel(p, key, v);
    VC.render();
    const b = $(`[data-fk="lv-${p}-${key}"]`); if (b) b.focus();
  };
  A['ag-discard'] = () => {
    Object.entries(M.edits).forEach(([k, [was]]) => { const [p, ...rest] = k.split('.'); setLevel(p, rest.join('.'), was); });
    M.edits = {}; VC.render();
  };
  A['ag-review'] = () => {
    const lines = [['@', '@@ profiles @@']];
    Object.entries(M.edits).forEach(([k, [was, now]]) => {
      const [p, ...rest] = k.split('.'), key = rest.join('.');
      const name = key === 'write' ? 'writeDefinitions' : key === 'run' ? 'run' : `call.${key.slice(2)}`;
      const yv = (v) => (key === 'run' ? v !== 'off' : v);
      lines.push([' ', `  ${p}.agents:`], ['-', `    ${name}: ${yv(was)}`], ['+', `    ${name}: ${yv(now)}`]);
    });
    VC.diffDialog({ title: 'Save agent permissions', sub: 'Agents pick up the change on their next tool call.', file: 'vibranium.config.yaml', lines, confirm: 'Write', callout: 'The file is checked for changes on disk before writing.', after: () => { M.edits = {}; VC.render(); } });
  };
  A['ag-filter'] = (el) => {
    const group = (label, key, opts) => `<div class="pop-label">${label}</div>${opts.map(([v, l]) => VC.menuItem(l, 'ag-filter-set', `data-k="${key}" data-v="${v}"`, { radio: true, checked: M[key] === v })).join('')}`;
    VC.menu(el, group('Agent', 'logWho', [['all', 'All agents'], ['claude-code', 'claude-code'], ['cursor', 'cursor']])
      + '<div class="pop-sep"></div>' + group('Result', 'logRes', [['all', 'Any result'], ['waiting', 'Waiting'], ['approved', 'Approved'], ['refused', 'Refused'], ['ok', 'OK']]));
  };
  A['ag-filter-set'] = (el) => { M[el.dataset.k] = el.dataset.v; M.logOpen = {}; VC.hidePopovers(); VC.render(); const b = $('[data-fk="ag-filter"]'); if (b) b.focus(); };
  A['ag-log-toggle'] = (el) => { M.logOpen[el.dataset.i] = !M.logOpen[el.dataset.i]; VC.render(); };
  A['ag-run-open'] = (el) => { const R = VC.runsState; R.view = 'run'; R.run = 482; VC.toast(`Run #${el.dataset.n}`, 'Sample detail: the mockup stores one full run (#482).'); VC.go('runs'); };
  document.addEventListener('change', (e) => {
    const t = e.target;
    if (!t.classList || !t.classList.contains('ag-perm')) return;
    const { p, key } = t.dataset;
    setLevel(p, key, !t.checked ? 'off' : key === 'run' || key === 'm.GET' ? 'allow' : 'ask');
    VC.render();
    const s = $(`.ag-perm[data-p="${p}"][data-key="${key}"]`); if (s) s.focus();
  });
})();
