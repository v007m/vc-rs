/* Tests space · the read view (#tests). The sidebar (screens/tests-sidebar.js)
   chooses what it shows: VC.tx.view is
     'table'  All tests: every api and load test the sidebar's filter lets through,
              with bulk actions on the checked rows
     'file'   one test file: its lifecycle and its tests
     'test'   one test, full width: notes, a check run, the last 30 runs, details
   Run on a card is a check run: it verifies the test inline and is not saved to
   Runs (VC.tx.check). "Run in Runs…" in the ⋯ starts a real run (VC.startRun).
   Edit opens #tests-editor for that test.
   Also owns what the Tests screens share: VC.runBtn, VC.undoToast, VC.untilText,
   VC.dropWroteToast. */
(function () {
  'use strict';
  const VC = window.VC, D = VC.data, S = VC.state, A = VC.actions, TD = VC.testsData;
  const { $, esc, ic, badge, method, chipSrc } = VC.h;
  const TODAY = '2026-11-23';
  const TX = VC.tx, Q = VC.txq;
  const reduced = () => matchMedia('(prefers-reduced-motion: reduce)').matches || document.documentElement.dataset.motion === 'reduced';

  /* ---- shared: run button, undo toast, day maths ---------------------------- */
  VC.runBtn = (label) => `<button class="btn btn-primary btn-sm" data-action="tx-run-dialog" data-fk="tx-runbtn">${ic('play')}${esc(label || 'Run')}<span class="kbd hide-narrow">⌘R</span></button>`;
  VC.undoToast = function (msg, sub, fn) {
    VC.toast(msg, sub, '', { label: 'Undo', ico: 'arrow-left', action: () => { fn(); VC.render(); VC.toast('Undone', 'The files are back as they were; vc check · 0 errors'); } });
  };
  VC.dropWroteToast = () => { const t = $('#toasts').lastElementChild; if (t && !t.classList.contains('has-action')) t.remove(); };
  const days = (d) => Math.round((Date.parse(d) - Date.parse(TODAY)) / 864e5);
  VC.untilText = (d) => { const n = days(d); return n < 0 ? `expired ${-n} d ago` : n === 0 ? 'expires today' : `expires in ${n} d`; };
  const plural = (n, w) => `${n} ${w}${n === 1 ? '' : 's'}`;

  /* ---- All tests: the table ------------------------------------------------- */
  function flagHtml(t) {
    return t.quarantined ? '<span class="chip">quarantined</span>' : t.skip ? `<span class="chip" title="skip · ${esc(VC.untilText(t.skip.until))}">skipped</span>` : t.allow ? '<span class="chip">allowFailure</span>' : '';
  }
  function rowHtml(t) {
    const on = TX.checked.has(t.full);
    return `<tr class="${on ? 'is-checked' : ''}">
      <td class="tx-ck"><input type="checkbox" class="tx-check" data-id="${esc(t.full)}" ${on ? 'checked' : ''} aria-label="Select ${esc(t.id)}" data-fk="txc-${esc(t.full)}"></td>
      <td class="tx-name"><button class="tx-open" data-action="tx-sel" data-id="${esc(t.full)}" data-fk="txr-${esc(t.full)}"><span class="dot st-${t.st}" role="img" aria-label="${t.st}"></span><span class="mono ellipsis">${esc(t.id)}</span></button></td>
      <td class="tx-desc hide-narrow"><span class="ellipsis">${esc(t.name)}</span></td>
      <td class="tx-flag">${flagHtml(t)}</td>
      <td class="r mono muted">${t.ms ? t.ms + ' ms' : '—'}</td>
      <td class="r mono ${t.flake >= 10 ? 'tx-hot' : 'muted'}"${t.flake >= 10 ? ' title="Flaky in 10% or more of the last 30 runs"' : ''}>${t.flake}%</td></tr>`;
  }
  function loadRowHtml(l) {
    return `<tr><td class="tx-ck"></td>
      <td class="tx-name"><button class="tx-open" data-action="tx-load" data-id="${esc(l.id)}" data-fk="txl-${esc(l.id)}"><span class="dot st-${l.st}" role="img" aria-label="${l.st}"></span><span class="mono ellipsis">${esc(l.id)}</span></button></td>
      <td class="tx-desc hide-narrow"><span class="ellipsis">${esc(l.name)}</span></td>
      <td class="tx-flag"><span class="chip ts-load-chip">load</span></td>
      <td class="r mono muted" colspan="2">${esc(l.last)}</td></tr>`;
  }
  const fileHead = (f, n) => `<tr class="tx-file"><th colspan="6" scope="rowgroup"><button class="tx-file-open" data-action="tx-file" data-file="${esc(f)}" data-fk="txf-${esc(f)}" title="${esc(TD.files[f].path)} · open the file">${ic('file', 'ico-sm')}<span class="mono">${esc(f)}</span><span class="subtle hide-narrow">${esc(TD.files[f].name)}</span>${TD.files[f].changed ? '<span class="chip">changed</span>' : ''}<span class="grow"></span><span class="subtle num">${n}</span></button></th></tr>`;
  function tableBody() {
    const vis = Q.visible(), loads = Q.visibleLoad();
    const { bad } = Q.parse(TX.q);
    const byFile = Object.keys(TD.files).map((f) => [f, vis.filter((t) => t.file === f)]).filter(([, ts]) => ts.length);
    const all = vis.length && vis.every((t) => TX.checked.has(t.full));
    const warn = bad.length ? `<p class="tx-bad" role="status">${ic('warn', 'ico-sm')}Unknown key ${esc(bad.join(', '))}. Keys are tag, status, api, service, file, changed.</p>` : '';
    if (!vis.length && !loads.length) return warn + `<div class="empty empty-quiet"><span>No tests match. Try <code>status:failed</code> or <code>tag:smoke OR api:payments</code>.</span><button class="link" data-action="ts-clear" data-fk="tx-clear">Clear filters</button></div>`;
    return warn + `<table class="table tx-table" aria-label="Tests">
      <thead><tr><th class="tx-ck">${vis.length ? `<input type="checkbox" class="tx-check-all" ${all ? 'checked' : ''} aria-label="Select all ${vis.length} visible api tests" data-fk="tx-all">` : ''}</th><th>Test</th><th class="hide-narrow">Name</th><th><span class="sr-only">Flag</span></th><th class="r">Last</th><th class="r" title="Share of the last 30 runs that failed, flaked or were allowed to fail">Flake</th></tr></thead>
      ${byFile.map(([f, ts]) => `<tbody>${fileHead(f, ts.length)}${ts.map(rowHtml).join('')}</tbody>`).join('')}
      ${loads.length ? `<tbody><tr class="tx-file"><th colspan="6" scope="rowgroup"><span class="tx-file-open">${ic('chart', 'ico-sm')}<span>Load tests</span><span class="grow"></span><span class="subtle num">${loads.length}</span></span></th></tr>${loads.map(loadRowHtml).join('')}</tbody>` : ''}
    </table>`;
  }
  function bulkHtml() {
    const n = TX.checked.size;
    if (!n) return '';
    return `<div class="tx-bulk l2" role="toolbar" aria-label="${n} selected">
      <b>${n} selected</b><span class="grow"></span>
      <button class="btn btn-sm btn-primary" data-action="tx-bulk-run" data-fk="txb-run">${ic('play', 'ico-sm')}Run in Runs</button>
      <button class="btn btn-sm" data-action="tx-edit-dlg" data-kind="tag" data-fk="txb-tag">${ic('pin', 'ico-sm')}Tag…</button>
      ${VC.moreBtn('tx-bulk-more', 'More actions for the selection', 'txb-more')}</div>`;
  }
  const countText = () => { const a = Q.visible().length, l = Q.visibleLoad().length; return `${plural(a, 'api test')} · ${plural(l, 'load test')}`; };
  function tableHtml() {
    return `<section class="glass tx-card" aria-labelledby="tx-title">
      <div class="card-head"><h2 class="card-title" id="tx-title">All tests</h2><span class="subtle tx-count" id="tx-count" aria-live="polite">${countText()}</span><span class="grow"></span>
        <div class="input-group tx-search">${ic('search')}<input class="input input-sm mono" id="tx-q" value="${esc(TX.q)}" placeholder="Search or tag:smoke -tag:slow" aria-label="Search and filter tests" aria-describedby="tx-q-help" data-fk="tx-q" autocomplete="off" spellcheck="false"></div>
        <span id="tx-q-help" class="sr-only">Keys: tag, status, api, service, file, changed. OR joins groups; a leading minus excludes.</span></div>
      <div class="card-body tx-tablebody" id="tx-list" data-sk="tx-list">${tableBody()}</div>
      <div id="tx-bulk">${bulkHtml()}</div>
    </section>`;
  }
  function refreshTable() {
    const l = $('#tx-list'); if (l) l.innerHTML = tableBody();
    const b = $('#tx-bulk'); if (b) b.innerHTML = bulkHtml();
    const c = $('#tx-count'); if (c) c.textContent = countText();
    const sb = $('#sidebar .sb-body'); if (sb) sb.innerHTML = VC.spaces.tests.sidebar();
  }

  /* ---- one test ------------------------------------------------------------- */
  const SLAB = { pass: 'passed', fail: 'failed', flaky: 'flaky', allow: 'allowed failure', blocked: 'blocked', skipped: 'skipped' };
  function strip(t) {
    const n = (s) => t.history.filter((x) => x === s).length;
    const parts = [['pass', 'passed'], ['flaky', 'flaky'], ['fail', 'failed'], ['allow', 'allowed'], ['blocked', 'blocked'], ['skipped', 'skipped']].filter(([s]) => n(s)).map(([s, l]) => `${n(s)} ${l}`);
    return `<ol class="tx-strip" aria-label="Last 30 runs, oldest first: ${parts.join(', ')}">${t.history.map((s, i) => `<li class="hc st-${s}" title="run ${453 + i} · ${SLAB[s] || s}${t.durs[i] ? ' · ' + t.durs[i] + ' ms' : ''}${i === 0 ? ' · oldest' : i === t.history.length - 1 ? ' · newest' : ''}"></li>`).join('')}</ol>`;
  }
  /* the sparkline is drawn at the figure's measured width (mount), so it fills the card */
  function sparkSvg(t, W) {
    const H = 104, P = 6;
    const ds = t.durs.map((d, i) => [i, d]).filter(([, d]) => d != null);
    const max = Math.max(...ds.map((d) => d[1])) * 1.1, min = Math.min(...ds.map((d) => d[1])) * 0.85;
    const x = (i) => P + (i / 29) * (W - 2 * P), y = (v) => H - P - ((v - min) / (max - min)) * (H - 2 * P);
    const sorted = ds.map((d) => d[1]).sort((a, b) => a - b), p50 = sorted[Math.floor(sorted.length / 2)];
    const path = ds.map(([i, d], k) => `${k ? 'L' : 'M'}${x(i).toFixed(1)},${y(d).toFixed(1)}`).join('');
    const last = ds[ds.length - 1], hw = Math.max(4, (W - 2 * P) / 58);
    return `<line class="ref" x1="${P}" x2="${W - P}" y1="${y(p50).toFixed(1)}" y2="${y(p50).toFixed(1)}"/><path class="ln" d="${path}"/>
      <path class="pt-ring" d="M${x(last[0]).toFixed(1)},${y(last[1]).toFixed(1)}h0"/><path class="pt" d="M${x(last[0]).toFixed(1)},${y(last[1]).toFixed(1)}h0"/>
      ${ds.map(([i, d]) => `<rect class="hit" x="${(x(i) - hw).toFixed(1)}" y="0" width="${(2 * hw).toFixed(1)}" height="${H}"><title>run ${453 + i} · ${d} ms</title></rect>`).join('')}`;
  }
  function spark(t) {
    const ds = t.durs.filter((d) => d != null);
    if (ds.length < 2) return '<p class="subtle">No timings: the test has not run lately.</p>';
    const sorted = ds.slice().sort((a, b) => a - b), p50 = sorted[Math.floor(sorted.length / 2)], p95 = sorted[Math.floor(sorted.length * 0.95) - 1] || sorted[sorted.length - 1];
    const last = ds[ds.length - 1];
    return `<figure class="tx-spark"><svg viewBox="0 0 600 104" preserveAspectRatio="none" role="img" aria-label="Duration over the last 30 runs: median ${p50} ms, p95 ${p95} ms, last ${last} ms">${sparkSvg(t, 600)}</svg>
      <figcaption><span><b class="num">${last}</b> ms last</span><span>p50 <b class="num">${p50}</b></span><span>p95 <b class="num">${p95}</b></span></figcaption></figure>`;
  }
  function lifeRows(f, t) {
    return [].concat(TD.beforeRun.map((x) => ['beforeRun', x, 'config']), f.lifecycle.beforeAll.map((x) => ['beforeAll', x, 'file']), f.lifecycle.beforeEach.map((x) => ['beforeEach', x, 'file']),
      t ? [['test', t.full, 'this']] : [], f.lifecycle.afterAll.map((x) => ['afterAll', x, 'file']), f.lifecycle.cleanups.slice().reverse().map((x) => ['cleanup', x, 'api']));
  }
  function lifecycle(f, t) {
    return `<ol class="tx-life">${lifeRows(f, t).map(([ph, x, src]) => `<li class="${src === 'this' ? 'is-this' : ''}"><span class="ph mono">${ph}</span><span class="ellipsis mono">${esc(x)}</span>${src === 'this' ? '<span class="chip">this test</span>' : chipSrc(src)}</li>`).join('')}</ol>
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
    if (t.st === 'fail') out.push(`<p class="callout" data-tone="danger">${ic('fail')}<span><b>Failed in run 482:</b> body.$.error.code expected <code>card_declined</code>, got <code>"insufficient_funds"</code>. <button class="link" data-action="tx-open-drill" data-fk="txd-drill">Open the run</button></span></p>`);
    if (t.st === 'blocked') out.push(`<p class="callout" data-tone="warn">${ic('blocked')}<span><b>Blocked:</b> beforeEach carts#addItem failed (409), so the test did not run.</span></p>`);
    return out.join('');
  }
  function sourceHtml(t, f) {
    const tags = TD.allTags(t);
    const rows = [['file', `<span class="mono ellipsis" title="${esc(t.full)}">${esc(f.path)}:${t.line}</span>`], ['tags', tags.length ? `<span class="tx-tags">${tags.map((g) => `<span class="chip">${esc(g)}</span>`).join('')}</span>` : '<span class="subtle">none</span>']];
    if (t.examples) rows.push(['examples', `<span class="num">${t.examples} rows</span>`]);
    if (t.repeat) rows.push(['repeat', `<span class="num">${t.repeat}</span>`]);
    return `<div class="kv is-readonly" role="table" aria-label="Source">${rows.map(([k, v]) => `<div class="kv-row" role="row"><span class="k" role="cell">${k}</span><span class="v" role="cell">${v}</span><span class="s" role="cell"></span></div>`).join('')}</div>`;
  }
  const disc = (id, label, meta, body, open) => `<details class="disc" data-fk="${id}"${open ? ' open' : ''}><summary>${ic('chev-down', 'chev')}${label}${meta ? `<span class="meta">${meta}</span>` : ''}</summary><div class="disc-body">${body}</div></details>`;
  const lifeCount = (f) => TD.beforeRun.length + f.lifecycle.beforeAll.length + f.lifecycle.beforeEach.length + f.lifecycle.afterAll.length + f.lifecycle.cleanups.length;
  const cardActs = (pre, runLabel) => `<div class="row tx-acts"><button class="btn btn-primary btn-sm" data-action="tx-check" data-fk="${pre}-run" title="Check run: verifies it now, not saved to Runs">${ic('play', 'ico-sm')}${runLabel}</button>
      <button class="btn btn-sm" data-action="tx-edit" data-fk="${pre}-edit">${ic('pencil', 'ico-sm')}Edit</button>
      ${VC.moreBtn(pre === 'txd' ? 'tx-more' : 'tx-file-more', pre === 'txd' ? 'More test actions' : 'More file actions', pre + '-more')}</div>`;
  function testHtml(t) {
    const f = TD.files[t.file];
    return `<section class="glass tx-card" aria-labelledby="txd-title">
      <div class="card-head"><div class="grow stack-tight">
          <div class="row">${badge(t.st)}<h2 class="card-title mono ellipsis" id="txd-title" title="${esc(t.full)}">${esc(t.id)}</h2></div>
          <div class="card-sub"><span class="ellipsis">${esc(t.name)}</span><span aria-hidden="true">·</span><button class="link mono" data-action="tx-file" data-file="${esc(t.file)}" data-fk="txd-file" title="Open the file">${esc(t.file)}</button><span class="mono subtle">:${t.line}</span></div></div>
        ${cardActs('txd', 'Run')}</div>
      <div class="card-body tx-detail" data-sk="txd">${notes(t)}
        <div id="tx-res" aria-live="polite">${VC.txCheckHtml()}</div>
        <section class="tx-hist" aria-labelledby="txh"><div class="row"><h3 class="section-label" id="txh">Last 30 runs</h3><span class="grow"></span><span class="tx-flake"><b class="num">${t.flake}%</b> flake rate</span></div>
          ${strip(t)}${spark(t)}</section>
        <div class="tx-discs">${disc('txd-life', 'Lifecycle it inherits', `${lifeCount(f)} steps`, lifecycle(f, t), true)}${disc('txd-apis', 'Apis it calls', String(t.apis.length), apisHtml(t), true)}${disc('txd-src', 'Source and tags', '', sourceHtml(t, f), true)}</div>
      </div></section>`;
  }

  /* ---- one file ------------------------------------------------------------- */
  function fileHtml(id) {
    const f = TD.files[id];
    const ts = TD.order.map((k) => TD.tests[k]).filter((t) => t.file === id);
    const bad = ts.filter((t) => t.st === 'fail' || t.st === 'blocked').length;
    return `<section class="glass tx-card" aria-labelledby="txf-title">
      <div class="card-head"><div class="grow stack-tight">
          <div class="row">${ic('file')}<h2 class="card-title mono ellipsis" id="txf-title">${esc(id)}</h2>${f.changed ? '<span class="chip">changed</span>' : ''}</div>
          <div class="card-sub"><span>${esc(f.name)}</span><span aria-hidden="true">·</span><span class="mono ellipsis">${esc(f.path)}</span></div></div>
        ${cardActs('txfv', 'Run file')}</div>
      <div class="card-body tx-detail" data-sk="txfv">
        <div id="tx-res" aria-live="polite">${VC.txCheckHtml()}</div>
        <div class="tx-cols">
          <section aria-labelledby="txfv-t"><div class="row"><h3 class="section-label" id="txfv-t">Tests</h3><span class="grow"></span><span class="subtle">${plural(ts.length, 'test')}${bad ? ` · ${bad} need attention` : ''}</span></div>
            <ul class="tx-flist">${ts.map((t) => `<li><button class="tx-open" data-action="tx-sel" data-id="${esc(t.full)}" data-fk="txfv-${esc(t.full)}"><span class="dot st-${t.st}" role="img" aria-label="${t.st}"></span><span class="mono ellipsis">${esc(t.id)}</span><span class="subtle ellipsis grow">${esc(t.name)}</span>${flagHtml(t)}<span class="mono muted">${t.ms ? t.ms + ' ms' : '—'}</span></button></li>`).join('')}</ul></section>
          <section aria-labelledby="txfv-l"><div class="row"><h3 class="section-label" id="txfv-l">Lifecycle</h3><span class="grow"></span><span class="subtle">${lifeCount(f)} steps</span></div>${lifecycle(f, null)}</section>
        </div></div></section>`;
  }

  /* ---- screen --------------------------------------------------------------- */
  function measureSpark(view) {
    const fig = $('.tx-spark', view), t = TD.tests[TX.sel];
    if (!fig || !t) return;
    const w = Math.round(fig.clientWidth);
    const svg = fig.querySelector('svg');
    if (w > 40 && svg && svg.getAttribute('viewBox') !== `0 0 ${w} 104`) { svg.setAttribute('viewBox', `0 0 ${w} 104`); svg.innerHTML = sparkSvg(t, w); }
  }
  let ro = null;
  VC.screens.tests = {
    space: 'tests',
    enter: () => { if (TX.view === 'test' && !TD.tests[TX.sel]) TX.view = 'table'; if (TX.view === 'file' && !TD.files[TX.file]) TX.view = 'table'; },
    crumbs: () => [D.project.name, 'Tests', TX.view === 'test' ? TX.sel : TX.view === 'file' ? TX.file : 'All tests'],
    status: () => (TX.view === 'test' ? `last run ${esc(TD.tests[TX.sel].lastRun)}` : `${ic('runs', 'ico-sm')} run 482 · 1 failed`),
    primary: () => '',
    render: () => `<div class="stage-one tx-one${TX.view === 'file' ? ' is-hug' : ''}">${TX.view === 'test' ? testHtml(TD.tests[TX.sel]) : TX.view === 'file' ? fileHtml(TX.file) : tableHtml()}</div>`,
    mount: (view) => {
      measureSpark(view);
      const fig = $('.tx-spark', view);
      if (fig && window.ResizeObserver) { ro = new ResizeObserver(() => measureSpark(view)); ro.observe(fig); }
    },
    unmount: () => { if (ro) ro.disconnect(); ro = null; }
  };

  /* ---- actions -------------------------------------------------------------- */
  A['tx-sel'] = (el) => VC.txOpen(el.dataset.id);
  A['tx-file'] = (el) => VC.txFile(el.dataset.file);
  A['tx-load'] = (el) => (VC.ldOpen ? VC.ldOpen(el.dataset.id) : VC.go('tests-load'));
  A['tx-uncheck'] = () => { TX.checked.clear(); VC.render(); };
  const curTests = () => (TX.view === 'file' ? TD.order.filter((k) => TD.tests[k].file === TX.file) : [TX.sel]);
  A['tx-edit'] = () => { if (VC.edSel) VC.edSel(curTests()[0]); VC.go('tests-editor'); };
  A['tx-open-drill'] = () => VC.go('runs-live');
  const runInRuns = (ids, extra) => (VC.startRun ? VC.startRun(Object.assign({ selector: TX.view === 'file' && ids.length > 1 ? TX.file : ids.join(' '), only: ids }, extra || {})) : VC.go('runs-live'));
  A['tx-run-runs'] = () => runInRuns(curTests());
  A['tx-run-repeat'] = () => runInRuns(curTests(), { repeat: 20 });
  A['tx-bulk-run'] = () => runInRuns([...TX.checked]);
  A['tx-bulk-more'] = (el) => VC.menu(el, `${VC.menuItem('Skip until…', 'tx-edit-dlg', 'data-kind="skip"', { ico: 'skipped' })}${VC.menuItem('Quarantine…', 'tx-edit-dlg', 'data-kind="quarantine"', { ico: 'shield' })}<div class="pop-sep"></div>${VC.menuItem('Clear selection', 'tx-uncheck', '', { ico: 'x' })}`);
  A['tx-more'] = (el) => VC.menu(el, `${VC.menuItem('Run in Runs…', 'tx-run-runs', '', { ico: 'runs', meta: 'saved as a run' })}${VC.menuItem('Run 20 times to check flakiness', 'tx-run-repeat', '', { ico: 'refresh', meta: '--repeat 20' })}
    <div class="pop-sep"></div>${VC.menuItem('Quarantine…', 'tx-edit-dlg', 'data-kind="quarantine" data-one="1"', { ico: 'shield' })}${VC.menuItem('Skip until…', 'tx-edit-dlg', 'data-kind="skip" data-one="1"', { ico: 'skipped' })}
    <div class="pop-sep"></div>${VC.menuItem('Copy id', 'tx-copy', `data-t="${esc(TX.sel)}"`, { ico: 'copy' })}${VC.menuItem('Copy vc run command', 'tx-copy', `data-t="vc run ${esc(TX.sel)} --profile ${esc(S.profile)}"`, { ico: 'terminal' })}${VC.menuItem('Show in runs', 'go', 'data-route="runs"', { ico: 'history' })}`);
  A['tx-file-more'] = (el) => VC.menu(el, `${VC.menuItem('Run in Runs…', 'tx-run-runs', '', { ico: 'runs', meta: 'saved as a run' })}<div class="pop-sep"></div>${VC.menuItem('Copy path', 'tx-copy', `data-t="${esc(TD.files[TX.file].path)}"`, { ico: 'copy' })}${VC.menuItem('Copy vc run command', 'tx-copy', `data-t="vc run ${esc(TX.file)} --profile ${esc(S.profile)}"`, { ico: 'terminal' })}`);
  A['tx-copy'] = (el) => VC.copy(el.dataset.t, 'Copied', el.dataset.t);

  /* ---- check run: Run on a card verifies inline; nothing is saved to Runs ---- */
  const NOTE = `<span class="tx-res-note">${ic('info', 'ico-sm')}Check run, not saved to Runs</span>`;
  function checkOne(id, r, solo) {
    const t = TD.tests[id];
    const checks = `<ul class="tx-res-checks" aria-label="Checks">${r.checks.map((c) => `<li class="${c.st === 'fail' ? 'is-fail' : ''}">${ic(c.st === 'fail' ? 'x' : 'check', 'ico-sm')}<span class="mono">${esc(c.what)}</span><span class="mono subtle">expected ${esc(c.exp)}</span><span class="mono">got ${esc(c.got)}</span></li>`).join('')}</ul>`;
    const peek = `<pre class="code tx-res-peek" aria-label="Response"><span class="subtle">${esc(r.req)}</span>\n<b>${r.status}</b>${r.body.length ? '\n' + esc(r.body.join('\n')) : ''}</pre>`;
    if (!solo) return `<li class="tx-res-row"><span class="dot st-${r.st}" role="img" aria-label="${r.st}"></span><span class="mono ellipsis">${esc(t.id)}</span><span class="grow"></span><span class="mono muted">${r.ms} ms</span></li>`;
    return `<div class="tx-res-grid">${checks}${peek}</div>`;
  }
  VC.txCheckHtml = function () {
    const c = TX.check, key = TX.view === 'file' ? TX.file : TX.sel;
    if (!c || c.key !== key) return '';
    const head = (inner) => `<div class="tx-res-head">${inner}<span class="grow"></span>${NOTE}<button class="btn btn-ghost btn-icon btn-sm" data-action="tx-check-x" aria-label="Dismiss the check result" data-fk="tx-check-x">${ic('x')}</button></div>`;
    if (!c.done) return `<section class="tx-res l2" aria-label="Check run">${head(`${badge('running', 'Checking')}<span class="subtle">${plural(c.ids.length, 'test')} on ${esc(S.profile)}</span>`)}</section>`;
    const bad = c.results.filter(([, r]) => r.st !== 'pass').length, ms = c.results.reduce((s, [, r]) => s + r.ms, 0);
    const st = c.results.length === 1 ? c.results[0][1].st : bad ? 'fail' : 'pass';
    const sum = c.results.length === 1 ? '' : `<span class="subtle">${c.results.length - bad} of ${c.results.length} passed</span>`;
    const body = c.results.length === 1 ? checkOne(c.results[0][0], c.results[0][1], true) : `<ul class="tx-res-list">${c.results.map(([id, r]) => checkOne(id, r, false)).join('')}</ul>`;
    return `<section class="tx-res l2" aria-label="Check run result">${head(`${badge(st)}<span class="mono num">${ms} ms</span>${sum}`)}${body}</section>`;
  };
  A['tx-check'] = () => {
    const ids = curTests();
    const key = TX.view === 'file' ? TX.file : TX.sel;
    TX.check = { key, ids, done: false, results: null };
    const paint = () => { const r = $('#tx-res'); if (r) r.innerHTML = VC.txCheckHtml(); };
    paint();
    setTimeout(() => {
      if (!TX.check || TX.check.key !== key) return;
      TX.check.results = ids.map((id) => [id, TD.checkRun(id)]);
      TX.check.done = true;
      paint();
    }, reduced() ? 0 : 700);
  };
  A['tx-check-x'] = () => { TX.check = null; const r = $('#tx-res'); if (r) r.innerHTML = ''; const b = $('[data-fk$="-run"]'); if (b) b.focus(); };

  /* ---- writes: skip until / tag / quarantine → form → diff → undo ---------- */
  const KIND = {
    skip: { title: 'Skip until a date', verb: 'Skip', done: 'Skipped', key: 'skip', def: '2026-12-01', reason: 'blocked on the payments sandbox, see #340' },
    quarantine: { title: 'Quarantine', verb: 'Quarantine', done: 'Quarantined', key: 'allowFailure', def: '2026-12-07', reason: 'quarantined: flaky over the last 30 runs' },
    tag: { title: 'Add a tag', verb: 'Tag', done: 'Tagged', key: 'tags', def: '', reason: 'needs-triage' }
  };
  let pend = null;
  A['tx-edit-dlg'] = (el) => {
    const k = KIND[el.dataset.kind], ids = el.dataset.one ? [TX.sel] : [...TX.checked];
    pend = { kind: el.dataset.kind, ids };
    const body = el.dataset.kind === 'tag'
      ? `<div class="field"><label for="txe-tag">Tag</label><input class="input mono" id="txe-tag" value="${esc(k.reason)}" autofocus><span class="hint">kebab-case (V4 convention). Added to each test's <code>tags:</code>.</span></div>`
      : `<div class="field"><label for="txe-until">Until</label><input class="input mono" type="date" id="txe-until" value="${k.def}" min="${TODAY}" autofocus><span class="hint">After this date vc check reports ${el.dataset.kind === 'skip' ? 'V3104' : 'V4008'} and the test counts again.</span></div>
         <div class="field"><label for="txe-reason">Reason</label><input class="input" id="txe-reason" value="${esc(k.reason)}"><span class="hint">Required. Shown in reports next to the test.</span></div>`;
    VC.dialog(`<div class="dlg dlg-narrow"><div class="dlg-head"><div class="grow"><h2 id="dlg-title">${esc(k.title)}</h2><div class="sub">${plural(ids.length, 'test')}: ${esc(ids.map((x) => x.split('#')[1]).join(', '))}</div></div><button class="btn btn-ghost btn-icon" data-action="close-dialog" aria-label="Close">${ic('x')}</button></div>
      <div class="dlg-body">${body}</div>
      <div class="dlg-foot"><span class="note">Nothing is written until you confirm the diff.</span><button class="btn" data-action="close-dialog">Cancel</button><button class="btn btn-primary" data-action="tx-edit-diff">${ic('eye')}Preview diff</button></div></div>`);
  };
  function diffFor(id, kind, until, reason) {
    const t = TD.tests[id];
    const hd = [['@', `@@ -${t.line},3 +${t.line},${kind === 'tag' ? 3 : 6} @@ tests:`], [' ', `  - id: ${t.id}`], [' ', `    name: ${t.name}`]];
    if (kind === 'tag') return hd.concat([['-', `    tags: [${t.tags.join(', ')}]`], ['+', `    tags: [${t.tags.concat(reason).join(', ')}]`]]);
    return hd.concat([['+', `    ${KIND[kind].key}:`], ['+', `      reason: ${reason}`], ['+', `      until: ${until}`], [' ', `    call: ${t.apis[0]}`]]);
  }
  function apply(ids, kind, until, reason) {
    ids.forEach((id) => {
      const t = TD.tests[id];
      if (kind === 'tag') t.tags = t.tags.concat(reason);
      else if (kind === 'skip') { t.skip = { reason, until }; t.st = 'skipped'; }
      else { t.allow = { reason, until }; t.quarantined = true; t.st = t.st === 'pass' ? 'pass' : 'allow'; }
    });
  }
  const syncTree = () => D.testsTree.forEach((s) => s.files.forEach((f) => { if (!f.load) f.tests = TD.order.filter((k) => TD.tests[k].file === f.id).map((k) => [TD.tests[k].id, TD.tests[k].st]); }));
  A['tx-edit-diff'] = () => {
    const { kind, ids } = pend, k = KIND[kind];
    const until = ($('#txe-until') || {}).value || k.def, reason = (($('#txe-reason') || $('#txe-tag') || {}).value || k.reason).trim();
    const byFile = {};
    ids.forEach((id) => { const f = TD.tests[id].file; (byFile[f] = byFile[f] || []).push(...diffFor(id, kind, until, reason)); });
    const nf = Object.keys(byFile).length;
    VC.diffDialog({
      title: `${k.verb} ${plural(ids.length, 'test')}`, sub: kind === 'tag' ? `adds tag ${reason}` : `writes ${k.key} with until: ${until}`, confirm: `Write ${plural(nf, 'file')}`,
      callout: kind === 'quarantine' ? 'A quarantined test still runs; its failure is reported as allowed and does not fail the run. vc check warns when until has passed.' : '',
      files: Object.entries(byFile).map(([f, lines]) => ({ path: TD.files[f].path, lines })),
      after: () => {
        const before = ids.map((id) => [id, JSON.stringify(TD.tests[id])]);
        apply(ids, kind, until, reason);
        TX.checked.clear();
        syncTree();
        VC.dropWroteToast();
        VC.render();
        VC.undoToast(`${k.done} ${plural(ids.length, 'test')}`, `Wrote ${Object.keys(byFile).map((f) => TD.files[f].path).join(', ')}`,
          () => { before.forEach(([id, js]) => { TD.tests[id] = JSON.parse(js); }); syncTree(); });
      }
    });
  };

  /* ---- input: search box and checkboxes ------------------------------------- */
  document.addEventListener('input', (e) => { if (e.target.id === 'tx-q') { TX.q = e.target.value; refreshTable(); } });
  document.addEventListener('change', (e) => {
    const c = e.target;
    if (c.classList.contains('tx-check')) { if (c.checked) TX.checked.add(c.dataset.id); else TX.checked.delete(c.dataset.id); }
    else if (c.classList.contains('tx-check-all')) Q.visible().forEach((t) => (c.checked ? TX.checked.add(t.full) : TX.checked.delete(t.full)));
    else return;
    refreshTable();
    const f = $(`[data-fk="${CSS.escape(c.dataset.fk)}"]`); if (f) f.focus();
  });
})();
