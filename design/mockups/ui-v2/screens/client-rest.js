/* Client · REST request/response (#client-rest) and the scratch tab (#client-scratch).
   Also owns the shared split divider (VC.mountSplit) and the request-type menu. */
(function () {
  'use strict';
  const VC = window.VC, D = VC.data, S = VC.state, A = VC.actions;
  const { $, $$, esc, ic, badge, method, chipSrc, urlTokens, layers, jsonTree, getPath, hlJson, hlYaml } = VC.h;

  Object.assign(S, {
    reqTab: 'params', respTab: 'body', bodyView: 'tree', filter: '',
    added: [], checked: new Set(), collapsed: new Set(['body.limits']), sending: false, sentAt: '14:06'
  });

  /* ---- split divider (shared by every two-card screen) ------------------- */
  const effectiveOrient = () => {
    if (S.orient !== 'auto') return S.orient;
    const app = $('#app');
    return app && app.clientWidth < 1100 ? 'stack' : 'side';
  };
  VC.effectiveOrient = effectiveOrient;
  VC.splitAttrs = () => (S.orient === 'auto' ? '' : ` data-orient="${S.orient}"`);
  VC.mountSplit = function (view, key) {
    const split = $('.split', view);
    if (!split) return;
    const k = key || 'split';
    split.style.setProperty('--split', S[k] + '%');
    const div = $('.divider', split);
    div.addEventListener('pointerdown', (e) => {
      e.preventDefault();
      div.setPointerCapture(e.pointerId);
      div.classList.add('is-dragging');
      const r = split.getBoundingClientRect();
      const stacked = effectiveOrient() === 'stack';
      const move = (ev) => {
        const pct = stacked ? (ev.clientY - r.top) / r.height : (ev.clientX - r.left) / r.width;
        S[k] = Math.round(Math.min(80, Math.max(20, pct * 100)));
        split.style.setProperty('--split', S[k] + '%');
        div.setAttribute('aria-valuenow', S[k]);
      };
      const up = () => { div.classList.remove('is-dragging'); div.removeEventListener('pointermove', move); div.removeEventListener('pointerup', up); VC.savePrefs(); };
      div.addEventListener('pointermove', move);
      div.addEventListener('pointerup', up);
    });
    div.dataset.key = k;
  };
  VC.dividerKey = function (e) {
    const div = e.target, k = div.dataset.key || 'split';
    const step = { ArrowLeft: -2, ArrowUp: -2, ArrowRight: 2, ArrowDown: 2 }[e.key];
    let v = S[k];
    if (step) v += e.shiftKey ? step * 5 : step;
    else if (e.key === 'Home') v = 20;
    else if (e.key === 'End') v = 80;
    else return;
    e.preventDefault();
    S[k] = Math.min(80, Math.max(20, v));
    div.closest('.split').style.setProperty('--split', S[k] + '%');
    div.setAttribute('aria-valuenow', S[k]);
    VC.savePrefs();
  };
  VC.divider = (label) => `<div class="divider" role="separator" tabindex="0" aria-label="${esc(label || 'Resize request and response')}" aria-orientation="${effectiveOrient() === 'stack' ? 'horizontal' : 'vertical'}" aria-valuemin="20" aria-valuemax="80" aria-valuenow="${S.split}" data-fk="divider" title="Drag, or use the arrow keys"></div>`;
  VC.orientToggle = () => {
    const o = effectiveOrient();
    return `<div class="seg seg-sm" role="group" aria-label="Layout">
      <button aria-pressed="${o === 'side'}" data-action="orient" data-v="side" aria-label="Side by side" title="Side by side" data-fk="orient-side">${VC.h.ic('columns')}</button>
      <button aria-pressed="${o === 'stack'}" data-action="orient" data-v="stack" aria-label="Stacked" title="Stacked" data-fk="orient-stack">${VC.h.ic('rows')}</button></div>`;
  };
  A.orient = (el) => { S.orient = el.dataset.v; VC.savePrefs(); VC.render(); };

  /* ---- shared request bar ------------------------------------------------ */
  VC.tabs = function (id, label, items, cur, action) {
    return `<div class="tabs" role="tablist" aria-label="${esc(label)}">${items.map(([k, l, extra]) =>
      `<button class="tab" role="tab" id="${id}-${k}" aria-selected="${cur === k}" aria-controls="${id}-panel" tabindex="${cur === k ? 0 : -1}" data-action="${action}" data-v="${k}" data-fk="${id}-${k}">${esc(l)}${extra || ''}</button>`).join('')}</div>`;
  };
  VC.panel = (id, cur, html, cls) => `<div class="card-body${cls ? ' ' + cls : ''}" role="tabpanel" id="${id}-panel" aria-labelledby="${id}-${cur}" tabindex="0" data-sk="${id}-panel">${html}</div>`;
  VC.reqbar = function (opts) {
    return `<div class="reqbar on-l0" role="group" aria-label="Request">
      <button class="btn type-btn" data-action="type-menu" aria-haspopup="menu" aria-label="Request type: ${esc(opts.typeLabel)}" data-fk="type">${method(opts.m)}${ic('chev-down', 'ico-sm')}</button>
      <div class="url">${opts.input ? `<input value="${esc(opts.url)}" placeholder="Paste a URL or a cURL command, or type \${baseUrl}/…" aria-label="URL" data-fk="url-input">` : `<span class="url-text">${urlTokens(opts.url)}</span>`}</div>
      ${opts.extra || ''}
      ${opts.conn ? opts.conn : `<div class="btn-split">
        <button class="btn btn-primary" data-action="send" data-fk="send"${S.sending ? ' aria-busy="true"' : ''}>${ic(opts.sendIco || 'send')}${esc(opts.sendLabel || 'Send')}<span class="kbd hide-narrow">⌘↵</span></button>
        <button class="btn btn-primary" data-action="send-menu" aria-haspopup="menu" aria-label="More send options" data-fk="send-menu">${ic('chev-down')}</button>
      </div>`}
    </div>`;
  };

  /* request-type picker */
  A['type-menu'] = (el) => {
    const cur = S.route;
    const item = (route, m, label, meta) => VC.menuItem(`${method(m)} ${label}`, 'set-type', `data-route="${route}"`, { radio: true, checked: cur === route || (route === 'client-rest' && cur === 'client-scratch'), meta });
    VC.menu(el, `<div class="pop-label">Request type</div>
      ${item(S.route === 'client-scratch' ? 'client-scratch' : 'client-rest', 'GET', 'HTTP', 'type: http')}
      ${item('client-graphql', 'GQL', 'GraphQL', 'type: graphql')}
      ${item('client-sse', 'SSE', 'Server-sent events', 'type: sse')}
      ${item('client-ws', 'WS', 'WebSocket', 'type: websocket')}
      <div class="pop-sep"></div>
      ${VC.menuItem('gRPC', 'noop', '', { disabled: true, meta: 'coming' })}
      ${VC.menuItem('JSON-RPC', 'noop', '', { disabled: true, meta: 'coming' })}`);
  };
  A.noop = () => {};
  A['set-type'] = (el) => {
    const r = el.dataset.route;
    const firstOf = { 'client-graphql': 'catalog#product', 'client-sse': 'payments#events', 'client-ws': 'rooms#join' };
    if (firstOf[r]) return VC.openApi(firstOf[r]);
    VC.go(r);
  };

  /* Send / ▾ menu */
  A.send = () => { const s = VC.screens[S.route]; if (s && s.send) s.send(); };
  A['send-menu'] = (el) => VC.menu(el, `
    ${VC.menuItem('Send', 'send', '', { ico: 'send', meta: '⌘↵' })}
    ${VC.menuItem('Send and keep', 'send-keep', '', { ico: 'pin', meta: 'skips cleanup' })}
    <div class="pop-sep"></div>
    ${VC.menuItem('Copy as cURL', 'copy-curl', '', { ico: 'terminal' })}
    ${VC.menuItem('Copy as <code>vc call</code>', 'copy-vc', '', { ico: 'copy' })}`);
  A['send-keep'] = () => { A.send(); VC.toast('Sent with --keep', 'Cleanup for users#get is skipped; the user stays.'); };
  A['copy-vc'] = () => {
    const txt = `vc call ${S.api} --profile ${[S.profile].concat(S.layers).join(',')} --var userId=${D.vars.userId.value}`;
    VC.copy(txt, 'Copied vc call', txt);
  };
  VC.copy = function (text, msg, sub) {
    const done = () => VC.toast(msg, sub);
    try { navigator.clipboard.writeText(text).then(done, done); } catch (e) { done(); }
  };

  /* ---- REST: request card ------------------------------------------------ */
  const allExpect = () => ({ inherited: D.request.expect.inherited, own: D.request.expect.own.concat(S.added) });
  /* D3: no counts on tabs, except a new-check marker on Expect */
  function reqTabs() {
    return [
      ['params', 'Params'], ['headers', 'Headers'], ['auth', 'Auth'], ['body', 'Body'], ['vars', 'Vars'],
      ['expect', 'Expect', S.added.length ? `<span class="count is-good">+${S.added.length}</span>` : ''], ['options', 'Options']
    ];
  }
  function bodyPanel() {
    const a = VC.api();
    if (a.m === 'GET' || a.m === 'DELETE') {
      return `<div class="body-modes"><div class="seg seg-sm" role="group" aria-label="Body type">${['none', 'json', 'form', 'multipart', 'raw', 'file'].map((m) => `<button aria-pressed="${m === 'none'}">${m}</button>`).join('')}</div></div>
        <div class="empty">${ic('braces')}<b>No body</b><span>${esc(a.m)} requests usually send none. Pick a type to add one for this request only.</span></div>`;
    }
    return `<div class="body-modes"><div class="seg seg-sm" role="group" aria-label="Body type">${['none', 'json', 'form', 'multipart', 'raw', 'file'].map((m) => `<button aria-pressed="${m === 'json'}">${m}</button>`).join('')}</div></div>
      <pre class="code glass-dense code-fill">${hlJson('{\n  "name": "Ada Lovelace",\n  "email": "ada+${uuid()}@shop.test",\n  "role": "admin"\n}')}</pre>`;
  }
  function expectPanel() {
    const ex = allExpect();
    const yaml = 'expect:\n' + ex.inherited.concat(ex.own).map((r) => `  ${r.k}: ${r.v}`).join('\n');
    return `${layers(ex, 'expect')}
      <section class="layer" aria-label="Merged expect">
        <div class="layer-head"><h3 class="section-label">Merged · what runs</h3></div>
        <pre class="code glass-dense expect-yaml">${hlYaml(yaml)}</pre>
      </section>
      <p class="callout" data-tone="info">${ic('info')}<span>Click any value in the response body to add a check here: equals, type, exists, regex, oneOf, between or schema.</span></p>`;
  }
  function reqPanel() {
    switch (S.reqTab) {
      case 'params': return layers(D.request.params, 'params');
      case 'headers': return layers(D.request.headers, 'headers');
      case 'auth': return VC.authTab ? VC.authTab() : '';
      case 'body': return bodyPanel();
      case 'vars': return layers(D.request.vars, 'vars');
      case 'expect': return expectPanel();
      default: return layers(D.request.options, 'options');
    }
  }
  /* B1: an editable title. Hover shows a pencil; click turns it into an input */
  VC.titleEdit = (name, action, label, tip) => `<h2 class="card-title"><button class="title-edit" data-action="${action}" aria-label="${esc(label)}: ${esc(name)}" title="${esc(tip || 'Rename')}" data-fk="${action}"><span class="t">${esc(name)}</span>${ic('pencil')}</button></h2>`;
  function requestCard() {
    const a = VC.api();
    return `<section class="glass" aria-label="Request">
      <div class="card-head">
        <div class="req-name">${method(a.m)}${VC.titleEdit(a.name, 'edit-title', 'Rename request', `Rename · ${S.api} in ${VC.fileOf().split('/').pop()}:${a.line}`)}</div>
        <button class="btn btn-sm" data-action="save-test" data-fk="save-test">${ic('runs')}Save as test</button>
        ${VC.moreBtn('req-more', 'More request actions')}
      </div>
      <div class="card-tabs">${VC.tabs('rq', 'Request', reqTabs(), S.reqTab, 'req-tab')}</div>
      ${VC.panel('rq', S.reqTab, reqPanel())}
    </section>`;
  }
  A['req-tab'] = (el) => { S.reqTab = el.dataset.v; VC.render(); };
  A['edit-title'] = (el) => {
    const a = VC.api();
    VC.inlineEdit(el, a.name, (v) => { a.name = v; VC.render(); VC.toast('Request renamed', `name: ${v} · written to ${VC.fileOf().split('/').pop()}:${a.line}`); }, 'Request name');
  };
  A['req-more'] = (el) => {
    const a = VC.api();
    VC.menu(el, `${VC.menuItem('Save as api', 'save-api', '', { ico: 'file' })}
      ${VC.menuItem('Copy as cURL', 'copy-curl', '', { ico: 'terminal' })}
      ${VC.menuItem('Copy as <code>vc call</code>', 'copy-vc', '', { ico: 'copy' })}
      <div class="pop-sep"></div>
      ${VC.menuItem('Copy id', 'copy-id', '', { ico: 'copy', meta: esc(S.api) })}
      ${VC.menuItem('Open in editor', 'toast', `data-msg="Opened ${esc(VC.fileOf())}" data-sub="line ${a.line}"`, { ico: 'external', meta: `${esc(VC.fileOf().split('/').pop())}:${a.line}` })}`);
  };
  A['copy-id'] = () => VC.copy(S.api, 'Copied id', S.api);

  /* ---- REST: response card ----------------------------------------------- */
  const R = D.response;
  function respTabs() {
    const fails = R.checks.filter((c) => c.st === 'fail').length;
    return [
      ['body', 'Body'], ['headers', 'Headers'], ['cookies', 'Cookies'], ['timing', 'Timing'],
      ['checks', 'Checks', `<span class="count ${fails ? 'is-bad' : 'is-good'}">${R.checks.length - fails}/${R.checks.length}</span>`],
      ['saved', 'Saved']
    ];
  }
  /* JSONPath-ish ($.a.b, $.orders[*].id, body.x) or CEL (size(body.orders) > 0) */
  function filterHits(q) {
    q = q.trim();
    if (!q) return null;
    if (/[()=<>!]/.test(q)) return { cel: true };
    const norm = q.replace(/^\$/, 'body').replace(/^(?!body)/, 'body.').replace(/\.\.+/g, '.');
    const re = new RegExp('^' + norm.replace(/[.+?^${}()|\\]/g, '\\$&').replace(/\[\\\*\]|\[\*\]/g, '\\[\\d+\\]').replace(/\\\.\*/g, '\\.[^.\\[]+') + '$');
    const paths = [];
    (function walk(v, p) { paths.push(p); if (v && typeof v === 'object') (Array.isArray(v) ? v.map((x, i) => [i, x]) : Object.entries(v)).forEach(([k, x]) => walk(x, typeof k === 'number' ? `${p}[${k}]` : `${p}.${k}`)); })(R.body, 'body');
    const exact = new Set(paths.filter((p) => re.test(p)));
    const hits = new Set();
    paths.forEach((p) => { exact.forEach((x) => { if (p === x || x.startsWith(p) || p.startsWith(x)) hits.add(p); }); });
    return { hits, exact };
  }
  function bodyView() {
    if (S.bodyView === 'raw') return `<pre class="code" data-sk="resp-raw">${hlJson(R.body)}</pre>`;
    if (S.bodyView === 'preview') {
      const b = R.body;
      return `<div class="preview-frame"><span class="avatar" aria-hidden="true">AL</span><div class="stack-tight grow"><b>${esc(b.name)}</b><span class="muted">${esc(b.email)} · ${esc(b.role)}</span><span class="subtle">${b.orders.length} orders · ${esc(b.address.city)}</span></div>${badge('pass', 'active', true)}</div>
        <p class="filter-note">Preview renders <b>application/json</b> as a card; HTML responses render sandboxed with scripts off.</p>`;
    }
    const f = filterHits(S.filter);
    if (f && f.cel) return `<p class="filter-note">CEL <b>${esc(S.filter)}</b> → <b>true</b> · evaluated against the response</p>${jsonTree(R.body, { checkable: true, checked: S.checked, collapsed: S.collapsed, sk: 'resp-json' })}`;
    const note = f ? `<p class="filter-note"><b>${f.exact.size}</b> match${f.exact.size === 1 ? '' : 'es'} for <b>${esc(S.filter)}</b></p>` : '';
    return note + jsonTree(R.body, { checkable: true, checked: S.checked, collapsed: S.collapsed, hits: f && f.hits, hitExact: f && f.exact, sk: 'resp-json' });
  }
  VC.bodyView = bodyView;
  function bodyPanelResp() {
    return `<div class="body-tools">
        <div class="seg seg-sm" role="group" aria-label="Body view">${[['tree', 'Tree'], ['raw', 'Raw'], ['preview', 'Preview']].map(([k, l]) => `<button aria-pressed="${S.bodyView === k}" data-action="body-view" data-v="${k}" data-fk="bv-${k}">${l}</button>`).join('')}</div>
        <div class="input-group">${ic('filter')}<input class="input input-sm mono" id="resp-filter" value="${esc(S.filter)}" placeholder="Filter: $.orders[*].id or CEL" title="JSONPath ($.orders[*].id) or CEL (size(body.orders) > 0)" aria-label="Filter the body with JSONPath or CEL" data-fk="resp-filter"></div>
      </div>
      <div class="body-view glass-dense" id="body-view">${bodyView()}</div>`;
  }
  function table(head, rows) {
    return `<div class="table-wrap glass-dense"><table class="table"><thead><tr>${head.map((h) => `<th scope="col">${h}</th>`).join('')}</tr></thead><tbody>${rows.join('')}</tbody></table></div>`;
  }
  function timingPanel() {
    const total = 150, x0 = 150, w = 470, rowH = 26, top = 22;
    const sx = (ms) => x0 + (ms / total) * w;
    const grid = [0, 50, 100, 150].map((t) => `<line class="grid" x1="${sx(t)}" y1="14" x2="${sx(t)}" y2="${top + R.timing.length * rowH}"/><text x="${sx(t)}" y="10" text-anchor="middle">${t} ms</text>`).join('');
    const rows = R.timing.map((s, i) => {
      const y = top + i * rowH;
      return `<text x="0" y="${y + 16}">${esc(s.k)}</text><rect class="track" x="${x0}" y="${y + 6}" width="${w}" height="12" rx="3"/><rect class="seg-${s.cls}" x="${sx(s.start)}" y="${y + 6}" width="${Math.max(2, (s.d / total) * w)}" height="12" rx="3"/><text class="v" x="${sx(s.start + s.d) + 6}" y="${y + 16}">${s.d} ms</text>`;
    }).join('');
    const h = top + R.timing.length * rowH + 6;
    return `<section class="layer" aria-label="Waterfall"><div class="layer-head"><h3 class="section-label">Final attempt · ${R.ms} ms</h3><span class="subtle">${esc(R.proto)} · connection reused: no</span></div>
      <div class="glass-dense card-body"><svg class="wf" viewBox="0 0 680 ${h}" role="img" aria-label="Timing waterfall: ${R.timing.map((s) => `${s.k} ${s.d} ms`).join(', ')}">${grid}${rows}</svg></div></section>
      <section class="layer" aria-label="Engine events"><div class="layer-head"><h3 class="section-label">Engine events</h3><span class="subtle">what vc did around the request</span></div>
      <ol class="events glass-dense">${R.events.map((e) => `<li class="e-${e.kind}"><span class="t">${esc(e.t)}</span>${ic(e.ico)}<span><b>${esc(e.title)}</b><small>${esc(e.sub)}</small></span></li>`).join('')}</ol></section>`;
  }
  function checksPanel() {
    const rows = R.checks.map((c) => `<tr><td>${badge(c.st, null, true)}</td><td class="mono">${esc(c.what)}</td><td class="mono">${VC.h.hlVars(c.exp)}</td><td class="mono"${c.code ? ` title="${esc(c.code)}"` : ''}>${esc(c.got)}</td><td>${chipSrc(c.from)}</td></tr>`)
      .concat(S.added.map((c) => `<tr><td>${badge('skipped', 'not run', true)}</td><td class="mono">${esc(c.k)}</td><td class="mono">${esc(c.v)}</td><td class="subtle">send to evaluate</td><td>${chipSrc('request')}</td></tr>`));
    return `${table(['Result', 'Check', 'Expected', 'Got', 'From'], rows)}
      <p class="callout" data-tone="danger">${ic('fail')}<span><b>V5120 · body.email does not match.</b> Expected <code>^[^@]+@shop\\.test$</code>, got <code>"ada@shop.example"</code>. help: the demo seeds <code>@shop.example</code> addresses. fix: change the regex in this request's expect, or keep it to catch the seed.</span></p>`;
  }
  function respPanel() {
    switch (S.respTab) {
      case 'body': return bodyPanelResp();
      case 'headers': return table(['Name', 'Value'], R.headers.map(([k, v]) => `<tr><td class="mono">${esc(k)}</td><td class="mono">${v.includes('••••') ? esc(v).replace('••••', '<span class="redacted">••••</span>') : esc(v)}</td></tr>`));
      case 'cookies': return table(['Name', 'Value', 'Domain', 'Path', 'Expires', 'Flags'], R.cookies.map((c) => `<tr><td class="mono">${esc(c.name)}</td><td class="mono">${c.value === '••••' ? '<span class="redacted">••••</span>' : esc(c.value)}</td><td class="mono">${esc(c.domain)}</td><td class="mono">${esc(c.path)}</td><td>${esc(c.expires)}</td><td class="muted">${esc(c.flags)}</td></tr>`));
      case 'timing': return timingPanel();
      case 'checks': return checksPanel();
      case 'saved': return `${table(['Var', 'Value', 'Rule'], R.saved.map((s) => `<tr><td class="mono">${esc(s.k)}</td><td class="mono">${esc(s.v)}</td><td class="mono muted">${esc(s.from)}</td></tr>`))}<p class="subtle">Saved vars are visible to later calls under this profile, as \${userEmail}.</p>`;
      default: return bodyPanelResp();
    }
  }
  function rawExchange() {
    return `HTTP/2 ${R.status}\n${R.headers.map(([k, v]) => `${esc(k)}: ${esc(v).replace('••••', '<span class="redacted">••••</span>')}`).join('\n')}\n\n${hlJson(R.body)}`;
  }
  /* B2: before the first send the pane says one quiet line, with no tabs */
  VC.emptyResponse = (last) => `<section class="glass resp-empty" aria-label="Response">
      <div class="empty empty-quiet" role="status">${S.sending ? `<span>${ic('refresh')} Sending…</span>` : `<span>Send the request to see the response<span class="keys-inline" aria-label="Command Enter"><kbd class="kbd">⌘</kbd><kbd class="kbd">↵</kbd></span></span>
        ${last ? `<button class="btn btn-ghost btn-sm" data-action="show-last" data-fk="show-last">${ic('history')}Show last response (${esc(last)})</button>` : ''}`}</div>
    </section>`;
  function responseCard() {
    if (S.noResp) return VC.emptyResponse(S.sentAt);
    const fails = R.checks.filter((c) => c.st === 'fail').length;
    return `<section class="glass" aria-label="Response">
      <div class="card-head">
        <div class="resp-meta grow">
          ${S.sending ? `<span class="stat">${ic('refresh')} Sending…</span>` : `<span class="hs" data-class="2">${R.status}</span><span class="stat">${esc(R.text)}</span>
          <span class="stat" title="Final attempt; retried once, token refreshed"><b>${R.ms}</b> ms</span><span class="stat"><b>${esc(R.size)}</b></span>
          ${fails ? badge('fail', `${fails} check failed`, true) : badge('pass', 'checks pass', true)}`}
        </div>
        ${VC.moreBtn('resp-more', 'More response actions')}
      </div>
      <div class="card-tabs">${VC.tabs('rs', 'Response', respTabs(), S.respTab, 'resp-tab')}</div>
      ${VC.panel('rs', S.respTab, respPanel())}
    </section>`;
  }
  /* response ⋯: layout and copy live here instead of in the head */
  VC.respMore = (el, extra) => {
    const o = VC.effectiveOrient();
    VC.menu(el, `<div class="pop-label">Layout</div>
      ${VC.menuItem('Side by side', 'orient', 'data-v="side"', { radio: true, checked: o === 'side', ico: 'columns' })}
      ${VC.menuItem('Stacked', 'orient', 'data-v="stack"', { radio: true, checked: o === 'stack', ico: 'rows' })}
      ${extra ? '<div class="pop-sep"></div>' + extra : ''}`);
  };
  A['resp-more'] = (el) => VC.respMore(el, `${VC.menuItem('View raw exchange', 'raw-exchange', '', { ico: 'code' })}
    ${VC.menuItem('Copy body', 'copy-body', '', { ico: 'copy', meta: esc(R.size) })}
    ${VC.menuItem('Open in history', 'go', 'data-route="history"', { ico: 'history', meta: esc(S.sentAt) })}`);
  A['show-last'] = () => { S.noResp = false; VC.render(); };
  A['raw-exchange'] = () => VC.dialog(`<div class="dlg is-raw">
      <div class="dlg-head"><div class="grow"><h2 id="dlg-title">Raw exchange</h2><div class="sub">The final attempt as it went over the wire; secrets redacted</div></div>
        <button class="btn btn-ghost btn-icon" data-action="close-dialog" aria-label="Close">${ic('x')}</button></div>
      <div class="dlg-body"><pre class="code" data-sk="resp-raw-x">${rawExchange()}</pre></div>
      <div class="dlg-foot"><button class="btn btn-primary" data-action="close-dialog" autofocus>Done</button></div>
    </div>`);
  A['resp-tab'] = (el) => { S.respTab = el.dataset.v; VC.render(); };
  A['body-view'] = (el) => { S.bodyView = el.dataset.v; VC.render(); };
  A['copy-body'] = () => VC.copy(JSON.stringify(R.body, null, 2), 'Body copied', '1.84 KB of JSON');
  A['json-toggle'] = (el) => {
    const p = el.dataset.path;
    if (S.collapsed.has(p)) S.collapsed.delete(p); else S.collapsed.add(p);
    const bv = $('#body-view');
    if (bv) { const sc = bv.querySelector('.json'); const top = sc ? sc.scrollTop : 0; bv.innerHTML = bodyView(); const n = bv.querySelector('.json'); if (n) n.scrollTop = top; const t = bv.querySelector(`[data-path="${CSS.escape(p)}"].tog`); if (t) t.focus(); }
  };

  /* ---- click-to-check popover -------------------------------------------- */
  A['pick-value'] = (el) => {
    const path = el.dataset.path;
    const v = getPath(R.body, path);
    const t = v === null ? 'null' : Array.isArray(v) ? 'list' : typeof v;
    const lit = JSON.stringify(v);
    const last = path.split(/[.[\]]/).filter(Boolean).pop().replace(/"/g, '');
    const opts = [
      ['equals', 'equals', lit],
      ['type', 'type', `{ type: ${t === 'boolean' ? 'bool' : t} }`],
      ['exists', 'exists', '{ exists: true }'],
      t === 'string' ? ['regex', 'regex', `{ regex: "^${String(v).replace(/[0-9]/g, '\\d').replace(/[a-z]/g, '').slice(0, 2) || ''}.+$" }`] : null,
      ['oneOf', 'oneOf', `{ oneOf: [${lit}] }`],
      t === 'number' ? ['between', 'between', `{ between: [${Math.floor(v * 0.5)}, ${Math.ceil(v * 1.5)}] }`] : null,
      ['schema', 'schema', '{ schema: users.api#/components/User }']
    ].filter(Boolean);
    VC.pick(el, `<div class="pop-head"><span class="title">Add a check</span><span class="grow"></span><span class="chip">${esc(t)}</span></div>
      <div class="pop-path">${esc(path)} = <b>${esc(lit)}</b></div>
      <div class="pop-menu" role="menu" aria-label="Check kind">${opts.map(([k, l, e]) => `<button class="pop-item" role="menuitem" data-action="add-check" data-path="${esc(path)}" data-expr="${esc(e)}" data-kind="${k}"><span>${l}</span><span class="meta mono">${esc(e.length > 30 ? e.slice(0, 29) + '…' : e)}</span></button>`).join('')}</div>
      <div class="pop-sep"></div>
      <div class="field"><label for="save-name">Or save as a var</label><div class="row"><input class="input input-sm mono grow" id="save-name" value="${esc(last)}" aria-label="Var name"><button class="btn btn-sm" data-action="save-var" data-path="${esc(path)}">${ic('db')}Save</button></div></div>`);
    el.classList.add('is-anchor');
  };
  A['add-check'] = (el) => {
    VC.hidePopovers();
    S.added.push({ k: el.dataset.path, v: el.dataset.expr, isNew: true });
    S.checked.add(el.dataset.path);
    VC.render();
    VC.toast('Check added to Expect', `${el.dataset.path}: ${el.dataset.expr}`);
  };
  A['save-var'] = (el) => {
    const name = ($('#save-name') || {}).value || 'value';
    VC.hidePopovers();
    R.saved.push({ k: name, v: JSON.stringify(getPath(R.body, el.dataset.path)), from: `save: { ${name}: ${el.dataset.path} }` });
    VC.render();
    VC.toast(`Saved as \${${name}}`, 'Shown in the Saved tab; later calls can use it.');
  };
  function onFilter(e) {
    if (e.target.id !== 'resp-filter') return;
    S.filter = e.target.value;
    const bv = $('#body-view');
    if (bv) bv.innerHTML = bodyView();
  }
  document.addEventListener('input', onFilter);

  /* ---- Save as test / Save as api ---------------------------------------- */
  A['save-test'] = () => {
    const extra = S.added.map((c) => ['+', `        ${c.k}: ${c.v}`]);
    VC.diffDialog({
      title: 'Save as test', sub: 'Adds a test that calls users#get with this request\'s changes and checks.',
      file: 'services/users/crud.test.yaml', fileId: 'users.crud',
      fields: `<div class="field"><label for="st-id">Test id</label><input class="input mono" id="st-id" value="getsUserWithOrders"><span class="hint">Third-person verb, camelCase · full id users.crud#getsUserWithOrders</span></div>
        <div class="field"><label for="st-name">Name</label><input class="input" id="st-name" value="Gets a user with their orders"></div>`,
      lines: [
        ['@', '@@ -52,4 +52,17 @@ tests:'],
        [' ', '  - id: deletesUser'], [' ', '    call: users#delete'], [' ', '    expect: { status: 204 }'], [' ', ''],
        ['+', '  - id: getsUserWithOrders'], ['+', '    name: Gets a user with their orders'], ['+', '    call: users#get'],
        ['+', '    with:'], ['+', '      params: { fields: "id,name,email,role,createdAt" }'], ['+', '      headers: { x-debug: "true" }'], ['+', '      retries: 2'],
        ['+', '    expect:'], ['+', '      body:'], ['+', '        id: ${userId}'], ['+', '        email: { regex: "^[^@]+@shop\\\\.test$" }']
      ].concat(extra.map(([s, l]) => [s, l.replace(/^ {8}body\./, '        ')])),
      confirm: 'Write test'
    });
  };
  A['save-api'] = () => VC.diffDialog({
    title: 'Save as api', sub: 'Moves this tab\'s changes into users#get, so every test that calls it inherits them.',
    file: 'services/users/users.api.yaml', fileId: 'users',
    lines: [
      ['@', '@@ -24,9 +24,11 @@ apis:'], [' ', '  - id: get'], [' ', '    name: Get user'], [' ', '    method: GET'], [' ', '    path: /users/${userId}'],
      [' ', '    params:'], [' ', '      expand: orders'], ['+', '      fields: "id,name,email,role,createdAt"'], ['-', '    retries: 1'], ['+', '    retries: 2'],
      [' ', '    expect:'], [' ', '      status: 200'], [' ', '      headers: { content-type: { contains: json } }']
    ],
    callout: 'The x-debug header stays in this tab: it is marked local-only. 4 tests in 2 files call users#get and will inherit these changes.',
    confirm: 'Write api'
  });

  /* ---- send ---------------------------------------------------------------- */
  let sendTimer = null;
  function send() {
    if (S.sending) return;
    S.sending = true; VC.render();
    sendTimer = setTimeout(() => {
      S.sending = false;
      S.noResp = false;
      const d = new Date(); S.sentAt = d.toTimeString().slice(0, 5);
      VC.render();
      VC.toast('200 OK · 142 ms', '3 of 4 checks pass · 1 retry · token refreshed');
    }, 650);
  }

  const ensureHttp = () => { if (['GQL', 'WS', 'SSE'].includes(VC.api().m)) S.api = 'users#get'; };
  VC.screens['client-rest'] = {
    space: 'client',
    enter: ensureHttp,
    crumbs: () => [D.project.name, VC.serviceOf(), VC.api().name],
    status: () => '',
    render: () => {
      const a = VC.api();
      return `<div class="client">${VC.reqbar({ m: a.m, typeLabel: 'HTTP', url: '${baseUrl}' + a.path + (a.q || '') })}
        <div class="split"${VC.splitAttrs()}>${requestCard()}${VC.divider()}${responseCard()}</div></div>`;
    },
    mount: (v) => VC.mountSplit(v),
    unmount: () => { clearTimeout(sendTimer); S.sending = false; },
    send
  };

  /* ---- scratch tab (⌘T) ---------------------------------------------------- */
  S.scratchUrl = 'https://httpbin.org/anything?probe=1';
  const scratchTab = () => S.tabs.find((t) => t.id === S.tab && !t.api) || S.tabs.find((t) => !t.api);
  const scratchName = () => { const t = scratchTab(); return (t && (t.title || t.name)) || 'Untitled request'; };
  VC.screens['client-scratch'] = {
    space: 'client',
    crumbs: () => [D.project.name, 'Scratch', scratchName()],
    status: () => 'Not saved',
    render: () => `<div class="client">${VC.reqbar({ m: 'GET', typeLabel: 'HTTP', url: S.scratchUrl, input: true })}
      <div class="split"${VC.splitAttrs()}>
        <section class="glass" aria-label="Request">
          <div class="card-head"><div class="req-name">${ic('tab')}${VC.titleEdit(scratchName(), 'edit-scratch', 'Rename tab', 'Rename this tab')}</div>
            <button class="btn btn-sm" data-action="scratch-save-api" data-fk="sc-save">${ic('file')}Save as api</button></div>
          <div class="card-tabs">${VC.tabs('sq', 'Request', [['params', 'Params'], ['headers', 'Headers'], ['auth', 'Auth'], ['body', 'Body']], 'params', 'noop')}</div>
          ${VC.panel('sq', 'params', layers({ inherited: [{ k: 'x-client', v: 'vibranium', src: 'config', at: 'vibranium.config.yaml:11' }], own: [{ k: 'probe', v: '1' }] }, 'sparams'))}
        </section>
        ${VC.divider()}
        ${VC.emptyResponse(null)}
      </div></div>`,
    mount: (v) => {
      VC.mountSplit(v);
      const inp = $('[data-fk="url-input"]', v);
      if (inp) inp.addEventListener('change', () => { S.scratchUrl = inp.value; });
    },
    send: () => VC.toast('Sent · 200 OK · 311 ms', 'Scratch call saved to history (ui).')
  };
  A['edit-scratch'] = (el) => {
    const t = scratchTab();
    VC.inlineEdit(el, scratchName(), (v) => { if (t) { t.title = v; t.name = v; } VC.render(); VC.toast('Tab renamed', v); }, 'Tab name');
  };
  A['scratch-save-api'] = () => VC.diffDialog({
    title: 'Save as api', sub: 'Turns this scratch request into an api other tests can call.',
    file: 'services/probe/probe.api.yaml', fileId: 'probe', isNew: true,
    fields: `<div class="row-wrap row"><div class="field grow"><label for="sa-svc">Service</label><select class="select" id="sa-svc"><option>probe (new)</option>${D.services.map((s) => `<option>${esc(s.id)}</option>`).join('')}</select></div>
      <div class="field grow"><label for="sa-id">Api id</label><input class="input mono" id="sa-id" value="anything"><span class="hint">probe#anything</span></div></div>`,
    lines: [['@', '@@ -0,0 +1,9 @@'], ['+', 'apis:'], ['+', '  - id: anything'], ['+', '    name: Anything probe'], ['+', '    method: GET'], ['+', '    url: https://httpbin.org/anything'], ['+', '    params:'], ['+', '      probe: "1"'], ['+', '    expect:'], ['+', '      status: 200']],
    callout: 'The URL is absolute, so V4003 suggests a baseUrl var. fix: add httpbinUrl to vibranium.config.yaml vars.',
    confirm: 'Create file'
  });
})();
