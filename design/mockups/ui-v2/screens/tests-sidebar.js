/* Tests space · the sidebar (VC.spaces.tests.sidebar) and what it shares with
   the read view: the selection (VC.tx) and the query model (VC.txq).

   Sidebar, top to bottom:
     kind seg   All · API · Load
     filter     one picker: status, service, tag, changed, saved views
     chips      the active query tokens, each removable
     sections   Tests (All tests + the file tree; load tests carry a "load" chip)
                Changes (the git pane, screens/tests-git.js)
   Selection: VC.tx.view is 'table' (All tests), 'file' (VC.tx.file) or 'test'
   (VC.tx.sel). A load test opens #tests-load. */
(function () {
  'use strict';
  const VC = window.VC, S = VC.state, A = VC.actions, TD = VC.testsData;
  const { $, esc, ic } = VC.h;

  const TX = (VC.tx = { sel: null, file: null, view: 'table', q: '', kind: 'all', checked: new Set(), check: null });

  /* ---- query: tag:smoke status:failed api:orders, OR, -term excludes ------ */
  const STQ = { failed: 'fail', fail: 'fail', passed: 'pass', pass: 'pass', flaky: 'flaky', skipped: 'skipped', blocked: 'blocked', allowed: 'allow', allow: 'allow' };
  const PRED = {
    tag: (t, v) => TD.allTags(t).includes(v),
    status: (t, v) => t.st === (STQ[v] || v),
    api: (t, v) => t.apis.some((a) => a === v || a.split('#')[0] === v),
    service: (t, v) => t.file.split('.')[0] === v,
    file: (t, v) => t.file === v,
    changed: (t, v) => TD.files[t.file].changed === (v !== 'no'),
    text: (t, v) => `${t.full} ${t.name}`.toLowerCase().includes(v)
  };
  /* a load test answers the keys that make sense for it; the rest never match */
  const LPRED = {
    tag: (l, v) => l.tags.includes(v),
    status: (l, v) => l.st === (STQ[v] || v),
    changed: (l, v) => v === 'no',
    text: (l, v) => `${l.id} ${l.name}`.toLowerCase().includes(v)
  };
  function parse(q) {
    const bad = [];
    const groups = q.trim().split(/\s+OR\s+/).map((g) => g.split(/\s+/).filter(Boolean).map((tok) => {
      const neg = /^[-!]/.test(tok);
      const body = neg ? tok.slice(1) : tok;
      const i = body.indexOf(':');
      const key = i > 0 ? body.slice(0, i) : 'text';
      if (i > 0 && !PRED[key]) bad.push(key);
      return { neg, key: PRED[key] ? key : 'text', val: (i > 0 && PRED[key] ? body.slice(i + 1) : body).toLowerCase() };
    })).filter((g) => g.length);
    return { groups, bad };
  }
  const match = (preds, x, groups) => !groups.length || groups.some((g) => g.every((c) => (preds[c.key] ? preds[c.key](x, c.val) : false) !== c.neg));
  function visible() {
    if (TX.kind === 'load') return [];
    const { groups } = parse(TX.q);
    return TD.order.map((k) => TD.tests[k]).filter((t) => match(PRED, t, groups));
  }
  function visibleLoad() {
    if (TX.kind === 'api') return [];
    const { groups } = parse(TX.q);
    return TD.loadList.filter((l) => match(LPRED, l, groups));
  }
  const toks = () => TX.q.split(/\s+/).filter(Boolean);
  const hasTok = (tok) => toks().includes(tok);
  function toggleTok(tok) { TX.q = (hasTok(tok) ? toks().filter((p) => p !== tok) : toks().concat(tok)).join(' '); }
  const FILTER_RE = /^-?(status|service|tag|changed|api|file):/;
  const nFilters = () => toks().filter((x) => FILTER_RE.test(x)).length;
  VC.txq = { parse, visible, visibleLoad, hasTok, toggleTok, nFilters, STQ };

  /* ---- filter picker ------------------------------------------------------- */
  const STATUS_F = [['failed', 'fail'], ['flaky', 'flaky'], ['blocked', 'blocked'], ['skipped', 'skipped'], ['passed', 'pass'], ['allowed', 'allow']];
  function filterPop() {
    const item = (k, v, label, dot) => {
      const on = hasTok(k + ':' + v);
      return `<button class="pop-item" role="menuitemcheckbox" aria-checked="${on}" data-action="ts-facet" data-tok="${k}:${esc(v)}">${on ? ic('check') : '<span class="leaf-gap"></span>'}${dot ? `<span class="dot st-${dot}" aria-hidden="true"></span>` : ''}<span>${esc(label)}</span></button>`;
    };
    const svcs = [...new Set(TD.order.map((x) => TD.tests[x].file.split('.')[0]))];
    const tags = [...new Set(TD.order.flatMap((x) => TD.allTags(TD.tests[x])).concat(TD.loadList.flatMap((l) => l.tags)))].sort();
    return `<div class="pop-menu ts-pop" role="group" aria-label="Filter tests">
      <div class="pop-label">Status</div>${STATUS_F.map(([q, st]) => item('status', q, q, st)).join('')}
      <div class="pop-sep"></div><div class="pop-label">Tag</div><div class="ts-pop-tags">${tags.map((v) => item('tag', v, v)).join('')}</div>
      <div class="pop-sep"></div><div class="pop-label">Service</div>${svcs.map((v) => item('service', v, v)).join('')}
      <div class="pop-sep"></div>${item('changed', 'yes', 'Changed on this branch')}
      <div class="pop-sep"></div><div class="pop-label">Saved views</div>${TD.savedViews.map((v, i) => `<button class="pop-item" role="menuitemradio" aria-checked="${TX.q === v.q}" data-action="ts-view" data-i="${i}">${TX.q === v.q ? ic('check') : '<span class="leaf-gap"></span>'}<span class="grow">${esc(v.name)}</span><code class="pop-meta">${esc(v.q)}</code></button>`).join('')}
      <button class="pop-item" data-action="ts-save-view"${TX.q.trim() ? '' : ' aria-disabled="true" disabled'}>${ic('plus')}<span>Save this filter as a view</span></button>
      ${TX.q ? `<div class="pop-sep"></div><button class="pop-item" data-action="ts-clear">${ic('x')}<span>Clear filters</span></button>` : ''}</div>`;
  }
  function openFilter(focusSel) {
    const anchor = $('[data-fk="ts-filter"]');
    if (!anchor) return;
    VC.pick(anchor, filterPop());
    const f = focusSel && $(`#pick ${focusSel}`);
    if (f) f.focus();
  }
  VC.txFilter = openFilter;

  /* ---- sidebar pieces ------------------------------------------------------ */
  const KINDS = [['all', 'All'], ['api', 'API'], ['load', 'Load']];
  function barHtml() {
    const n = toks().length;
    return `<div class="ts-bar">
      <div class="seg seg-sm seg-fill ts-kind" role="group" aria-label="Kind of test">${KINDS.map(([k, l]) => `<button aria-pressed="${TX.kind === k}" data-action="ts-kind" data-k="${k}" data-fk="ts-kind-${k}">${l}</button>`).join('')}</div>
      <button class="btn btn-ghost btn-icon btn-sm on-l0 ts-filter${n ? ' is-on' : ''}" data-action="ts-filter" aria-haspopup="dialog" aria-expanded="false" data-fk="ts-filter" aria-label="Filter${n ? `, ${n} on` : ''}" title="Filter and saved views">${ic('filter')}</button>
    </div>`;
  }
  function chipsHtml() {
    const ts = toks();
    if (!ts.length) return '';
    return `<ul class="ts-chips" aria-label="Active filters">${ts.map((t) => `<li><button class="chip ts-chip" data-action="ts-chip-x" data-tok="${esc(t)}" data-fk="ts-chip-${esc(t)}" aria-label="Remove filter ${esc(t)}" title="Remove">${esc(t)}${ic('x', 'ico-sm')}</button></li>`).join('')}</ul>`;
  }
  const onTests = () => S.route === 'tests';
  function fileItem(f, ts, tab) {
    const open = !(S.closed || []).includes('tt:' + f);
    const cur = onTests() && TX.view === 'file' && TX.file === f;
    const hid = !open && onTests() && TX.view === 'test' && ts.some((t) => t.full === TX.sel);
    return `<li class="ti" role="treeitem" aria-level="1" aria-expanded="${open}" aria-selected="${cur}" tabindex="${tab(cur || hid)}" data-action="ts-file" data-file="${esc(f)}" data-fk="ts-f-${esc(f)}">
      <div class="ti-row"><span class="ti-fold" data-action="ts-fold" title="${open ? 'Collapse' : 'Expand'}">${ic('chev-down', 'chev')}</span>${ic('file')}<span class="ti-name" title="${esc(TD.files[f].path)}">${esc(f)}</span>${TD.files[f].changed ? '<span class="ts-mod" title="Changed on this branch" aria-label="changed">M</span>' : ''}<span class="ti-meta">${ts.length}</span></div>
      <ul role="group">${ts.map((t) => {
        const on = open && onTests() && TX.view === 'test' && TX.sel === t.full;
        return `<li class="ti" role="treeitem" aria-level="2" aria-selected="${onTests() && TX.view === 'test' && TX.sel === t.full}" tabindex="${tab(on)}" data-action="ts-test" data-id="${esc(t.full)}" data-fk="ts-t-${esc(t.full)}"><div class="ti-row"><span class="leaf-gap"></span><span class="dot st-${t.st}" role="img" aria-label="${t.st}"></span><span class="ti-name mono" title="${esc(t.name)}">${esc(t.id)}</span></div></li>`;
      }).join('')}</ul></li>`;
  }
  function loadItem(l, tab) {
    const on = S.route === 'tests-load' && (VC.ld || {}).cur === l.id;
    return `<li class="ti ts-load" role="treeitem" aria-level="1" aria-selected="${on}" tabindex="${tab(on)}" data-action="ts-load" data-id="${esc(l.id)}" data-fk="ts-l-${esc(l.id)}">
      <div class="ti-row"><span class="leaf-gap"></span><span class="dot st-${l.st}" role="img" aria-label="${l.st}"></span><span class="ti-name" title="${esc(l.name)} · ${esc(l.path)}">${esc(l.id)}</span><span class="chip ts-load-chip">load</span></div></li>`;
  }
  function treeHtml() {
    const vis = visible(), loads = visibleLoad();
    const files = Object.keys(TD.files).map((f) => [f, vis.filter((t) => t.file === f)]).filter(([, ts]) => ts.length);
    if (!files.length && !loads.length) return `<p class="empty-quiet empty-tree">No tests match. <button class="link" data-action="ts-clear" data-fk="ts-clear">Clear filters</button></p>`;
    /* one tab stop: the selected item, else the first */
    const any = hasSel(files, loads);
    let i = 0;
    const tab = (on) => ((any ? on : i++ === 0) ? 0 : -1);
    return `<ul class="tree" role="tree" aria-label="Tests">${files.map(([f, ts]) => fileItem(f, ts, tab)).join('')}${loads.map((l) => loadItem(l, tab)).join('')}</ul>`;
  }
  function hasSel(files, loads) {
    if (S.route === 'tests-load') return loads.some((l) => (VC.ld || {}).cur === l.id);
    if (!onTests()) return false;
    if (TX.view === 'file') return files.some(([f]) => f === TX.file);
    if (TX.view === 'test') return files.some(([, ts]) => ts.some((t) => t.full === TX.sel));
    return false;
  }
  function testsBody() {
    const n = visible().length + visibleLoad().length;
    const all = onTests() && TX.view === 'table';
    return `<div class="ts-tests"><button class="sb-row ts-all" data-action="ts-all" data-fk="ts-all"${all ? ' aria-current="page"' : ''}>${ic('rows')}<span class="title">All tests</span><span class="meta num">${n}</span></button>
      <div class="sb-tree">${treeHtml()}</div></div>`;
  }
  VC.spaces = VC.spaces || {};
  VC.spaces.tests = VC.spaces.tests || { label: 'Tests', icon: 'flow', home: 'tests' };
  VC.spaces.tests.sidebar = function () {
    const n = VC.gitCount ? VC.gitCount() : 0;
    const secs = [
      { id: 'tests', label: 'Tests', body: testsBody(), sk: 'ts-tests' },
      { id: 'changes', label: 'Changes', meta: n ? `<span class="ts-count num" aria-label="${n} changed files">${n}</span>` : '', body: VC.gitPane ? VC.gitPane() : '', sk: 'ts-git' }
    ];
    return `<div class="ts-side">${barHtml()}${chipsHtml()}${VC.sbSections('tests', secs)}</div>`;
  };

  /* ---- navigation ---------------------------------------------------------- */
  function show(view, patch) {
    Object.assign(TX, { view }, patch || {});
    TX.check = null;
    S.drawer = false;   /* below 1100px the sidebar is a drawer: picking a row closes it */
    if (S.route === 'tests') VC.render(); else VC.go('tests');
  }
  VC.txOpen = (id) => show('test', { sel: id, file: TD.tests[id].file });
  VC.txFile = (f) => show('file', { file: f });
  VC.txAll = () => show('table');

  /* ---- actions ------------------------------------------------------------- */
  A['ts-kind'] = (el) => { TX.kind = el.dataset.k; VC.render(); };
  A['ts-filter'] = () => openFilter();
  A['ts-facet'] = (el) => { toggleTok(el.dataset.tok); VC.render(); openFilter(`[data-tok="${CSS.escape(el.dataset.tok)}"]`); };
  A['ts-view'] = (el) => { const v = TD.savedViews[+el.dataset.i]; TX.q = TX.q === v.q ? '' : v.q; VC.hidePopovers(); VC.render(); };
  A['ts-save-view'] = () => {
    if (!TX.q.trim()) return;
    TD.savedViews.push({ name: TX.q, q: TX.q });
    VC.hidePopovers();
    VC.toast('View saved', `${TX.q} · kept in .vc/ui.json on this machine`);
  };
  A['ts-clear'] = () => { TX.q = ''; VC.hidePopovers(); VC.render(); };
  A['ts-chip-x'] = (el) => {
    toggleTok(el.dataset.tok);
    VC.render();
    const f = $('.ts-chip') || $('[data-fk="ts-filter"]');
    if (f) f.focus();
  };
  A['ts-all'] = () => VC.txAll();
  A['ts-fold'] = (el, ev) => {
    if (ev) ev.stopPropagation();
    const k = 'tt:' + el.closest('[data-file]').dataset.file;
    S.closed = (S.closed || []).includes(k) ? S.closed.filter((x) => x !== k) : (S.closed || []).concat(k);
    VC.render();
  };
  A['ts-file'] = (el, ev) => {
    if (ev && ev.target.closest && ev.target.closest('[role="group"]')) return;
    VC.txFile(el.dataset.file);
  };
  A['ts-test'] = (el) => VC.txOpen(el.dataset.id);
  A['ts-load'] = (el) => {
    const l = TD.loadList.find((x) => x.id === el.dataset.id);
    if (VC.ldOpen) VC.ldOpen(l.id); else VC.go('tests-load');
  };
})();
