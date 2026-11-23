/* Tests mode · Explorer (#tests). Also owns what the other Tests screens share:
   the Tests sub-nav (VC.testsNav) and the undo toast (VC.undoToast). The sidebar tree lives in app.js. */
(function () {
  'use strict';
  const VC = window.VC, D = VC.data, S = VC.state, A = VC.actions, TD = VC.testsData;
  const { $, $$, esc, ic, badge, method, chipSrc } = VC.h;
  const TODAY = '2026-11-23';

  const TX = (VC.tx = { sel: 'orders.checkout#rejectsDeclinedCard', q: '', checked: new Set() });
  if (S.txSplit == null) S.txSplit = 54;

  /* ---- shared: sub-nav, undo toast, day maths ---------------------------- */
  const NAV = [['tests', 'Explorer', 'flow'], ['tests-editor', 'Editor', 'code'], ['tests-run', 'Runner', 'play'], ['tests-load', 'Load', 'chart']];
  VC.testsNav = (cur, right) => `<div class="tx-bar glass">
    <nav class="tabs tx-nav" aria-label="Tests mode">${NAV.map(([r, l, i]) => `<a class="tab" href="#${r}" data-fk="txnav-${r}"${r === cur ? ' aria-current="page"' : ''}>${ic(i, 'ico-sm')}${l}</a>`).join('')}</nav>
    <span class="grow"></span>${right || ''}</div>`;
  VC.runBtn = (label) => `<button class="btn btn-primary btn-sm" data-action="tx-run-dialog" data-fk="tx-runbtn">${ic('play')}${esc(label || 'Run')}<span class="kbd hide-narrow">⌘R</span></button>`;
  /* an undo toast is a VC.toast with an Undo action */
  VC.undoToast = function (msg, sub, fn) {
    VC.toast(msg, sub, '', { label: 'Undo', ico: 'arrow-left', action: () => { fn(); VC.render(); VC.toast('Undone', 'The files are back as they were; vc check · 0 errors'); } });
  };
  const dropWroteToast = () => { const t = $('#toasts').lastElementChild; if (t && !t.classList.contains('has-action')) t.remove(); };
  const days = (d) => Math.round((Date.parse(d) - Date.parse(TODAY)) / 864e5);
  VC.untilText = (d) => { const n = days(d); return n < 0 ? `expired ${-n} d ago` : n === 0 ? 'expires today' : `expires in ${n} d`; };

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
  function visible() {
    const { groups } = parse(TX.q);
    return TD.order.map((k) => TD.tests[k]).filter((t) => !groups.length || groups.some((g) => g.every((c) => PRED[c.key](t, c.val) !== c.neg)));
  }
  const hasTok = (tok) => TX.q.split(/\s+/).includes(tok);
  function toggleTok(tok) {
    const parts = TX.q.split(/\s+/).filter(Boolean);
    TX.q = (hasTok(tok) ? parts.filter((p) => p !== tok) : parts.concat(tok)).join(' ');
  }

  /* ---- list (left card) --------------------------------------------------- */
  const STATUS_F = [['failed', 'fail'], ['flaky', 'flaky'], ['blocked', 'blocked'], ['skipped', 'skipped'], ['passed', 'pass'], ['allowed', 'allow']];
  function facetsHtml() {
    const cnt = (st) => TD.order.filter((k) => TD.tests[k].st === st).length;
    const svc = TX.q.match(/(?:^|\s)service:(\S+)/), tag = TX.q.match(/(?:^|\s)tag:(\S+)/);
    return `<div class="tx-facets" role="group" aria-label="Filters">
      <div class="tx-chips" role="group" aria-label="Status">${STATUS_F.map(([q, st]) => `<button class="tx-fchip" aria-pressed="${hasTok('status:' + q)}" data-action="tx-facet" data-tok="status:${q}" data-fk="txf-${q}"><span class="dot st-${st}" aria-hidden="true"></span>${q}<span class="count">${cnt(st)}</span></button>`).join('')}</div>
      <span class="grow"></span>
      <button class="btn btn-sm${svc ? ' is-on' : ''}" data-action="tx-facet-pick" data-k="service" aria-haspopup="dialog" data-fk="txf-service">${ic('folder', 'ico-sm')}${svc ? esc(svc[1]) : 'Service'}${ic('chev-down', 'ico-sm')}</button>
      <button class="btn btn-sm${tag ? ' is-on' : ''}" data-action="tx-facet-pick" data-k="tag" aria-haspopup="dialog" data-fk="txf-tag">${ic('pin', 'ico-sm')}${tag ? esc(tag[1]) : 'Tag'}${ic('chev-down', 'ico-sm')}</button>
      <button class="btn btn-sm" aria-pressed="${hasTok('changed:yes')}" data-action="tx-facet" data-tok="changed:yes" data-fk="txf-changed">${ic('branch', 'ico-sm')}Changed</button>
    </div>`;
  }
  function rowHtml(t) {
    const on = TX.checked.has(t.full);
    const flag = t.quarantined ? '<span class="chip">quarantined</span>' : t.skip ? `<span class="chip">skip · ${esc(VC.untilText(t.skip.until))}</span>` : t.allow ? '<span class="chip">allowFailure</span>' : '';
    return `<tr aria-selected="${TX.sel === t.full}" class="${on ? 'is-checked' : ''}">
      <td class="tx-ck"><input type="checkbox" class="tx-check" data-id="${esc(t.full)}" ${on ? 'checked' : ''} aria-label="Select ${esc(t.id)}" data-fk="txc-${esc(t.full)}"></td>
      <td class="tx-name"><button class="tx-open" data-action="tx-sel" data-id="${esc(t.full)}" data-fk="txr-${esc(t.full)}"><span class="dot st-${t.st}" role="img" aria-label="${t.st}"></span><span class="ellipsis"><span class="mono">${esc(t.id)}</span><small>${esc(t.name)}</small></span></button></td>
      <td class="hide-narrow"><span class="tx-tags">${t.tags.map((g) => `<span class="chip">${esc(g)}</span>`).join('')}${flag}</span></td>
      <td class="r mono muted">${t.ms ? t.ms + ' ms' : '—'}</td>
      <td class="r mono ${t.flake >= 10 ? 'tx-hot' : 'muted'}">${t.flake}%</td></tr>`;
  }
  function listHtml() {
    const vis = visible();
    const { bad } = parse(TX.q);
    const byFile = Object.keys(TD.files).map((f) => [f, vis.filter((t) => t.file === f)]).filter(([, ts]) => ts.length);
    const all = vis.length && vis.every((t) => TX.checked.has(t.full));
    const head = `<p class="tx-count" aria-live="polite">${bad.length ? `<span class="tx-bad">${ic('warn', 'ico-sm')}unknown key ${esc(bad.join(', '))}: · keys are tag status api service file changed</span>` : `<b>${vis.length}</b> of ${TD.order.length} tests · ${byFile.length} files`}</p>`;
    if (!vis.length) return head + `<div class="empty">${ic('search')}<b>No tests match</b><span>Try <code>status:failed</code>, <code>tag:smoke OR api:payments</code>, or clear the query.</span><button class="btn btn-sm" data-action="tx-clear">Clear query</button></div>`;
    return head + `<div class="table-wrap glass-dense" data-sk="tx-list"><table class="table tx-table" aria-label="Tests">
      <thead><tr><th class="tx-ck"><input type="checkbox" class="tx-check-all" ${all ? 'checked' : ''} aria-label="Select all ${vis.length} visible tests" data-fk="tx-all"></th><th>Test</th><th class="hide-narrow">Tags</th><th class="r">Last</th><th class="r" title="Share of the last 30 runs that failed, flaked or were allowed to fail">Flake</th></tr></thead>
      ${byFile.map(([f, ts]) => `<tbody><tr class="tx-file"><th colspan="5" scope="rowgroup"><span class="row">${ic('file', 'ico-sm')}<span class="mono">${esc(f)}</span><span class="subtle mono hide-narrow">${esc(TD.files[f].path)}</span>${TD.files[f].changed ? '<span class="chip">changed</span>' : ''}</span></th></tr>${ts.map(rowHtml).join('')}</tbody>`).join('')}
    </table></div>`;
  }
  function bulkHtml() {
    const n = TX.checked.size;
    if (!n) return '';
    return `<div class="tx-bulk l2" role="toolbar" aria-label="${n} selected">
      <b>${n} selected</b><span class="grow"></span>
      <button class="btn btn-sm btn-primary" data-action="tx-bulk-run" data-fk="txb-run">${ic('play', 'ico-sm')}Run</button>
      <button class="btn btn-sm" data-action="tx-edit-dlg" data-kind="skip" data-fk="txb-skip">${ic('skipped', 'ico-sm')}Skip until…</button>
      <button class="btn btn-sm" data-action="tx-edit-dlg" data-kind="tag" data-fk="txb-tag">${ic('pin', 'ico-sm')}Tag…</button>
      <button class="btn btn-sm" data-action="tx-edit-dlg" data-kind="quarantine" data-fk="txb-q">${ic('shield', 'ico-sm')}Quarantine…</button>
      <button class="btn btn-ghost btn-icon btn-sm" data-action="tx-uncheck" aria-label="Clear selection" data-fk="txb-x">${ic('x')}</button></div>`;
  }
  function leftHtml() {
    return `<section class="glass" aria-labelledby="tx-title">
      <div class="card-head"><h2 class="card-title" id="tx-title">Tests</h2><span class="card-sub">16 tests · 6 files · last run today 14:02</span><span class="grow"></span>
        <button class="btn btn-sm btn-ghost" data-action="tx-views" aria-haspopup="menu" data-fk="tx-views">${ic('layers', 'ico-sm')}Views${ic('chev-down', 'ico-sm')}</button></div>
      <div class="card-tabs tx-qrow"><div class="input-group grow">${ic('search')}<input class="input mono" id="tx-q" value="${esc(TX.q)}" placeholder="tag:smoke status:failed api:orders   ·   OR   ·   -tag:slow" aria-label="Filter tests" aria-describedby="tx-q-help" data-fk="tx-q" autocomplete="off" spellcheck="false"></div>
        <span id="tx-q-help" class="sr-only">Keys: tag, status, api, service, file, changed. OR joins groups; a leading minus excludes.</span></div>
      <div class="card-tabs">${facetsHtml()}</div>
      <div class="card-body tx-listbody" id="tx-list">${listHtml()}</div>
      <div id="tx-bulk">${bulkHtml()}</div>
    </section>`;
  }
  function refreshList() {
    const l = $('#tx-list'); if (l) l.innerHTML = listHtml();
    const b = $('#tx-bulk'); if (b) b.innerHTML = bulkHtml();
    const f = $('.tx-facets'); if (f) f.outerHTML = facetsHtml();
  }

  /* ---- detail (right card) ------------------------------------------------ */
  function strip(t) {
    const n = (s) => t.history.filter((x) => x === s).length;
    const parts = [['pass', 'passed'], ['flaky', 'flaky'], ['fail', 'failed'], ['allow', 'allowed'], ['blocked', 'blocked'], ['skipped', 'skipped']].filter(([s]) => n(s)).map(([s, l]) => `${n(s)} ${l}`);
    return `<ol class="tx-strip" aria-label="Last 30 runs, oldest first: ${parts.join(', ')}">${t.history.map((s, i) => `<li class="hc st-${s}" title="run ${453 + i} · ${s}${t.durs[i] ? ' · ' + t.durs[i] + ' ms' : ''}"></li>`).join('')}</ol>`;
  }
  function spark(t) {
    const W = 280, H = 44, P = 4;
    const ds = t.durs.map((d, i) => [i, d]).filter(([, d]) => d != null);
    if (ds.length < 2) return '<p class="subtle">No timings: the test has not run lately.</p>';
    const max = Math.max(...ds.map((d) => d[1])) * 1.1, min = Math.min(...ds.map((d) => d[1])) * 0.85;
    const x = (i) => P + (i / 29) * (W - 2 * P), y = (v) => H - P - ((v - min) / (max - min)) * (H - 2 * P);
    const sorted = ds.map((d) => d[1]).sort((a, b) => a - b), p50 = sorted[Math.floor(sorted.length / 2)], p95 = sorted[Math.floor(sorted.length * 0.95) - 1] || sorted[sorted.length - 1];
    const path = ds.map(([i, d], k) => `${k ? 'L' : 'M'}${x(i).toFixed(1)},${y(d).toFixed(1)}`).join('');
    const last = ds[ds.length - 1];
    return `<figure class="tx-spark"><svg viewBox="0 0 ${W} ${H}" role="img" aria-label="Duration over the last 30 runs: median ${p50} ms, p95 ${p95} ms, last ${last[1]} ms">
      <line class="ref" x1="${P}" x2="${W - P}" y1="${y(p50).toFixed(1)}" y2="${y(p50).toFixed(1)}"/><path class="ln" d="${path}"/>
      <circle class="pt" cx="${x(last[0]).toFixed(1)}" cy="${y(last[1]).toFixed(1)}" r="4"/>
      ${ds.map(([i, d]) => `<rect class="hit" x="${(x(i) - 4.5).toFixed(1)}" y="0" width="9" height="${H}"><title>run ${453 + i} · ${d} ms</title></rect>`).join('')}</svg>
      <figcaption><span><b class="num">${last[1]}</b> ms last</span><span>p50 <b class="num">${p50}</b></span><span>p95 <b class="num">${p95}</b></span></figcaption></figure>`;
  }
  function lifecycle(t) {
    const f = TD.files[t.file];
    const rows = [].concat(TD.beforeRun.map((x) => ['beforeRun', x, 'config']), f.lifecycle.beforeAll.map((x) => ['beforeAll', x, 'file']), f.lifecycle.beforeEach.map((x) => ['beforeEach', x, 'file']),
      [['test', t.full, 'this']], f.lifecycle.afterAll.map((x) => ['afterAll', x, 'file']), f.lifecycle.cleanups.slice().reverse().map((x) => ['cleanup', x, 'api']));
    return `<ol class="tx-life">${rows.map(([ph, x, src]) => `<li class="${src === 'this' ? 'is-this' : ''}"><span class="ph mono">${ph}</span><span class="ellipsis mono">${esc(x)}</span>${src === 'this' ? '<span class="chip">this test</span>' : chipSrc(src)}</li>`).join('')}</ol>
      <p class="subtle">Cleanups run newest first, after the test and after a failure. A cached beforeAll is reused across runs until it expires.</p>`;
  }
  function apisHtml(t) {
    return `<ul class="tx-apis">${t.apis.map((id) => {
      const a = D.apis[id];
      return a ? `<li><button class="btn btn-ghost btn-sm" data-action="open-api" data-api="${esc(id)}" data-fk="txa-${esc(id)}" title="Open in Client">${method(a.m)}<span class="mono">${esc(id)}</span><span class="subtle mono ellipsis">${esc(a.path)}</span></button></li>`
        : `<li class="subtle mono">${esc(id)}</li>`;
    }).join('')}</ul>`;
  }
  function notes(t) {
    const out = [];
    if (t.skip) out.push(`<p class="callout" data-tone="warn">${ic('skipped')}<span><b>Skipped</b> until ${esc(t.skip.until)} (${esc(VC.untilText(t.skip.until))}): ${esc(t.skip.reason)}. After that date vc check reports V3104 and the test runs again.</span></p>`);
    if (t.allow) out.push(`<p class="callout" data-tone="info">${ic('allow')}<span><b>allowFailure</b> until ${esc(t.allow.until)} (${esc(VC.untilText(t.allow.until))}): ${esc(t.allow.reason)}. A failure is reported but does not fail the run.</span></p>`);
    if (t.st === 'fail') out.push(`<p class="callout" data-tone="danger">${ic('fail')}<span><b>Failed in run 482:</b> body.$.error.code expected <code>card_declined</code>, got <code>"insufficient_funds"</code>. <a href="#tests-run" data-action="tx-open-drill">Open the check</a></span></p>`);
    if (t.st === 'blocked') out.push(`<p class="callout" data-tone="warn">${ic('blocked')}<span><b>Blocked:</b> beforeEach carts#addItem failed (409), so the test did not run.</span></p>`);
    return out.join('');
  }
  function detailHtml() {
    const t = TD.tests[TX.sel] || TD.tests[TD.order[0]];
    const f = TD.files[t.file];
    return `<section class="glass" aria-labelledby="txd-title">
      <div class="card-head"><div class="grow stack-tight">
          <div class="row row-wrap">${badge(t.st)}<h2 class="card-title mono" id="txd-title">${esc(t.full)}</h2></div>
          <div class="card-sub">${esc(t.name)}</div></div>
        <div class="row"><button class="btn btn-primary btn-sm" data-action="tx-run-one" data-fk="txd-run">${ic('play', 'ico-sm')}Run</button>
          <button class="btn btn-sm" data-action="tx-edit" data-fk="txd-edit">${ic('code', 'ico-sm')}Edit</button>
          <button class="btn btn-sm" data-action="tx-edit-dlg" data-kind="quarantine" data-one="1" data-fk="txd-q">${ic('shield', 'ico-sm')}Quarantine</button>
          <button class="btn btn-ghost btn-icon btn-sm" data-action="tx-more" aria-haspopup="menu" aria-label="More actions" data-fk="txd-more">${ic('dots')}</button></div></div>
      <div class="card-tabs tx-meta"><span class="mono">${esc(f.path)}:${t.line}</span><span class="tx-tags">${TD.allTags(t).map((g) => `<span class="chip">${esc(g)}</span>`).join('')}</span>
        ${t.examples ? `<span class="chip">examples ×${t.examples}</span>` : ''}${t.repeat ? `<span class="chip">repeat ${t.repeat}</span>` : ''}</div>
      <div class="card-body" data-sk="txd">${notes(t)}
        <section class="tx-hist" aria-labelledby="txh"><div class="row"><h3 class="section-label" id="txh">Last 30 runs</h3><span class="grow"></span><span class="tx-flake"><b class="num">${t.flake}%</b> flake rate</span></div>
          ${strip(t)}<div class="tx-legend subtle"><span><i class="hc st-pass"></i>passed</span><span><i class="hc st-flaky"></i>flaky</span><span><i class="hc st-fail"></i>failed</span><span><i class="hc st-allow"></i>allowed</span><span><i class="hc st-skipped"></i>skipped or blocked</span><span class="grow"></span><span>oldest → newest</span></div>
          ${spark(t)}</section>
        <section aria-labelledby="txl"><h3 class="section-label" id="txl">Lifecycle it inherits</h3>${lifecycle(t)}</section>
        <section aria-labelledby="txc"><h3 class="section-label" id="txc">Apis it calls</h3>${apisHtml(t)}</section>
      </div></section>`;
  }

  /* ---- screen ------------------------------------------------------------- */
  VC.screens.tests = {
    enter: () => { S.mode = 'tests'; },
    crumbs: () => [D.project.name, 'Tests', 'Explorer'],
    status: () => `${ic('runs', 'ico-sm')} run 482 · 1 failed · 2 flaky`,
    render: () => `${VC.testsNav('tests', VC.runBtn('Run…'))}
      <div class="split tx-split"${VC.splitAttrs()}>${leftHtml()}${VC.divider('Resize list and detail')}${detailHtml()}</div>`,
    mount: (view) => { VC.mountSplit(view, 'txSplit'); const d = $('.tx-split .divider', view); if (d) d.setAttribute('aria-valuenow', S.txSplit); }
  };

  /* ---- actions ------------------------------------------------------------ */
  A['tx-sel'] = (el) => { TX.sel = el.dataset.id; VC.render(); };
  A['tx-facet'] = (el) => { toggleTok(el.dataset.tok); const fromPick = el.closest('#pick'); VC.render(); if (fromPick) openFacet(el.dataset.k, el.dataset.tok); };
  A['tx-clear'] = () => { TX.q = ''; VC.render(); };
  A['tx-uncheck'] = () => { TX.checked.clear(); VC.render(); };
  function openFacet(k, focusTok) {
    const anchor = $(`[data-fk="txf-${k}"]`);
    if (!anchor) return;
    const vals = k === 'service' ? [...new Set(TD.order.map((x) => TD.tests[x].file.split('.')[0]))] : [...new Set(TD.order.flatMap((x) => TD.allTags(TD.tests[x])))].sort();
    const cnt = (v) => TD.order.filter((x) => PRED[k](TD.tests[x], v)).length;
    VC.pick(anchor, `<div class="pop-head"><span class="title">${k === 'service' ? 'Service' : 'Tag'}</span></div><div class="pop-menu" role="group" aria-label="${k}">${vals.map((v) =>
      `<button class="pop-item" role="menuitemcheckbox" aria-checked="${hasTok(k + ':' + v)}" data-action="tx-facet" data-k="${k}" data-tok="${k}:${esc(v)}">${hasTok(k + ':' + v) ? ic('check') : '<span class="leaf-gap"></span>'}<span>${esc(v)}</span><span class="meta">${cnt(v)}</span></button>`).join('')}</div>`);
    const f = focusTok && $(`#pick [data-tok="${CSS.escape(focusTok)}"]`); if (f) f.focus();
  }
  A['tx-facet-pick'] = (el) => openFacet(el.dataset.k);
  A['tx-views'] = (el) => VC.menu(el, `<div class="pop-label">Saved views</div>${TD.savedViews.map((v, i) => VC.menuItem(esc(v.name), 'tx-view', `data-i="${i}"`, { radio: true, checked: TX.q === v.q, meta: `<code>${esc(v.q)}</code>` })).join('')}
    <div class="pop-sep"></div>${VC.menuItem('Save current query as a view…', 'tx-save-view', '', { ico: 'plus', disabled: !TX.q.trim() })}${VC.menuItem('Clear query', 'tx-clear', '', { ico: 'x' })}`);
  A['tx-view'] = (el) => { TX.q = TD.savedViews[+el.dataset.i].q; VC.render(); };
  A['tx-save-view'] = () => { TD.savedViews.push({ name: TX.q, q: TX.q }); VC.toast('View saved', `${TX.q} · kept in .vc/ui.json on this machine`); };
  A['tx-edit'] = () => { VC.edSel && VC.edSel(TX.sel); VC.go('tests-editor'); };
  A['tx-open-drill'] = () => { VC.go('tests-run'); };
  A['tx-run-one'] = () => VC.startRun && VC.startRun({ selector: TX.sel, only: [TX.sel] });
  A['tx-bulk-run'] = () => VC.startRun && VC.startRun({ selector: [...TX.checked].join(' '), only: [...TX.checked] });
  A['tx-more'] = (el) => VC.menu(el, `${VC.menuItem('Copy id', 'tx-copy', `data-t="${esc(TX.sel)}"`, { ico: 'copy' })}${VC.menuItem('Copy vc run command', 'tx-copy', `data-t="vc run ${esc(TX.sel)} --profile ${esc(S.profile)}"`, { ico: 'terminal' })}
    ${VC.menuItem('Run 20 times to check flakiness', 'tx-run-one', '', { ico: 'refresh', meta: '--repeat 20' })}<div class="pop-sep"></div>${VC.menuItem('Show in runs', 'go', 'data-route="runs"', { ico: 'runs' })}`);
  A['tx-copy'] = (el) => VC.copy(el.dataset.t, 'Copied', el.dataset.t);

  /* ---- writes: skip until / tag / quarantine → form → diff → undo ---------- */
  const KIND = {
    skip: { title: 'Skip until a date', verb: 'Skip', ico: 'skipped', key: 'skip', def: '2026-12-01', reason: 'blocked on the payments sandbox, see #340' },
    quarantine: { title: 'Quarantine', verb: 'Quarantine', ico: 'shield', key: 'allowFailure', def: '2026-12-07', reason: 'quarantined: flaky over the last 30 runs' },
    tag: { title: 'Add a tag', verb: 'Tag', ico: 'pin', key: 'tags', def: '', reason: 'needs-triage' }
  };
  let pend = null;
  function targets(one) { return one ? [TX.sel] : [...TX.checked]; }
  A['tx-edit-dlg'] = (el) => {
    const k = KIND[el.dataset.kind], ids = targets(el.dataset.one);
    pend = { kind: el.dataset.kind, ids };
    const body = el.dataset.kind === 'tag'
      ? `<div class="field"><label for="txe-tag">Tag</label><input class="input mono" id="txe-tag" value="${esc(k.reason)}" autofocus><span class="hint">kebab-case (V4 convention). Added to each test's <code>tags:</code>.</span></div>`
      : `<div class="field"><label for="txe-until">Until</label><input class="input mono" type="date" id="txe-until" value="${k.def}" min="${TODAY}" autofocus><span class="hint">After this date vc check reports ${el.dataset.kind === 'skip' ? 'V3104' : 'V4008'} and the test counts again.</span></div>
         <div class="field"><label for="txe-reason">Reason</label><input class="input" id="txe-reason" value="${esc(k.reason)}"><span class="hint">Required. Shown in reports next to the test.</span></div>`;
    VC.dialog(`<div class="dlg dlg-narrow"><div class="dlg-head"><div class="grow"><h2 id="dlg-title">${esc(k.title)}</h2><div class="sub">${ids.length} test${ids.length === 1 ? '' : 's'}: ${esc(ids.map((x) => x.split('#')[1]).join(', '))}</div></div><button class="btn btn-ghost btn-icon" data-action="close-dialog" aria-label="Close">${ic('x')}</button></div>
      <div class="dlg-body">${body}</div>
      <div class="dlg-foot"><span class="note">Nothing is written until you confirm the diff.</span><button class="btn" data-action="close-dialog">Cancel</button><button class="btn btn-primary" data-action="tx-edit-diff">${ic('eye')}Preview diff</button></div></div>`);
  };
  function diffFor(id, kind, until, reason) {
    const t = TD.tests[id];
    const hd = [['@', `@@ -${t.line},3 +${t.line},${kind === 'tag' ? 3 : 6} @@ tests:`], [' ', `  - id: ${t.id}`], [' ', `    name: ${t.name}`]];
    if (kind === 'tag') return hd.concat([['-', `    tags: [${t.tags.join(', ')}]`], ['+', `    tags: [${t.tags.concat(reason).join(', ')}]`]]);
    const key = kind === 'skip' ? 'skip' : 'allowFailure';
    return hd.concat([['+', `    ${key}:`], ['+', `      reason: ${reason}`], ['+', `      until: ${until}`], [' ', `    call: ${t.apis[0]}`]]);
  }
  A['tx-edit-diff'] = () => {
    const kind = pend.kind, k = KIND[kind];
    const until = ($('#txe-until') || {}).value || k.def, reason = (($('#txe-reason') || $('#txe-tag') || {}).value || k.reason).trim();
    const byFile = {};
    pend.ids.forEach((id) => { const f = TD.tests[id].file; (byFile[f] = byFile[f] || []).push(...diffFor(id, kind, until, reason)); });
    const ids = pend.ids;
    VC.diffDialog({
      title: `${k.verb} ${ids.length} test${ids.length === 1 ? '' : 's'}`, sub: kind === 'tag' ? `adds tag ${reason}` : `writes ${k.key} with until: ${until}`, confirm: `Write ${Object.keys(byFile).length} file${Object.keys(byFile).length === 1 ? '' : 's'}`,
      callout: kind === 'quarantine' ? 'A quarantined test still runs; its failure is reported as allowed and does not fail the run. vc check warns when until has passed.' : '',
      files: Object.entries(byFile).map(([f, lines]) => ({ path: TD.files[f].path, lines })),
      after: () => {
        const before = ids.map((id) => [id, JSON.stringify(TD.tests[id])]);
        ids.forEach((id) => {
          const t = TD.tests[id];
          if (kind === 'tag') t.tags = t.tags.concat(reason);
          else if (kind === 'skip') { t.skip = { reason, until }; t.st = 'skipped'; }
          else { t.allow = { reason, until }; t.quarantined = true; t.st = t.st === 'pass' ? 'pass' : 'allow'; }
        });
        TX.checked.clear();
        syncTree();
        dropWroteToast();
        VC.render();
        VC.undoToast(`${k.verb === 'Tag' ? 'Tagged' : k.verb === 'Skip' ? 'Skipped' : 'Quarantined'} ${ids.length} test${ids.length === 1 ? '' : 's'}`, `Wrote ${Object.keys(byFile).map((f) => TD.files[f].path).join(', ')}`,
          () => { before.forEach(([id, js]) => { TD.tests[id] = JSON.parse(js); }); syncTree(); });
      }
    });
  };
  function syncTree() {
    D.testsTree.forEach((s) => s.files.forEach((f) => { if (!f.load) f.tests = TD.order.filter((k) => TD.tests[k].file === f.id).map((k) => [TD.tests[k].id, TD.tests[k].st]); }));
  }

  /* ---- input: query box and checkboxes ------------------------------------ */
  document.addEventListener('input', (e) => { if (e.target.id === 'tx-q') { TX.q = e.target.value; refreshList(); } });
  document.addEventListener('change', (e) => {
    const c = e.target;
    if (c.classList.contains('tx-check')) { if (c.checked) TX.checked.add(c.dataset.id); else TX.checked.delete(c.dataset.id); refreshList(); const f = $(`[data-fk="${CSS.escape(c.dataset.fk)}"]`); if (f) f.focus(); }
    else if (c.classList.contains('tx-check-all')) { visible().forEach((t) => (c.checked ? TX.checked.add(t.full) : TX.checked.delete(t.full))); refreshList(); const f = $('[data-fk="tx-all"]'); if (f) f.focus(); }
  });
})();
