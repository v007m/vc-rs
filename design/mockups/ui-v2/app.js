/* Vibranium UI v2 mockups: core (state, helpers, router, shell, keyboard).
   ---------------------------------------------------------------------------
   Conventions for screens (screens/*.js):
   - Register:  VC.screens['route-name'] = { enter(), crumbs(), status(), render(), mount(view), unmount() }
     enter() runs before the shell is drawn (fix up state such as S.api).
     Only render() is required. Route names are plain hash words (#client-rest).
   - Actions:   put data-action="name" on a control and define VC.actions.name = (el, ev) => {}.
     One delegated click listener runs them. Never add inline handlers or style=.
   - Re-render: VC.render() redraws the shell and the view. Give focusable controls a
     data-fk="unique-key" so focus survives, and scrollers a data-sk="key".
   - Helpers:   VC.h.{esc, ic, badge, method, chipSrc, layers, jsonTree, hlJson, hlYaml, urlTokens}
   - Overlays:  VC.toast(), VC.dialog(html), VC.menu(anchor, html), VC.pick(anchor, html)
   - The only style set from JS is the --split custom property on .split.
   --------------------------------------------------------------------------- */
(function () {
  'use strict';
  const VC = window.VC;
  const D = VC.data;
  VC.screens = {};
  VC.actions = {};

  /* ---- helpers ----------------------------------------------------------- */
  const $ = (s, r) => (r || document).querySelector(s);
  const $$ = (s, r) => Array.from((r || document).querySelectorAll(s));
  const ESC = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };
  const esc = (s) => String(s == null ? '' : s).replace(/[&<>"']/g, (c) => ESC[c]);
  const ic = (n, cls) => `<svg class="ico${cls ? ' ' + cls : ''}" aria-hidden="true"><use href="#i-${n}"/></svg>`;
  const STATUS = {
    pass: ['pass', 'Passed'], fail: ['fail', 'Failed'], blocked: ['blocked', 'Blocked'],
    skipped: ['skipped', 'Skipped'], flaky: ['flaky', 'Flaky'], allow: ['allow', 'Allowed failure'], running: ['', 'Running']
  };
  const badge = (st, text, sm) =>
    `<span class="badge${sm ? ' badge-sm' : ''} st-${st}">${st === 'running' ? '<span class="dot is-live" aria-hidden="true"></span>' : ic(STATUS[st][0])}${esc(text == null ? STATUS[st][1] : text)}</span>`;
  const method = (m) => `<span class="method" data-m="${esc(m)}">${m === 'DELETE' ? 'DEL' : esc(m)}</span>`;
  const SRC_LABEL = { api: 'api', file: 'file', config: 'config', profile: 'profile', request: 'request', secret: 'secret', env: 'env', set: 'saved' };
  const chipSrc = (src, title) =>
    `<span class="chip chip-src" data-src="${esc(src)}"${title ? ` title="${esc(title)}"` : ''}>${esc(SRC_LABEL[src] || src)}</span>`;

  /* tokenise text with a regex, escaping the gaps */
  function tokenize(str, re, fn) {
    let out = '', last = 0, m;
    re.lastIndex = 0;
    while ((m = re.exec(str))) {
      out += esc(str.slice(last, m.index)) + fn(m);
      last = m.index + m[0].length;
      if (m[0].length === 0) re.lastIndex++;
    }
    return out + esc(str.slice(last));
  }
  const varSpan = (name) => `<span class="var-token${name.startsWith('secret.') ? ' is-secret' : ''}" data-var="${esc(name)}">\${${esc(name)}}</span>`;
  function hlJson(v) {
    const s = typeof v === 'string' ? v : JSON.stringify(v, null, 2);
    return tokenize(s, /("(?:\\.|[^"\\])*")(\s*:)?|\b(true|false)\b|\bnull\b|-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?/g, (m) => {
      if (m[1]) return m[2] ? `<span class="k">${esc(m[1])}</span>${esc(m[2])}` : `<span class="s">${hlVars(m[1])}</span>`;
      if (m[3]) return `<span class="b">${m[0]}</span>`;
      if (m[0] === 'null') return '<span class="z">null</span>';
      return `<span class="n">${m[0]}</span>`;
    });
  }
  function hlVars(s) { return tokenize(s, /\$\{([^}]+)\}/g, (m) => varSpan(m[1])); }
  function hlYamlValue(v) {
    return tokenize(v, /(#.*$)|\$\{([^}]+)\}|("[^"]*"|'[^']*')|\b(true|false)\b|\b(null)\b|(-?\b\d+(?:\.\d+)?\b)/g, (m) => {
      if (m[1]) return `<span class="c">${esc(m[1])}</span>`;
      if (m[2]) return varSpan(m[2]);
      if (m[3]) return `<span class="s">${hlVars(m[3])}</span>`;
      if (m[4]) return `<span class="b">${m[4]}</span>`;
      if (m[5]) return '<span class="z">null</span>';
      return `<span class="n">${m[6]}</span>`;
    });
  }
  function hlYamlLine(l) {
    if (/^\s*#/.test(l)) return `<span class="c">${esc(l)}</span>`;
    const m = l.match(/^(\s*(?:-\s+)?)([A-Za-z_$][\w.$\-[\]*'"]*)(:)(\s.*|$)/);
    if (m) return esc(m[1]) + `<span class="k">${esc(m[2])}</span><span class="p">:</span>` + hlYamlValue(m[4]);
    const d = l.match(/^(\s*-\s+)(.*)$/);
    return d ? esc(d[1]) + hlYamlValue(d[2]) : hlYamlValue(l);
  }
  const hlYaml = (src) => src.split('\n').map(hlYamlLine).join('\n');

  /* a URL with ${var} tokens as focusable buttons (hover shows value + origin) */
  function urlTokens(url) {
    return tokenize(url, /\$\{([^}]+)\}|(\?.*$)/g, (m) => m[2]
      ? `<span class="q">${hlVarsBtn(m[2])}</span>`
      : `<button type="button" class="var-token${m[1].startsWith('secret.') ? ' is-secret' : ''}" data-var="${esc(m[1])}" aria-label="Variable ${esc(m[1])}">\${${esc(m[1])}}</button>`);
  }
  function hlVarsBtn(s) {
    return tokenize(s, /\$\{([^}]+)\}/g, (m) => `<button type="button" class="var-token" data-var="${esc(m[1])}" aria-label="Variable ${esc(m[1])}">\${${esc(m[1])}}</button>`);
  }

  /* Inherited (read-only, with source) above This request */
  function layers(group, key) {
    const own = group.own || [];
    const ownKeys = new Set(own.map((r) => r.k));
    const inh = (group.inherited || []).map((r) => `
      <div class="kv-row${ownKeys.has(r.k) ? ' is-overridden' : ''}" role="row">
        <span class="k" role="cell">${esc(r.k)}</span>
        <span class="v" role="cell"><span>${r.v.includes('••••') ? `<span class="redacted">${esc(r.v)}</span>` : hlVars(r.v)}</span></span>
        <span class="s" role="cell">${ownKeys.has(r.k) ? '<span class="chip">overridden</span>' : ''}${chipSrc(r.src, r.at)}</span>
      </div>`).join('');
    const mine = own.map((r, i) => `
      <div class="kv-row${r.isNew ? ' expect-row is-new' : ''}" role="row">
        <span class="k" role="cell"><input class="input" value="${esc(r.k)}" aria-label="Key" data-fk="${key}-k${i}"></span>
        <span class="v" role="cell"><input class="input" value="${esc(r.v)}" aria-label="Value for ${esc(r.k)}" data-fk="${key}-v${i}"></span>
        <span class="s" role="cell"><button class="btn btn-ghost btn-icon btn-sm" aria-label="Remove ${esc(r.k)}">${ic('x')}</button></span>
      </div>`).join('');
    return `
      <section class="layer" aria-label="Inherited">
        <div class="layer-head"><h3 class="section-label">${ic('lock', 'ico-sm')}Inherited <span class="subtle">· read-only</span></h3>
          <span class="subtle">${group.inherited.length} from api, file, config and profile</span></div>
        <div class="glass-dense kv is-readonly" role="table" aria-label="Inherited ${esc(key)}">${inh || '<div class="kv-row" role="row"><span class="k subtle" role="cell">none</span><span role="cell"></span><span role="cell"></span></div>'}</div>
      </section>
      <section class="layer" aria-label="This request">
        <div class="layer-head"><h3 class="section-label">This request</h3>
          <button class="btn btn-sm" data-action="toast" data-msg="Row added">${ic('plus')}Add</button></div>
        <div class="glass-dense kv" role="table" aria-label="This request ${esc(key)}">${mine || '<div class="kv-add subtle">Nothing set here. Inherited values apply.</div>'}</div>
      </section>`;
  }

  /* JSON tree. opts: { root, checkable, checked:Set, collapsed:Set, hits:Set|null, sk } */
  const IDENT = /^[A-Za-z_$][\w$]*$/;
  const childPath = (p, k) => typeof k === 'number' ? `${p}[${k}]` : IDENT.test(k) ? `${p}.${k}` : `${p}["${k}"]`;
  function jsonNode(key, v, path, last, o) {
    const k = key === null || typeof key === 'number' ? '' : `<span class="jk">"${esc(key)}"</span><span class="jp">: </span>`;
    const comma = last ? '' : '<span class="jp">,</span>';
    const dim = o.hits && !o.hits.has(path) ? ' is-dim' : '';
    const hit = o.hitExact && o.hitExact.has(path) ? ' is-hit' : '';
    if (v && typeof v === 'object') {
      const arr = Array.isArray(v);
      const ents = arr ? v.map((x, i) => [i, x]) : Object.entries(v);
      const col = o.collapsed && o.collapsed.has(path);
      const [op, cl] = arr ? ['[', ']'] : ['{', '}'];
      const sum = `<span class="jsum"> ${ents.length} ${arr ? 'items' : 'keys'} </span>`;
      return `<li class="${col ? 'is-collapsed' : ''}${dim}${hit}"><button class="tog" data-action="json-toggle" data-path="${esc(path)}" aria-expanded="${!col}" aria-label="${col ? 'Expand' : 'Collapse'} ${esc(path)}">${ic('chev-down')}</button>${k}<span class="jp">${op}</span>${col ? sum + `<span class="jp">${cl}</span>` + comma : ''}<ul>${ents.map(([kk, vv], i) => jsonNode(kk, vv, childPath(path, kk), i === ents.length - 1, o)).join('')}</ul>${col ? '' : `<span class="jp">${cl}</span>${comma}`}</li>`;
    }
    const t = v === null ? 'null' : typeof v;
    const txt = esc(JSON.stringify(v));
    const val = o.checkable
      ? `<button class="jv${o.checked && o.checked.has(path) ? ' is-checked' : ''}" data-t="${t}" data-action="pick-value" data-path="${esc(path)}" data-fk="jv-${esc(path)}" aria-label="Add a check on ${esc(path)}, value ${txt}">${txt}</button>`
      : `<span class="jv" data-t="${t}">${txt}</span>`;
    return `<li class="${dim.trim()}${hit}">${k}${val}${comma}</li>`;
  }
  function jsonTree(v, o) {
    o = o || {};
    return `<div class="json" data-sk="${esc(o.sk || 'json')}"><ul>${jsonNode(null, v, o.root || 'body', true, o)}</ul></div>`;
  }
  function getPath(obj, path, root) {
    const parts = path.slice((root || 'body').length).match(/\.[A-Za-z_$][\w$]*|\[\d+\]|\["[^"]*"\]/g) || [];
    return parts.reduce((a, p) => a == null ? a : a[p[0] === '.' ? p.slice(1) : p.startsWith('["') ? p.slice(2, -2) : Number(p.slice(1, -1))], obj);
  }

  VC.h = { $, $$, esc, ic, badge, method, chipSrc, tokenize, hlJson, hlYaml, hlVars, urlTokens, layers, jsonTree, getPath, STATUS };

  /* ---- state ------------------------------------------------------------- */
  const PREF_KEY = 'vc-ui-v2-prefs';
  let prefs = {};
  try { prefs = JSON.parse(localStorage.getItem(PREF_KEY) || '{}') || {}; } catch (e) { prefs = {}; }
  const PREFS = ['theme', 'tint', 'motion', 'transparency', 'orient', 'split', 'dev', 'width', 'density', 'idle', 'tints'];
  const S = VC.state = Object.assign({
    route: 'client-rest', lastClient: 'client-rest', mode: 'client',
    profile: 'local', layers: ['eu'], api: 'users#get', tab: 't1',
    tabs: D.tabs.map((t) => Object.assign({}, t)), nextTab: 5,
    theme: 'dark', tint: 'auto', motion: 'system', transparency: 'system', width: 'fit',
    orient: 'auto', split: 50, dev: false, empty: false, drawer: false, rail: false,
    density: 'comfortable', idle: '12h', tints: {}, lastTests: 'tests'
  }, pick(prefs, PREFS));
  function pick(o, keys) { const r = {}; keys.forEach((k) => { if (o[k] !== undefined) r[k] = o[k]; }); return r; }
  VC.savePrefs = function () {
    try { localStorage.setItem(PREF_KEY, JSON.stringify(pick(S, PREFS))); } catch (e) { /* storage may be blocked */ }
  };
  Object.entries(S.tints || {}).forEach(([id, t]) => { if (id === '@project') D.project.tint = t; else { const p = D.profiles.find((x) => x.id === id); if (p) p.tint = t || undefined; } });
  VC.profile = () => D.profiles.find((p) => p.id === S.profile);
  VC.api = (id) => D.apis[id || S.api];
  VC.serviceOf = (id) => (id || S.api).split('#')[0];
  VC.fileOf = (id) => (D.services.find((s) => s.id === VC.serviceOf(id)) || {}).file || 'services/scratch.api.yaml';

  /* ---- document attributes (theme, tint, a11y) --------------------------- */
  function applyRoot() {
    const r = document.documentElement;
    if (S.theme === 'system') delete r.dataset.theme; else r.dataset.theme = S.theme;
    const p = VC.profile();
    r.dataset.tint = S.tint !== 'auto' ? S.tint : (p && p.tint) || D.project.tint;
    if (S.motion === 'system') delete r.dataset.motion; else r.dataset.motion = S.motion;
    if (S.transparency === 'system') delete r.dataset.transparency; else r.dataset.transparency = S.transparency;
    r.dataset.density = S.density;
  }

  /* ---- shell ------------------------------------------------------------- */
  function sidebar() {
    const p = VC.profile();
    const tests = S.mode === 'tests';
    const layerChips = S.layers.map((id, i) => `<span class="chip layer-chip" title="${esc((D.layers.find((l) => l.id === id) || {}).note)}"><span class="order">${i + 2}</span>${esc(id)}<button class="x" data-action="remove-layer" data-id="${esc(id)}" aria-label="Remove layer ${esc(id)}">${ic('x', 'ico-sm')}</button></span>`).join('');
    return `
    <aside class="sidebar" id="sidebar" aria-label="Sidebar">
      <div class="sb-top">
        <div class="traffic hide-rail" aria-hidden="true"><span></span><span></span><span></span></div>
        <button class="btn btn-ghost btn-icon btn-sm on-l0 rail-toggle" data-action="toggle-rail" aria-label="Toggle sidebar" data-fk="rail">${ic('columns')}</button>
      </div>
      <div class="seg seg-fill mode-switch on-l0" role="group" aria-label="Mode">
        <button aria-pressed="${!tests}" data-action="mode" data-mode="client" data-fk="mode-client" title="Client (⌘1)">${ic('send')}<span class="hide-rail">Client</span><span class="kbd hide-rail">⌘1</span></button>
        <button aria-pressed="${tests}" data-action="mode" data-mode="tests" data-fk="mode-tests" title="Tests (⌘2)">${ic('runs')}<span class="hide-rail">Tests</span><span class="kbd hide-rail">⌘2</span></button>
      </div>
      <div class="profile-row">
        <button class="btn profile-btn" data-action="profile-menu" aria-haspopup="menu" data-fk="profile" title="Profile: ${esc(p.id)}"><span class="tint-dot" aria-hidden="true"></span><span class="label hide-rail">${esc(p.id)}${S.layers.length ? `<small>+ ${S.layers.length} layer${S.layers.length > 1 ? 's' : ''}</small>` : ''}</span>${ic('chev-down', 'chev hide-rail')}</button>
        <button class="btn btn-icon hide-rail" data-action="explain" aria-label="Explain merged config" title="Config explain" data-fk="explain">${ic('layers')}</button>
      </div>
      <div class="add-layer hide-rail" aria-label="Profile layers, applied in order">${layerChips}<button class="chip plus" data-action="add-layer" aria-haspopup="menu" data-fk="add-layer">${ic('plus', 'ico-sm')}layer</button></div>
      <button class="btn search-btn" data-action="palette" data-fk="search" title="Search (⌘K)">${ic('search')}<span class="label hide-rail">Search or run…</span><span class="kbd hide-rail">⌘K</span></button>
      <div class="sb-scroll" data-sk="sidebar">
        ${S.empty ? '' : tests ? testsNavHtml() : pinsHtml() + tabsHtml()}
        <section class="sb-section hide-rail" aria-label="${tests ? 'Tests' : 'Services'}">
          <h2 class="sb-label">${tests ? 'Tests' : 'Services'}${countHtml(tests)}</h2>
          <div class="sb-tree">${S.empty ? '<p class="empty-tree">No services yet. Init, import or open the demo.</p>' : tests ? testsTreeHtml() : treeHtml()}</div>
        </section>
        ${S.empty || tests ? '' : historyHtml()}
      </div>
      <nav class="sb-foot" aria-label="Shared screens">
        ${[['runs', 'runs', 'Runs'], ['insights', 'chart', 'Insights'], ['agents', 'bot', 'Agents'], ['project-settings', 'gear', 'Settings']].map(([r, i, l]) =>
          `<a class="sb-row" href="#${r}"${S.route === r ? ' aria-current="page"' : ''} title="${l}">${ic(i)}<span class="hide-rail">${l}</span></a>`).join('')}
      </nav>
    </aside>`;
  }
  function countHtml(tests) {
    if (S.empty) return '<span class="count">0</span>';
    const TD = VC.testsData;
    const [n, title] = tests && TD ? [TD.order.length, `${TD.order.length} tests in ${Object.keys(TD.files).length} files`]
      : [Object.keys(D.apis).length, `${Object.keys(D.apis).length} apis in ${D.services.length} services`];
    return `<span class="count" title="${title}">${n}</span>`;
  }
  const TESTS_NAV = [['tests', 'flow', 'Explorer'], ['tests-editor', 'code', 'Editor'], ['tests-run', 'play', 'Runner', '⌘R'], ['tests-load', 'chart', 'Load']];
  function testsNavHtml() {
    return `<nav class="sb-section" aria-label="Tests screens">${TESTS_NAV.map(([r, i, l, k]) =>
      `<a class="sb-row" href="#${r}"${S.route === r ? ' aria-current="page"' : ''} title="${l}" data-fk="tn-${r}">${ic(i)}<span class="title hide-rail">${l}</span>${k ? `<span class="kbd hide-rail">${k}</span>` : ''}</a>`).join('')}</nav>`;
  }
  function pinsHtml() {
    return `<section class="sb-section" aria-label="Pinned"><div class="pins">${D.pins.map((p, i) => {
      const a = p.api ? D.apis[p.api] : null;
      const cur = p.api ? S.api === p.api && S.route.startsWith('client') && S.route !== 'client-auth' : S.route === p.route;
      return `<button class="pin l2${cur ? ' is-active' : ''}" data-action="${p.api ? 'open-api' : 'go'}" data-api="${esc(p.api || '')}" data-route="${esc(p.route || '')}" data-fk="pin${i}" title="${esc(p.label)}">${a ? method(a.m) : ic(p.ico)}<span class="pin-name hide-rail">${esc(p.label)}</span></button>`;
    }).join('')}</div></section>`;
  }
  function tabsHtml() {
    return `<section class="sb-section" aria-label="Today's tabs">
      <div class="sb-section-head"><h2 class="sb-label">Today</h2></div>
      <button class="sb-row new-tab" data-action="new-tab" data-fk="new-tab" title="New tab (⌘T)">${ic('plus')}<span class="title hide-rail">New tab</span><span class="kbd hide-rail">⌘T</span></button>
      ${S.tabs.map((t) => {
        const a = t.api ? D.apis[t.api] : null;
        const cur = t.id === S.tab && S.route.startsWith('client') && S.route !== 'client-auth';
        return `<div class="sb-row tab-row"${cur ? ' aria-current="page"' : ''}>
          <button class="tab-open" data-action="open-tab" data-id="${t.id}" data-fk="tab-${t.id}" title="${esc(a ? a.name : t.name)}">${a ? method(a.m) : ic('tab', 'scratch-ico')}<span class="title hide-rail">${esc(a ? a.name : t.name)}</span></button>
          <button class="close hide-rail" data-action="close-tab" data-id="${t.id}" aria-label="Close ${esc(a ? a.name : t.name)}">${ic('x', 'ico-sm')}</button></div>`;
      }).join('')}
    </section>`;
  }
  function treeHtml() {
    return `<ul class="tree" role="tree" aria-label="Services and apis">${D.services.map((s, si) => {
      const open = !(S.closed || []).includes(s.id);
      return `<li class="ti" role="treeitem" aria-expanded="${open}" aria-level="1" tabindex="${si === 0 ? 0 : -1}" data-action="tree-toggle" data-id="${s.id}" data-fk="tree-${s.id}">
        <div class="ti-row">${ic('chev-down', 'chev')}${ic(s.type === 'graphql' ? 'graph' : s.type === 'websocket' ? 'chat' : 'folder')}<span class="ti-name">${esc(s.id)}</span><span class="ti-meta">${s.apis.length}</span></div>
        <ul role="group">${s.apis.map((id) => {
          const a = D.apis[id];
          return `<li class="ti" role="treeitem" aria-level="2" tabindex="-1" aria-selected="${S.api === id && S.route.startsWith('client')}" data-action="open-api" data-api="${id}" data-fk="tree-${id}"><div class="ti-row"><span class="leaf-gap"></span>${method(a.m)}<span class="ti-name">${esc(a.name)}</span></div></li>`;
        }).join('')}</ul></li>`;
    }).join('')}</ul>`;
  }
  /* Tests tree: a file row opens the editor (the load file opens Load); a test row selects that test; ▶ runs it */
  const EDITOR_FILE = 'orders.checkout';
  function ttSelected(f, t) {
    if (f.load) return false;
    const id = `${f.id}#${t}`;
    if (S.route === 'tests-editor') return f.id === EDITOR_FILE && !!VC.ed && VC.ed.sel === 'test' && VC.ed.test === t;
    return S.route === 'tests' && !!VC.tx && VC.tx.sel === id;
  }
  const runBtn = (id, label) => `<button class="ti-run" tabindex="-1" data-action="tt-run" data-id="${esc(id)}" aria-label="Run ${esc(label)}" title="Run">${ic('play', 'ico-sm')}</button>`;
  function testsTreeHtml() {
    let first = true;
    return `<ul class="tree" role="tree" aria-label="Test files and tests">${D.testsTree.map((s) => s.files.map((f) => {
      const ti = first ? 0 : -1; first = false;
      const open = !(S.closed || []).includes('tt:' + f.id);
      const cur = f.load ? S.route === 'tests-load' : S.route === 'tests-editor' && f.id === EDITOR_FILE && !!VC.ed && VC.ed.sel === 'lifecycle';
      return `<li class="ti" role="treeitem" aria-expanded="${open}" aria-level="1" aria-selected="${cur}" tabindex="${ti}" data-action="tt-file" data-file="${esc(f.id)}" data-fk="tt-${f.id}">
        <div class="ti-row"><span class="ti-fold" data-action="tt-fold" title="${open ? 'Collapse' : 'Expand'}">${ic('chev-down', 'chev')}</span>${ic(f.load ? 'chart' : 'file')}<span class="ti-name">${esc(f.id)}</span><span class="ti-meta">${f.tests.length}</span>${f.load ? '' : runBtn(f.id, f.id)}</div>
        <ul role="group">${f.tests.map(([t, st]) => `<li class="ti" role="treeitem" aria-level="2" tabindex="-1" aria-selected="${ttSelected(f, t)}" data-action="tt-test" data-id="${esc(f.load ? f.id : `${f.id}#${t}`)}"${f.load ? ' data-load="1"' : ''} data-fk="tt-${f.id}-${t}"><div class="ti-row"><span class="leaf-gap"></span><span class="dot st-${st}" role="img" aria-label="${STATUS[st][1]}"></span><span class="ti-name">${esc(t)}</span>${f.load ? '' : runBtn(`${f.id}#${t}`, t)}</div></li>`).join('')}</ul></li>`;
    }).join('')).join('')}</ul>`;
  }
  function historyHtml() {
    return `<section class="sb-section sb-history hide-rail" aria-label="History">
      <div class="sb-section-head"><h2 class="sb-label">History</h2><a class="sb-link" href="#history">All</a></div>
      ${D.history.slice(0, 2).map((d) => `<div class="day">${esc(d.day)}</div>${d.items.slice(0, d.day === 'Today' ? 4 : 2).map((it, i) =>
        `<button class="sb-row" data-action="reopen" data-api="${it.api}" data-fk="h-${esc(d.day)}-${i}" title="${esc(it.m + ' ' + it.path)}">${method(it.m)}<span class="title">${esc(it.path)}</span><span class="meta"><span class="hs" data-class="${String(it.s)[0]}">${it.s}</span></span></button>`).join('')}`).join('')}
    </section>`;
  }
  function header() {
    const scr = VC.screens[S.route] || {};
    const crumbs = (scr.crumbs ? scr.crumbs() : [S.route]);
    const p = VC.profile();
    const flag = p.id === 'prod' ? `<span class="prod-flag">${ic('warn', 'ico-sm')}prod · live data</span>` : p.id === 'staging' ? `<span class="prod-flag is-staging">${ic('warn', 'ico-sm')}staging</span>` : '';
    return `<header class="header">
      <button class="btn btn-ghost btn-icon on-l0 menu-btn" data-action="drawer" aria-label="Open sidebar" aria-expanded="${S.drawer}" data-fk="drawer">${ic('menu')}</button>
      <nav aria-label="Breadcrumb" class="grow"><ol class="crumbs">${crumbs.map((c, i) => `<li${i === crumbs.length - 1 ? ' aria-current="page"' : ''}>${esc(c)}</li>`).join('')}</ol></nav>
      <div class="header-actions">
        ${flag}
        <span class="run-status hide-narrow">${scr.status ? scr.status() : ''}</span>
        <button class="btn btn-ghost btn-icon on-l0" data-action="shortcuts" aria-label="Keyboard shortcuts (?)" title="Shortcuts (?)" data-fk="help">${ic('keyboard')}</button>
      </div>
    </header>`;
  }
  function statusline() {
    return `<footer class="statusline" aria-label="Status">
      <span class="item">${ic('cpu', 'ico-sm')}<b>${esc(D.project.engine)}</b></span>
      <span class="item hide-narrow">${ic('branch', 'ico-sm')}${esc(D.project.branch)}</span>
      <span class="item hide-narrow">${ic('pass', 'ico-sm')}${esc(D.project.lastCheck)}</span>
      <span class="spacer"></span>
      <span class="item">${ic('layers', 'ico-sm')}profile <b>${esc([S.profile].concat(S.layers).join(' + '))}</b></span>
      <span class="item hide-narrow">${ic('db', 'ico-sm')}history.db · 1,204 calls</span>
    </footer>`;
  }

  /* ---- render ------------------------------------------------------------ */
  let mounted = null;
  VC.render = function () {
    const app = $('#app');
    const fk = document.activeElement && document.activeElement.closest && document.activeElement.closest('[data-fk]');
    const fkKey = fk ? fk.dataset.fk : null;
    const scroll = {};
    $$('[data-sk]').forEach((el) => { scroll[el.dataset.sk] = [el.scrollTop, el.scrollLeft]; });
    if (mounted && mounted.unmount) mounted.unmount();
    applyRoot();
    app.dataset.width = S.width;
    app.classList.toggle('is-drawer-open', S.drawer);
    app.classList.toggle('is-rail-open', S.rail);
    const scr = VC.screens[S.route] || VC.screens['client-rest'];
    if (scr.enter) scr.enter();
    app.innerHTML = `<div class="shell">${sidebar()}<div class="main">${header()}<main class="stage" id="view" tabindex="-1">${scr.render()}</main>${statusline()}</div></div>${S.drawer ? '<div class="scrim-drawer" data-action="drawer"></div>' : ''}`;
    mounted = scr;
    if (scr.mount) scr.mount($('#view'));
    $$('[data-sk]').forEach((el) => { const s = scroll[el.dataset.sk]; if (s) { el.scrollTop = s[0]; el.scrollLeft = s[1]; } });
    if (fkKey) { const el = $(`[data-fk="${CSS.escape(fkKey)}"]`); if (el) el.focus({ preventScroll: true }); }
    VC.renderDev();
  };

  /* ---- router ------------------------------------------------------------ */
  function onHash() {
    const r = (location.hash || '').slice(1);
    S.route = VC.screens[r] ? r : (S.empty ? 'welcome' : 'client-rest');
    if (S.route.startsWith('client') && S.route !== 'client-auth') S.lastClient = S.route;
    if (S.route === 'tests' || S.route.startsWith('tests-')) { S.mode = 'tests'; S.lastTests = S.route; }
    else if (S.route.startsWith('client') || S.route === 'import' || S.route === 'history') S.mode = 'client';
    S.drawer = false; S.rail = false;
    VC.hidePopovers();
    clearToasts();
    VC.render();
  }
  VC.go = function (route) {
    if (location.hash === '#' + route) onHash(); else location.hash = route;
  };

  /* ---- tabs -------------------------------------------------------------- */
  VC.openApi = function (id, opts) {
    const a = D.apis[id];
    if (!a) return;
    let t = S.tabs.find((x) => x.api === id);
    if (!t) { t = { id: 't' + S.nextTab++, api: id }; S.tabs.push(t); }
    S.tab = t.id; S.api = id;
    if (opts && opts.toast) VC.toast(opts.toast);
    VC.go(a.route || 'client-rest');
  };
  VC.newTab = function () {
    const t = { id: 't' + S.nextTab++, api: null, name: 'Untitled request' };
    S.tabs.push(t); S.tab = t.id;
    VC.go('client-scratch');
  };
  VC.closeTab = function (id) {
    const i = S.tabs.findIndex((t) => t.id === (id || S.tab));
    if (i < 0) return;
    const [t] = S.tabs.splice(i, 1);
    if (!t.api) VC.toast('Scratch request closed', 'It stays in history, so you can reopen it.');
    if (t.id === S.tab) {
      const n = S.tabs[Math.min(i, S.tabs.length - 1)];
      if (n) { S.tab = n.id; if (n.api) return VC.openApi(n.api); return VC.go('client-scratch'); }
      S.tab = null; return VC.go('client-scratch');
    }
    VC.render();
  };
  function cycleTab(dir) {
    if (!S.tabs.length) return;
    const i = S.tabs.findIndex((t) => t.id === S.tab);
    const n = S.tabs[(i + dir + S.tabs.length) % S.tabs.length];
    S.tab = n.id;
    if (n.api) VC.openApi(n.api); else VC.go('client-scratch');
  }

  /* ---- overlays ---------------------------------------------------------- */
  /* VC.toast(msg, sub, tone, act): act = { label, action: fn | action name, ico } adds a button (e.g. Undo) and keeps the toast 9 s */
  VC.toast = function (msg, sub, tone, act) {
    const box = $('#toasts');
    const el = document.createElement('div');
    el.className = 'toast l3' + (act ? ' has-action' : '');
    el.dataset.born = Date.now();
    if (tone) el.dataset.tone = tone;
    el.innerHTML = `${ic(tone === 'warn' ? 'warn' : 'pass')}<div class="msg">${esc(msg)}${sub ? `<small>${esc(sub)}</small>` : ''}</div>${act ? `<button class="btn btn-sm toast-act">${act.ico ? ic(act.ico, 'ico-sm') : ''}${esc(act.label)}</button>` : ''}<button class="btn btn-ghost btn-icon btn-sm toast-x" aria-label="Dismiss">${ic('x')}</button>`;
    el.querySelector('.toast-x').addEventListener('click', () => el.remove());
    if (act) el.querySelector('.toast-act').addEventListener('click', (e) => { el.remove(); if (typeof act.action === 'function') act.action(); else if (A[act.action]) A[act.action](e.currentTarget, e); });
    box.appendChild(el);
    setTimeout(() => { el.classList.add('is-leaving'); setTimeout(() => el.remove(), 260); }, act ? 9000 : 4200);
    return el;
  };
  /* a route change clears toasts, except one raised by the action that navigated */
  function clearToasts() { const now = Date.now(); $$('#toasts .toast').forEach((t) => { if (now - +t.dataset.born > 400) t.remove(); }); }
  VC.dialog = function (html, cls) {
    const d = $('#dlg');
    d.className = 'l3' + (cls ? ' ' + cls : '');
    d.innerHTML = html;
    if (!d.open) d.showModal();
    const f = d.querySelector('[autofocus], .dlg-body input, .dlg-body button, .dlg-foot .btn-primary');
    if (f) f.focus();
  };
  VC.closeDialog = function () { const d = $('#dlg'); if (d.open) d.close(); };
  function anchorTo(cls, el) {
    $$('.' + cls).forEach((x) => { x.classList.remove(cls); if (x.getAttribute('aria-haspopup')) x.setAttribute('aria-expanded', 'false'); });
    el.classList.add(cls);
    if (el.getAttribute('aria-haspopup')) el.setAttribute('aria-expanded', 'true');
  }
  VC.menu = function (anchor, html) {
    const m = $('#menu');
    if (m.matches(':popover-open')) m.hidePopover();
    anchorTo('is-menu-anchor', anchor);
    m.innerHTML = html;
    m.showPopover();
    const f = m.querySelector('[role^="menuitem"]:not([aria-disabled="true"])');
    if (f) f.focus();
  };
  VC.pick = function (anchor, html) {
    const p = $('#pick');
    if (p.matches(':popover-open')) p.hidePopover();
    $$('.is-anchor').forEach((x) => x.classList.remove('is-anchor'));
    anchorTo('is-pick-anchor', anchor);
    p.innerHTML = html;
    p.showPopover();
    const f = p.querySelector('[autofocus], button, input');
    if (f) f.focus();
  };
  VC.hidePopovers = function () { ['#menu', '#pick'].forEach((s) => { const p = $(s); if (p.matches(':popover-open')) p.hidePopover(); }); };
  const menuItem = (label, action, attrs, extra) => `<button class="pop-item" role="${extra && extra.radio ? 'menuitemradio' : 'menuitem'}"${extra && extra.radio ? ` aria-checked="${!!extra.checked}"` : ''}${extra && extra.disabled ? ' aria-disabled="true"' : ''} data-action="${action}" ${attrs || ''}>${extra && extra.ico ? ic(extra.ico) : ''}<span>${label}</span>${extra && extra.meta ? `<span class="meta">${extra.meta}</span>` : ''}</button>`;
  VC.menuItem = menuItem;

  /* var tooltip: value and where it was set (secrets show •••• and their source) */
  function showTip(el) {
    const name = el.dataset.var;
    const v = D.vars[name] || (name.startsWith('secret.') ? { value: '••••', secret: true, src: 'secret', at: 'keychain' } : { value: 'unset', src: 'config', at: 'not set under this profile' });
    VC.tip(el, `<div class="tip-val">${v.secret ? '<span class="redacted">••••</span> <span class="subtle">never shown</span>' : esc(v.value)}</div><div class="tip-src">${chipSrc(v.src)}<span>${esc(v.at)}</span></div>`);
  }
  const hideTip = () => VC.untip();
  /* VC.tip(anchor, html, cls): the one #tip popover, anchored under any element (.is-tip-anchor) */
  VC.tip = function (anchor, html, cls) {
    const tip = $('#tip');
    $$('.is-tip-anchor').forEach((x) => x.classList.remove('is-tip-anchor'));
    anchor.classList.add('is-tip-anchor');
    tip.className = 'l3 tip' + (cls ? ' ' + cls : '');
    tip.innerHTML = html;
    if (!tip.matches(':popover-open')) tip.showPopover();
  };
  VC.untip = function () { const tip = $('#tip'); if (tip.matches(':popover-open')) tip.hidePopover(); };

  /* ---- actions ----------------------------------------------------------- */
  const A = VC.actions;
  A.go = (el) => VC.go(el.dataset.route);
  A['open-api'] = (el) => VC.openApi(el.dataset.api);
  A['open-tab'] = (el) => { const t = S.tabs.find((x) => x.id === el.dataset.id); S.tab = t.id; if (t.api) VC.openApi(t.api); else VC.go('client-scratch'); };
  A['close-tab'] = (el) => VC.closeTab(el.dataset.id);
  A['new-tab'] = () => VC.newTab();
  A.reopen = (el) => VC.openApi(el.dataset.api, { toast: 'Reopened from history' });
  A.mode = (el) => {
    const m = el.dataset.mode;
    if (m === S.mode) return;
    S.mode = m;
    VC.go(m === 'tests' ? S.lastTests || 'tests' : S.lastClient);
  };
  A['tree-toggle'] = (el, ev) => {
    if (ev && ev.target.closest('[role="group"]')) return;
    toggleClosed(el.dataset.id);
    VC.render();
  };
  const toggleClosed = (id) => { S.closed = S.closed || []; S.closed = S.closed.includes(id) ? S.closed.filter((x) => x !== id) : S.closed.concat(id); };
  A['tt-fold'] = (el, ev) => { if (ev) ev.stopPropagation(); toggleClosed('tt:' + el.closest('[data-file]').dataset.file); VC.render(); };
  A['tt-file'] = (el, ev) => {
    if (ev && ev.target.closest && ev.target.closest('[role="group"]')) return;
    const f = el.dataset.file;
    if (f === 'checkout.peak') return VC.go('tests-load');
    if (f !== EDITOR_FILE) VC.toast(`Opened ${f}`, `Sample editor: the mockup holds one file (${EDITOR_FILE}).`);
    VC.go('tests-editor');
  };
  A['tt-test'] = (el) => {
    const id = el.dataset.id;
    if (el.dataset.load) return VC.go('tests-load');
    if (S.route === 'tests-editor' && id.startsWith(EDITOR_FILE + '#') && VC.edSel) { VC.edSel(id); return VC.render(); }
    if (VC.tx) VC.tx.sel = id;
    if (S.route === 'tests') VC.render(); else VC.go('tests');
  };
  A['tt-run'] = (el, ev) => {
    if (ev) ev.stopPropagation();
    const id = el.dataset.id;
    const only = id.includes('#') ? [id] : VC.testsData.order.filter((k) => k.startsWith(id + '#'));
    if (VC.startRun) VC.startRun({ selector: id, only });
  };
  A.toast = (el) => VC.toast(el.dataset.msg || 'Done', el.dataset.sub);
  A['close-dialog'] = () => VC.closeDialog();
  A.drawer = () => { S.drawer = !S.drawer; VC.render(); if (S.drawer) { const f = $('#sidebar .mode-switch button'); if (f) f.focus(); } };
  A['toggle-rail'] = () => { S.rail = !S.rail; VC.render(); };
  A['profile-menu'] = (el) => VC.menu(el, `<div class="pop-label">Profile</div>${D.profiles.map((p) =>
    menuItem(`<span class="tint-dot is-${p.tint || 'project'}"></span>${esc(p.id)}`, 'set-profile', `data-id="${p.id}"`, { radio: true, checked: p.id === S.profile, meta: esc(p.note) })).join('')}
    <div class="pop-sep"></div>${menuItem('Add layer…', 'add-layer', '', { ico: 'plus' })}${menuItem('Explain merged config', 'explain', '', { ico: 'layers' })}${menuItem('Edit profiles', 'go', 'data-route="project-settings"', { ico: 'gear' })}`);
  A['set-profile'] = (el) => {
    VC.hidePopovers();
    S.profile = el.dataset.id;
    const p = VC.profile();
    VC.render();
    VC.toast(`Profile: ${p.id}`, p.tint ? `Tint follows the profile (${p.tint}). ${p.id === 'prod' ? 'Writes ask for confirmation.' : ''}` : 'Tint follows the project.', p.id === 'prod' ? 'warn' : '');
  };
  A['add-layer'] = () => {
    const anchor = $('.add-layer .plus') || $('.profile-btn');
    const left = D.layers.filter((l) => !S.layers.includes(l.id));
    VC.menu(anchor, `<div class="pop-label">Stack a layer on ${esc(S.profile)}</div>${left.length ? left.map((l) => menuItem(esc(l.id), 'push-layer', `data-id="${l.id}"`, { meta: esc(l.note) })).join('') : '<div class="pop-item" aria-disabled="true">Every layer is applied</div>'}`);
  };
  A['push-layer'] = (el) => { VC.hidePopovers(); S.layers.push(el.dataset.id); VC.render(); VC.toast(`Layer ${el.dataset.id} added`, `Applied after ${S.layers.length > 1 ? S.layers[S.layers.length - 2] : S.profile}; later layers win.`); };
  A['remove-layer'] = (el) => { S.layers = S.layers.filter((x) => x !== el.dataset.id); VC.render(); };
  A.explain = () => {
    const anchor = $('.profile-row [data-action="explain"]') || $('.profile-btn');
    const p = VC.profile();
    const rows = D.explain.map((r) => {
      const v = r.key === 'baseUrl' ? p.baseUrl : r.key === 'timeout' && S.layers.includes('slowNetwork') ? '30s' : r.value;
      const at = r.key === 'timeout' && S.layers.includes('slowNetwork') ? 'layer slowNetwork · vibranium.config.yaml:40' : r.at.replace('<p>', p.id);
      return `<span class="k">${esc(r.key)}</span><span class="v">${v === '••••' ? '<span class="redacted">••••</span>' : esc(v)}</span><span class="at">${chipSrc(r.src)}<span class="ellipsis">${esc(at)}</span></span>`;
    }).join('');
    VC.pick(anchor, `<div class="pop-head"><span class="title">Config explain</span><span class="grow"></span><span class="chip">${esc([S.profile].concat(S.layers).join(' → '))}</span></div><div class="explain-table">${rows}</div><div class="row"><button class="btn btn-sm" data-action="go" data-route="project-settings">Open settings</button><span class="grow"></span><span class="subtle">Later layers win</span></div>`);
  };

  /* ---- keyboard ---------------------------------------------------------- */
  const typing = (el) => el && (el.matches('input, textarea, select, [contenteditable="true"]'));
  function onKey(e) {
    const mod = e.metaKey || e.ctrlKey;
    const k = e.key.toLowerCase();
    if (mod && (k === 'k')) { e.preventDefault(); return VC.openPalette && VC.openPalette(); }
    if (mod && e.code === 'Digit1') { e.preventDefault(); return A.mode({ dataset: { mode: 'client' } }); }
    if (mod && e.code === 'Digit2') { e.preventDefault(); return A.mode({ dataset: { mode: 'tests' } }); }
    if ((mod || e.altKey) && e.code === 'KeyT') { e.preventDefault(); return VC.newTab(); }
    if ((mod || e.altKey) && e.code === 'KeyW') { e.preventDefault(); return VC.closeTab(); }
    if (e.ctrlKey && e.key === 'Tab') { e.preventDefault(); return cycleTab(e.shiftKey ? -1 : 1); }
    if (mod && e.code === 'KeyR' && !e.shiftKey && !e.altKey) { e.preventDefault(); return VC.openRun && VC.openRun(); }
    if (mod && e.key === 'Enter') { e.preventDefault(); return A.send && A.send(); }
    if (!mod && !e.altKey && e.key === '?' && !typing(e.target)) { e.preventDefault(); return VC.openShortcuts && VC.openShortcuts(); }
    if (e.key === 'Escape' && (S.drawer || S.rail)) { S.drawer = false; S.rail = false; VC.render(); return; }
    if (e.target.closest('[role="tablist"]')) return tablistKey(e);
    if (e.target.matches('[role="treeitem"]')) return treeKey(e);
    if (e.target.closest('[role="menu"]')) return menuKey(e);
    if (e.target.matches('.divider')) return VC.dividerKey && VC.dividerKey(e);
  }
  function tablistKey(e) {
    const list = e.target.closest('[role="tablist"]');
    const tabs = $$('[role="tab"]', list);
    const i = tabs.indexOf(e.target);
    if (i < 0) return;
    const keys = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 };
    let n = null;
    if (keys[e.key]) n = (i + keys[e.key] + tabs.length) % tabs.length;
    if (e.key === 'Home') n = 0;
    if (e.key === 'End') n = tabs.length - 1;
    if (n === null) return;
    e.preventDefault();
    tabs[n].focus();
    tabs[n].click();
  }
  function treeKey(e) {
    const tree = e.target.closest('[role="tree"]');
    const items = $$('[role="treeitem"]', tree).filter((x) => !x.parentElement.closest('[role="treeitem"][aria-expanded="false"]'));
    const i = items.indexOf(e.target);
    const it = e.target;
    const focus = (el) => { if (!el) return; items.forEach((x) => x.tabIndex = -1); el.tabIndex = 0; el.focus(); };
    if (e.key === 'ArrowDown') { e.preventDefault(); focus(items[i + 1]); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); focus(items[i - 1]); }
    else if (e.key === 'Home') { e.preventDefault(); focus(items[0]); }
    else if (e.key === 'End') { e.preventDefault(); focus(items[items.length - 1]); }
    else if (e.key === 'ArrowRight') { e.preventDefault(); if (it.getAttribute('aria-expanded') === 'false') fold(it); else if (it.getAttribute('aria-expanded') === 'true') focus($('[role="treeitem"]', it)); }
    else if (e.key === 'ArrowLeft') { e.preventDefault(); if (it.getAttribute('aria-expanded') === 'true') fold(it); else focus(it.parentElement.closest('[role="treeitem"]')); }
    else if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); it.click(); }
  }
  function fold(it) { toggleClosed(it.dataset.file ? 'tt:' + it.dataset.file : it.dataset.id); VC.render(); }
  function menuKey(e) {
    const items = $$('[role^="menuitem"]:not([aria-disabled="true"])', e.target.closest('[role="menu"]'));
    const i = items.indexOf(document.activeElement);
    if (e.key === 'ArrowDown') { e.preventDefault(); (items[i + 1] || items[0]).focus(); }
    if (e.key === 'ArrowUp') { e.preventDefault(); (items[i - 1] || items[items.length - 1]).focus(); }
  }

  /* ---- dev corner -------------------------------------------------------- */
  const SCREENS = [['client-rest', 'REST'], ['client-scratch', 'Scratch'], ['client-graphql', 'GraphQL'], ['client-ws', 'WebSocket'], ['client-sse', 'SSE'],
    ['client-auth', 'Auth registry'], ['history', 'History'], ['import', 'Import'], ['welcome', 'Onboarding'], ['tests', 'Tests'], ['tests-editor', 'Editor'], ['tests-run', 'Runner'],
    ['tests-load', 'Load'], ['runs', 'Runs'], ['runs/compare', 'Compare runs'], ['insights', 'Insights'], ['agents', 'Agents'], ['project-settings', 'Settings']];
  VC.renderDev = function () {
    const box = $('#devcorner');
    const seg = (name, opts, cur) => `<div class="seg seg-sm" role="group" aria-label="${name}">${opts.map(([v, l]) => `<button aria-pressed="${cur === v}" data-action="dev" data-k="${name}" data-v="${v}">${l}</button>`).join('')}</div>`;
    const reduceM = S.motion === 'reduce' || (S.motion === 'system' && matchMedia('(prefers-reduced-motion: reduce)').matches);
    const reduceT = S.transparency === 'reduce' || (S.transparency === 'system' && matchMedia('(prefers-reduced-transparency: reduce)').matches);
    box.innerHTML = `${S.dev ? `<div class="devcorner-panel l3" role="region" aria-label="Review controls">
      <div class="row"><b>Review controls</b><span class="subtle">mockup only</span></div>
      <div class="row"><span>Theme</span>${seg('theme', [['dark', 'Dark'], ['light', 'Light'], ['system', 'Auto']], S.theme)}</div>
      <div class="row"><span>Tint</span><div class="row" role="group" aria-label="Tint">${['auto', 'violet', 'blue', 'teal', 'amber', 'red'].map((t) => `<button class="tint-swatch" data-t="${t}" aria-pressed="${S.tint === t}" data-action="dev" data-k="tint" data-v="${t}" aria-label="Tint ${t === 'auto' ? 'follows profile' : t}" title="${t === 'auto' ? 'Follow profile' : t}"></button>`).join('')}</div></div>
      <label class="switch row"><span>Reduce motion</span><input type="checkbox" data-action="dev-check" data-k="motion" ${reduceM ? 'checked' : ''}><span class="track"></span></label>
      <label class="switch row"><span>Reduce transparency</span><input type="checkbox" data-action="dev-check" data-k="transparency" ${reduceT ? 'checked' : ''}><span class="track"></span></label>
      <div class="row"><span>Width</span>${seg('width', [['fit', 'Fit'], ['1440', '1440'], ['1280', '1280'], ['1024', '1024'], ['760', '760']], S.width)}</div>
      <label class="switch row"><span>Empty project</span><input type="checkbox" data-action="dev-check" data-k="empty" ${S.empty ? 'checked' : ''}><span class="track"></span></label>
      <div class="stack-tight"><span class="subtle">Screens</span><div class="screens">${SCREENS.map(([r, l]) => r.includes('/') ? `<button data-action="go-view" data-route="${r}">${l}</button>` : `<a href="#${r}"${S.route === r ? ' aria-current="page"' : ''}>${l}</a>`).join('')}</div></div>
    </div>` : ''}<button class="btn btn-sm devcorner-toggle" data-action="dev-toggle" aria-expanded="${S.dev}">${ic('sliders')}${S.dev ? 'Hide' : 'Review'}</button>`;
  };
  A.dev = (el) => {
    const k = el.dataset.k;
    S[k] = el.dataset.v;
    VC.savePrefs();
    VC.render();
    const b = $(`#devcorner [data-k="${k}"][data-v="${CSS.escape(el.dataset.v)}"]`); if (b) b.focus();
  };
  /* route/view, e.g. runs/compare: the screen exposes VC.screens[route].view(name) */
  A['go-view'] = (el) => VC.goView(el.dataset.route);
  VC.goView = function (rv) { const [r, v] = rv.split('/'); const scr = VC.screens[r]; if (scr && scr.view) scr.view(v); VC.go(r); };
  A['dev-toggle'] = () => { S.dev = !S.dev; VC.savePrefs(); VC.renderDev(); $('#devcorner .devcorner-toggle').focus(); };
  function onDevChange(e) {
    const el = e.target.closest('[data-action="dev-check"]');
    if (!el) return;
    if (el.dataset.k === 'empty') { S.empty = el.checked; VC.go(S.empty ? 'welcome' : 'client-rest'); return; }
    S[el.dataset.k] = el.checked ? 'reduce' : 'full';
    VC.savePrefs();
    VC.render();
    const b = $(`#devcorner [data-k="${el.dataset.k}"]`); if (b) b.focus();
  }

  /* ---- boot -------------------------------------------------------------- */
  function onClick(e) {
    const el = e.target.closest('[data-action]');
    if (!el || el.matches('input[type="checkbox"]')) return;
    const fn = A[el.dataset.action];
    if (!fn) return;
    if (el.closest('#menu') && !el.dataset.keepOpen) VC.hidePopovers();
    if (el.tagName === 'A') e.preventDefault();
    fn(el, e);
  }
  VC.boot = function () {
    document.addEventListener('click', onClick);
    document.addEventListener('keydown', onKey);
    document.addEventListener('change', onDevChange);
    document.addEventListener('mouseover', (e) => { const v = e.target.closest('.var-token[data-var]'); if (v) showTip(v); });
    document.addEventListener('mouseout', (e) => { if (e.target.closest('.var-token[data-var]')) hideTip(); });
    document.addEventListener('focusin', (e) => { if (e.target.matches('.var-token[data-var]')) showTip(e.target); });
    document.addEventListener('focusout', (e) => { if (e.target.matches('.var-token[data-var]')) hideTip(); });
    $('#menu').addEventListener('toggle', (e) => { if (e.newState === 'closed') $$('.is-menu-anchor[aria-haspopup]').forEach((x) => x.setAttribute('aria-expanded', 'false')); });
    $('#dlg').addEventListener('click', (e) => { if (e.target === $('#dlg')) VC.closeDialog(); });
    window.addEventListener('hashchange', onHash);
    onHash();
  };
  document.addEventListener('DOMContentLoaded', () => VC.boot());
})();
