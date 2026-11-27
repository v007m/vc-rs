/* Vibranium UI v2 · shell-spaces.js: the five spaces, the space switcher, the
   sidebar frame and its helpers, and the top row's title. Loaded after app.js,
   before the screens.

   Spaces: VC.spaces.<key> = { key, label, icon, home, n, sidebar }. A space file
   attaches its sidebar body: VC.spaces.tests.sidebar = () => html.
   Switcher (pref S.switcher):
     'rail'  an icon rail on L0 left of the sidebar (traffic lights, four spaces,
             Settings pinned at the bottom). It stays when the sidebar is hidden.
     'foot'  the five icons in a row at the sidebar foot; when the sidebar is not
             in the layout a minimal strip of the same icons stays at the left.
   Helpers for every space's sidebar:
     VC.sbSections(group, [{ id, label, meta, actions, body, sk }])
     VC.sbNav([{ label, icon, current, action, attrs, meta, fk }])
   Actions: space (data-space), sb-sec (data-id "group:id"), sb-toggle.
   Keys: ⌘1–⌘5 switch spaces in order, ⌘, opens Settings. */
(function () {
  'use strict';
  const VC = window.VC, S = VC.state, A = VC.actions;
  const { $, esc, ic } = VC.h;

  /* ---- the registry ---------------------------------------------------- */
  const SPACES = [
    ['client', 'Client', 'send', 'client-rest'],
    ['tests', 'Tests', 'flow', 'tests'],
    ['runs', 'Runs', 'runs', 'runs'],
    ['agents', 'Agents', 'bot', 'agents'],
    ['settings', 'Settings', 'gear', 'project-settings']
  ];
  VC.spaces = {};
  SPACES.forEach(([key, label, icon, home], i) => { VC.spaces[key] = { key, label, icon, home, n: i + 1, sidebar: () => '' }; });
  VC.spaces.client.sidebar = () => VC.clientSidebar();   /* app.js: Requests and History */
  const KEYS = SPACES.map(([k]) => k);

  /* a screen names its space; the route prefix is the fallback */
  const CLIENT = ['history', 'import', 'welcome'];
  const RUNS = ['insights', 'schedules'];
  VC.spaceOf = function (route) {
    const scr = VC.screens[route];
    if (scr && VC.spaces[scr.space]) return scr.space;
    if (route.startsWith('client') || CLIENT.includes(route)) return 'client';
    if (route.startsWith('runs') || RUNS.includes(route)) return 'runs';
    if (route.startsWith('tests')) return 'tests';
    if (route.startsWith('agents')) return 'agents';
    if (route.endsWith('settings')) return 'settings';
    return 'client';
  };
  /* the route a space opens on: its last route, else its home */
  VC.spaceRoute = function (key) {
    const sp = VC.spaces[key];
    const last = S.lastRoute[key];
    return last && VC.screens[last] ? last : sp.home;
  };
  VC.goSpace = function (key) {
    if (!VC.spaces[key]) return;
    /* the current space again: back to its home (or just close the drawer) */
    const r = key === S.space ? VC.spaces[key].home : VC.spaceRoute(key);
    if (r === S.route) { if (S.drawer) { S.drawer = false; VC.render(); } return; }
    VC.go(r);
  };
  A.space = (el) => VC.goSpace(el.dataset.space);

  /* app prefs set from Settings › Appearance (switcher, theme, density …) */
  VC.setPref = function (key, value) {
    S[key] = value;
    VC.savePrefs();
    VC.render();
  };

  /* ---- switcher -------------------------------------------------------- */
  const kbd = (sp) => (sp.key === 'settings' ? '⌘,' : `⌘${sp.n}`);
  function spaceBtn(sp, where) {
    const cur = sp.key === S.space;
    return `<button class="sp-btn" data-action="space" data-space="${sp.key}" aria-label="${sp.label}"${cur ? ' aria-current="page"' : ''} title="${sp.label} (${kbd(sp)})" data-fk="sp-${where}-${sp.key}">${ic(sp.icon)}<span class="sp-label" aria-hidden="true">${sp.label}</span></button>`;
  }
  const li = (k, where) => `<li>${spaceBtn(VC.spaces[k], where || 'rail')}</li>`;
  const TRAFFIC = '<div class="traffic" aria-hidden="true"><span></span><span></span><span></span></div>';
  VC.shellRail = function () {
    return `<nav class="rail" aria-label="Spaces">${TRAFFIC}
      <ul class="rail-list">${KEYS.slice(0, -1).map((k) => li(k)).join('')}</ul>
      <ul class="rail-list rail-end">${li('settings')}</ul>
    </nav>`;
  };
  const footRow = () => `<nav class="sb-spaces" aria-label="Spaces"><ul class="sb-spaces-list">${KEYS.map((k) => li(k, 'foot')).join('')}</ul></nav>`;

  /* ---- sidebar frame --------------------------------------------------- */
  function body(sp) {
    try { return sp.sidebar() || ''; } catch (err) { console.error(err); return ''; }
  }
  VC.shellSidebar = function () {
    const sp = VC.spaces[S.space] || VC.spaces.client;
    const foot = S.switcher === 'foot';
    return `<aside class="sidebar" id="sidebar" aria-label="${sp.label} sidebar">
      <div class="sb-top">
        ${foot ? TRAFFIC : `<h2 class="sb-title">${sp.label}</h2>`}
        <button class="btn btn-ghost btn-icon btn-sm on-l0 sb-hide" data-action="sb-toggle" aria-controls="sidebar" aria-label="Hide sidebar" title="Hide sidebar" data-fk="sb-hide">${ic('columns')}</button>
      </div>
      <div class="sb-body" data-sk="sb-${sp.key}">${body(sp)}</div>
      ${foot ? footRow() : ''}
    </aside>`;
  };

  /* collapsible sections, stacked; open ones share the height equally */
  VC.sbSections = function (group, sections) {
    const closed = S.sbClosed || [];
    return `<div class="sb-secs">${sections.map((s) => {
      const key = `${group}:${s.id}`;
      const id = `sbs-${group}-${s.id}`;
      const open = !closed.includes(key);
      return `<section class="sb-sec${open ? ' is-open' : ''}" aria-labelledby="${id}-h">
        <div class="sb-sec-head">
          <button class="sb-sec-btn" id="${id}-h" data-action="sb-sec" data-id="${esc(key)}" aria-expanded="${open}" aria-controls="${id}" data-fk="${id}"><span class="sb-sec-label">${esc(s.label)}</span>${s.meta ? `<span class="sb-sec-meta">${s.meta}</span>` : ''}</button>
          ${s.actions ? `<span class="sb-sec-acts">${s.actions}</span>` : ''}
          ${ic('chev-down', 'ico-sm sb-sec-chev')}
        </div>
        <div class="sb-sec-body fade-end" id="${id}" data-sk="${esc(s.sk || key)}"${open ? '' : ' hidden'}>${s.body || ''}</div>
      </section>`;
    }).join('')}</div>`;
  };
  A['sb-sec'] = (el) => {
    const id = el.dataset.id;
    const c = S.sbClosed || [];
    S.sbClosed = c.includes(id) ? c.filter((x) => x !== id) : c.concat(id);
    VC.savePrefs();
    VC.render();
  };

  /* sidebar nav rows */
  VC.sbNav = function (items) {
    return `<ul class="sb-nav">${items.map((it) => `<li><button class="sb-row"${it.action ? ` data-action="${esc(it.action)}"` : ''} ${it.attrs || ''}${it.current ? ' aria-current="page"' : ''}${it.fk ? ` data-fk="${esc(it.fk)}"` : ''}>${it.icon ? ic(it.icon) : ''}<span class="title">${esc(it.label)}</span>${it.meta ? `<span class="meta">${it.meta}</span>` : ''}</button></li>`).join('')}</ul>`;
  };

  /* hide/show: in the layout above 1100px (a saved pref), a drawer below */
  const inFlowWidth = () => { const a = $('#app'); return !a || a.clientWidth > 1100; };
  A['sb-toggle'] = () => {
    if (inFlowWidth()) { S.sbCollapsed = !S.sbCollapsed; S.drawer = false; VC.savePrefs(); }
    else S.drawer = !S.drawer;
    VC.render();
    const open = inFlowWidth() ? !S.sbCollapsed : S.drawer;
    const f = open ? $('#sidebar .sb-body button, #sidebar .sb-body [tabindex="0"], #sidebar .sb-hide') : $('.topbar .sb-show');
    if (f) f.focus();
  };

  /* ---- top row title (every space but Client) -------------------------- */
  function call(fn, fallback) {
    if (!fn) return fallback;
    try { return fn(); } catch (err) { console.error(err); return fallback; }
  }
  VC.shellTitle = function (scr) {
    let crumbs = call(scr.crumbs, []).slice(1);
    if (!crumbs.length) crumbs = [(VC.spaces[S.space] || VC.spaces.client).label];
    const st = call(scr.status, '');
    const pr = call(scr.primary, '');
    const sep = ic('chev-right', 'ico-sm crumb-sep');
    return `<nav class="tb-title" aria-label="Breadcrumb"><ol class="crumbs">${crumbs.map((c, i) =>
      `<li${i === crumbs.length - 1 ? ' aria-current="page"' : ''}>${i ? sep : ''}<span class="crumb">${esc(c)}</span></li>`).join('')}</ol></nav>
      ${st ? `<span class="run-status tb-status hide-narrow">${st}</span>` : ''}
      <span class="grow"></span>
      ${pr ? `<div class="tb-primary">${pr}</div>` : ''}`;
  };

  /* ---- keys ------------------------------------------------------------ */
  document.addEventListener('keydown', (e) => {
    if (!(e.metaKey || e.ctrlKey) || e.altKey || e.shiftKey) return;
    const m = /^Digit([1-5])$/.exec(e.code);
    if (m) { e.preventDefault(); VC.goSpace(KEYS[+m[1] - 1]); }
    else if (e.key === ',') { e.preventDefault(); VC.goSpace('settings'); }
  });
})();
