/* Tests mode · Runner (#tests-run): the Run dialog (⌘R in Tests mode), the live run
   (progress, now running, counters, event log with filter and pause), drill-down
   (checks with expected/actual/span, exchanges, body diff), Stop with live cleanups,
   and the after-run actions. Timers drive the run; under reduced motion it steps on click. */
(function () {
  'use strict';
  const VC = window.VC, D = VC.data, S = VC.state, A = VC.actions, TD = VC.testsData;
  const { $, $$, esc, ic, badge, method, chipSrc } = VC.h;

  const ST = ['pass', 'fail', 'blocked', 'skipped', 'flaky', 'allow'];
  const LABEL = { pass: 'passed', fail: 'failed', blocked: 'blocked', skipped: 'skipped', flaky: 'flaky', allow: 'allowed' };
  const CLEANUPS = ['carts#delete c_91x', 'carts#delete c_88a', 'users#delete u_3kd8', 'users#delete u_2p1q', 'orders#cancel o_7f1', 'users#delete u_8f2c41'];
  const R = (VC.run = {
    phase: 'done', n: 482, done: TD.run.instances.length, results: TD.run.instances.slice(), events: [], open: {}, paused: false, buffered: 0, filter: 'all',
    drill: 'orders.checkout#rejectsDeclinedCard', tab: 'checks', clean: 0, timer: null, t0: 0, ms: 4630,
    opts: { selector: TD.run.selector, profile: 'local', layers: [], seed: 42, repeat: 1, bail: false, concurrency: 4, strict: false, reporters: ['pretty', 'html'], since: 'origin/master' }
  });
  if (S.trSplit == null) S.trSplit = 50;
  const reduced = () => S.motion === 'reduce' || (S.motion === 'system' && matchMedia('(prefers-reduced-motion: reduce)').matches);
  const counts = () => { const c = {}; ST.forEach((s) => { c[s] = R.results.filter((r) => r.st === s).length; }); return c; };
  const total = () => R.plan.length;
  const clock = (ms) => `${String(Math.floor(ms / 60000)).padStart(2, '0')}:${(ms / 1000 % 60).toFixed(1).padStart(4, '0')}`;
  const cmd = () => { const o = R.opts; return `vc run ${o.selector.includes(' OR ') ? `-t smoke --changed ${o.since}` : o.selector}${o.since && !o.selector.includes(' OR ') && o.since !== 'origin/master' ? ` --changed ${o.since}` : ''} --profile ${[o.profile].concat(o.layers).join(',')} --seed ${o.seed}${o.repeat > 1 ? ` --repeat ${o.repeat}` : ''}${o.bail ? ' --bail' : ''} --concurrency ${o.concurrency}${o.strict ? ' --strict' : ''}${o.reporters.filter((r) => r !== 'pretty').map((r) => ` --reporter ${r}`).join('')}`; };

  /* seed the log of the finished run 482 so the screen opens on a complete report */
  function evFor(r, i) {
    const out = [];
    const prev = R.plan[i - 1];
    const file = r.id.split('#')[0];
    if (!prev || prev.id.split('#')[0] !== file) {
      const lc = TD.files[file].lifecycle;
      if (lc.beforeAll.length) out.push({ k: 'life', st: 'pass', txt: `beforeAll ${lc.beforeAll[0].replace('call: ', '')}`, sub: 'cache hit · 3h left' });
      if (lc.beforeEach.length) out.push({ k: 'life', st: 'pass', txt: `beforeEach ×${lc.beforeEach.length} actions for ${file}` });
    }
    out.push({ k: 'test', st: r.st, txt: r.label, sub: r.st === 'fail' ? 'body.$.error.code: expected card_declined, got "insufficient_funds" · checkout.test.yaml:43' : r.st === 'blocked' ? 'beforeEach carts#addItem returned 409, so the test did not run' : r.st === 'skipped' ? 'skip until 2026-12-01: token service rollout' : r.st === 'flaky' ? 'failed once, passed on retry 1 of 2' : r.st === 'allow' ? 'failed, allowed until 2026-12-07' : '', ms: r.ms, id: r.id });
    if (r.st === 'fail' || r.st === 'blocked') out.push({ k: 'http', st: r.st, txt: r.st === 'fail' ? 'POST /payments/charges → 402 · 188 ms' : 'POST /carts/c_91x/items → 409 · 41 ms' });
    return out;
  }
  function stamp(list, t) { return list.map((e, j) => Object.assign({ t: clock(t + j * 40) }, e)); }
  function seedDone() {
    R.plan = TD.run.instances.slice();
    R.events = [{ k: 'run', st: 'pass', t: '00:00.0', txt: `plan ${R.plan.length} instances in 4 files · seed 42 · concurrency 4`, sub: 'lifecycle: beforeAll catalog (cached, 3h left) · beforeEach ×3 actions · profile local' }, { k: 'life', st: 'pass', t: '00:00.1', txt: 'beforeRun auth shopUser', sub: 'token cached, 41 min left' }];
    let t = 200;
    R.plan.forEach((r, i) => { R.events.push(...stamp(evFor(r, i), t)); t += 260; });
    R.events.push(...stamp(tail(), t));
  }
  const tail = () => [{ k: 'life', st: 'pass', txt: 'afterAll command ./scripts/print-order-stats.sh', sub: 'exit 0 · 3 lines' }, { k: 'life', st: 'pass', txt: `cleanup ${CLEANUPS.length} resources, newest first`, sub: 'all deleted' }, { k: 'run', st: 'fail', txt: summaryText(), sub: 'exit 1 · report .vc/reports/run-482.html' }];
  const summaryText = () => { const c = counts(); return `run ${R.n} · ${ST.filter((s) => c[s]).map((s) => `${c[s]} ${LABEL[s]}`).join(' · ')}`; };
  seedDone();

  /* ---- live run ----------------------------------------------------------- */
  VC.startRun = function (o) {
    VC.closeDialog();
    clearInterval(R.timer);
    Object.assign(R, { phase: 'running', n: R.n + 1, done: 0, results: [], buffered: 0, clean: 0, t0: 0, only: o && o.only });
    if (o && o.selector) R.opts.selector = o.selector;
    R.plan = R.only ? R.only.map((id) => { const t = TD.tests[id]; return { id, st: t.st === 'skipped' ? 'skipped' : t.st, ms: t.ms, label: id }; }) : TD.run.instances.slice();
    if (o && o.failedOnly) R.plan = TD.run.instances.filter((r) => r.st === 'fail' || r.st === 'blocked').map((r) => Object.assign({}, r, { st: r.st === 'fail' ? 'fail' : 'pass' }));
    R.open = {};
    R.events = [{ k: 'run', st: 'pass', t: '00:00.0', txt: `$ ${cmd()}` }, { k: 'run', st: 'pass', t: '00:00.0', txt: `plan ${R.plan.length} instance${R.plan.length === 1 ? '' : 's'} · seed ${R.opts.seed} · concurrency ${R.opts.concurrency}`, sub: `profile ${[R.opts.profile].concat(R.opts.layers).join(' + ')}` }, { k: 'life', st: 'pass', t: '00:00.1', txt: 'beforeRun auth shopUser', sub: 'token cached, 41 min left' }];
    if (S.route !== 'tests-run') VC.go('tests-run'); else VC.render();
    if (!reduced()) R.timer = setInterval(tick, 420);
  };
  function tick() {
    if (R.phase === 'running') {
      const r = R.plan[R.done];
      const ev = evFor(r, R.done);
      if (R.paused) R.buffered += ev.length;
      R.events.push(...stamp(ev, (R.t0 += 260)));
      R.results.push(r);
      R.done++;
      if (R.opts.bail && r.st === 'fail') return stop('bail');
      if (R.done >= R.plan.length) { R.events.push(...stamp(tail(), (R.t0 += 300))); return finish('done'); }
    } else if (R.phase === 'stopping') {
      R.events.push({ k: 'life', st: 'pass', t: clock((R.t0 += 180)), txt: `cleanup ${CLEANUPS[R.clean]}`, sub: '204 · deleted' });
      R.clean++;
      if (R.clean >= CLEANUPS.length) { R.events.push({ k: 'run', st: 'skipped', t: clock(R.t0 + 20), txt: `${summaryText()} · cancelled`, sub: 'exit 130 · every cleanup ran' }); return finish('cancelled'); }
    }
    paint();
  }
  function finish(phase) { clearInterval(R.timer); R.timer = null; R.phase = phase; R.ms = R.t0 + 400; VC.render(); }
  function stop(why) {
    R.phase = 'stopping';
    R.events.push({ k: 'run', st: 'blocked', t: clock(R.t0 + 10), txt: why === 'bail' ? 'bail: first failure stops the run' : 'stop requested · running cleanups', sub: `${CLEANUPS.length} resources to delete, newest first` });
    VC.render();
  }
  function paint() {
    const live = $('#tr-live');
    if (!live) return;
    live.innerHTML = liveHtml();
    const buf = $('#tr-buf'); if (buf) buf.innerHTML = bufHtml();
    if (R.paused) return;
    const log = $('#tr-log');
    if (log) { log.outerHTML = logHtml(); const l2 = $('#tr-log'); l2.scrollTop = l2.scrollHeight; }
  }

  /* ---- render pieces ------------------------------------------------------ */
  function counters() {
    const c = counts(), rest = ST.filter((s) => s !== 'pass' && s !== 'fail' && c[s]).map((s) => `${c[s]} ${LABEL[s]}`).join(' · ');
    return `<p class="tr-sum"${rest ? ` title="${rest}"` : ''}><span class="dot st-pass" aria-hidden="true"></span><b class="num">${c.pass}</b> passed<span class="tr-sep" aria-hidden="true">·</span><span class="dot st-fail" aria-hidden="true"></span><b class="num">${c.fail}</b> failed${rest ? `<span class="sr-only"> · ${rest}</span>` : ''}</p>`;
  }
  function nowRunning() {
    if (R.phase !== 'running') return '';
    const next = R.plan.slice(R.done, R.done + R.opts.concurrency);
    return `<section aria-labelledby="tr-now"><h3 class="section-label" id="tr-now">Now running · ${next.length} of concurrency ${R.opts.concurrency}</h3>
      <ul class="tr-now">${next.map((r, i) => `<li><span class="dot is-live" aria-hidden="true"></span><span class="mono ellipsis">${esc(r.label)}</span><span class="subtle mono">${['call', 'beforeEach', 'checks', 'call'][i % 4]}</span></li>`).join('')}</ul></section>`;
  }
  function logHtml() {
    const keep = (e) => R.filter === 'all' || (R.filter === 'fail' ? ['fail', 'blocked', 'flaky'].includes(e.st) && e.k !== 'run' : e.k === R.filter);
    const ev = R.events.filter(keep);
    if (!ev.length) return `<div class="empty empty-quiet tr-log" id="tr-log"><span>No events match this filter.</span><button class="link" data-action="tr-filter" data-v="all">Show all events</button></div>`;
    return `<ol class="events tr-log glass-dense" id="tr-log" tabindex="0" aria-label="Event log" aria-live="${R.phase === 'running' && !R.paused ? 'polite' : 'off'}">${ev.map(evHtml).join('')}</ol>`;
  }
  /* D5: one line per event (time · icon · name · duration); a detail line only for failures, lifecycle and run lines */
  const SUB_ST = ['fail', 'blocked'];
  function evHtml(e) {
    const st = e.k === 'test' ? `<span class="sr-only">${LABEL[e.st] || e.st} </span>` : '';
    const name = e.id ? `<button class="tr-evlink tr-name mono" data-action="tr-drill" data-id="${esc(e.id)}" title="${esc(e.txt)}">${st}${esc(e.txt)}</button>` : `<span class="tr-name mono" title="${esc(e.txt)}">${esc(e.txt)}</span>`;
    const has = e.sub && (SUB_ST.includes(e.st) || e.k === 'life' || e.k === 'run'), k = R.events.indexOf(e), open = has && !!R.open[k];
    const x = has ? `<button class="tr-evx" data-action="tr-ev" data-i="${k}" aria-expanded="${open}" aria-label="${open ? 'Hide' : 'Show'} detail">${ic('chev-down', 'ico-sm')}</button>` : '<span></span>';
    return `<li class="tr-ev is-${e.k}${has ? ' has-sub' : ''}${open ? ' is-open' : ''}"${has ? ` data-action="tr-ev" data-i="${k}"` : ''}><span class="t mono">${esc(e.t)}</span>${ic(e.st)}${name}<span class="tr-ms mono">${e.ms ? `${e.ms} ms` : ''}</span>${x}${open ? `<small class="tr-sub">${esc(e.sub)}</small>` : ''}</li>`;
  }
  const bufHtml = () => (R.paused && R.buffered ? `<span class="chip">${R.buffered} new while paused</span>` : '');
  function liveHtml() {
    const n = total(), d = R.phase === 'running' ? R.done : R.results.length;
    const head = { running: `Running · ${d} of ${n}`, stopping: `Stopping · cleanup ${R.clean} of ${CLEANUPS.length}`, done: `Run ${R.n} finished in ${(R.ms / 1000).toFixed(1)} s`, cancelled: `Run ${R.n} cancelled` }[R.phase];
    return `<div class="tr-progress"><div class="row"><b>${esc(head)}</b><span class="grow"></span><span class="subtle mono">${R.phase === 'stopping' ? 'cleanups' : `${Math.round((d / n) * 100)}%`}</span></div>
        <progress class="tr-prog${R.phase === 'stopping' ? ' is-stopping' : ''}" max="${R.phase === 'stopping' ? CLEANUPS.length : n}" value="${R.phase === 'stopping' ? R.clean : d}" aria-label="Run progress"></progress></div>
      ${counters()}${stoppingHtml()}${nowRunning()}`;
  }
  function stoppingHtml() {
    if (R.phase !== 'stopping') return '';
    return `<div class="callout" data-tone="warn">${ic('warn')}<span><b>Cleanups are running</b> so the run leaves nothing behind (${CLEANUPS.length - R.clean} left). Skipping them leaves ${CLEANUPS.length - R.clean} resources on <b>${esc(R.opts.profile)}</b>; they are listed in the report under "left behind".</span>
      ${R.clean > 0 ? '<button class="btn btn-sm btn-danger" data-action="tr-skip-clean" data-fk="tr-skipclean">Skip cleanups</button>' : ''}</div>`;
  }
  function controls() {
    if (R.phase === 'running' || R.phase === 'stopping') {
      return `${reduced() ? `<button class="btn btn-sm" data-action="tr-step" data-fk="tr-step">${ic('arrow-right', 'ico-sm')}Step</button><button class="btn btn-sm btn-ghost" data-action="tr-finish" data-fk="tr-fin">Finish</button>` : ''}
        ${R.phase === 'running' ? `<button class="btn btn-sm btn-danger" data-action="tr-stop" data-fk="tr-stop">${ic('stop', 'ico-sm')}Stop</button>` : ''}`;
    }
    const failed = R.results.filter((r) => r.st === 'fail' || r.st === 'blocked').length;
    return `<button class="btn btn-sm" data-action="tr-rerun-failed" data-fk="tr-rf"${failed ? '' : ' disabled'}>${ic('refresh', 'ico-sm')}Re-run failed${failed ? ` (${failed})` : ''}</button>
      <button class="btn btn-sm" data-action="tr-again" data-fk="tr-again">${ic('play', 'ico-sm')}Run again</button>
      ${VC.moreBtn('tr-more', 'More run actions')}`;
  }
  function leftHtml() {
    const F = [['all', 'All'], ['fail', 'Failures'], ['test', 'Tests'], ['life', 'Lifecycle'], ['http', 'HTTP']];
    return `<section class="glass" aria-labelledby="tr-title">
      <div class="card-head"><h2 class="card-title" id="tr-title" title="${esc(cmd())}">Run ${R.n}</h2><span class="grow"></span><div class="row">${controls()}</div></div>
      <div class="card-tabs" id="tr-live">${liveHtml()}</div>
      <div class="card-tabs tr-logbar"><div class="seg seg-sm" role="group" aria-label="Filter events">${F.map(([k, l]) => `<button aria-pressed="${R.filter === k}" data-action="tr-filter" data-v="${k}" data-fk="trf-${k}">${l}</button>`).join('')}</div><span class="grow"></span>
        <span id="tr-buf">${bufHtml()}</span><button class="btn btn-sm btn-ghost" aria-pressed="${R.paused}" data-action="tr-pause" data-fk="tr-pause">${ic(R.paused ? 'play' : 'pause', 'ico-sm')}${R.paused ? 'Resume log' : 'Pause log'}</button></div>
      <div class="card-body tr-logwrap">${logHtml()}</div></section>`;
  }

  /* ---- drill-down --------------------------------------------------------- */
  function drillData(id) {
    if (id === TD.drill.id) return TD.drill;
    const t = TD.tests[id] || {};
    const a = D.apis[(t.apis || [])[0]] || { m: 'GET', path: '/' };
    return { id, at: `${TD.files[t.file] ? TD.files[t.file].path : ''}:${t.line || 1}`, exchanges: [{ phase: 'call', m: a.m, path: a.path.replace('${userId}', 'u_3kd8'), s: a.m === 'POST' ? 201 : 200, ms: t.ms || 0, main: true }],
      checks: [{ st: 'pass', what: 'status', exp: a.m === 'POST' ? '201' : '200', got: a.m === 'POST' ? '201' : '200', from: 'api', at: 'api file' }, { st: 'pass', what: 'body.schema', exp: 'User', got: 'valid', from: 'api', at: 'api file' }], span: null, diff: null, headers: TD.drill.headers };
  }
  function checksHtml(d) {
    return `<div class="table-wrap glass-dense"><table class="table tr-fit" aria-label="Checks"><colgroup><col class="c-chk"><col><col></colgroup><thead><tr><th>Check</th><th>Expected</th><th>Actual</th></tr></thead><tbody>
      ${d.checks.map((c) => `<tr${c.st === 'fail' ? ' class="tr-badrow"' : ''}><td class="mono" title="${LABEL[c.st] || c.st} · ${esc(c.what)} · from ${esc(c.from)} · ${esc(c.at)}"><span class="tr-chk">${ic(c.st, `ico-sm tr-ci is-${c.st}`)}<span class="sr-only">${LABEL[c.st] || c.st}: </span><span class="ellipsis">${esc(c.what)}</span></span></td><td class="mono" title="${esc(c.exp)}">${esc(c.exp)}</td><td class="mono${c.st === 'fail' ? ' tr-bad' : ''}" title="${esc(c.got)}">${esc(c.got)}</td></tr>`).join('')}</tbody></table></div>
      ${d.span ? `<section aria-label="Where the expectation is written"><div class="row"><h4 class="section-label" title="V5120 · services/orders/checkout.test.yaml:43">${ic('fail', 'ico-sm')} Where it is written · line 43</h4><span class="grow"></span><button class="btn btn-sm btn-ghost" data-action="tr-open-editor">${ic('code', 'ico-sm')}Open in Editor</button></div>
        <pre class="code glass-dense tr-span">${d.span.map(([n, l]) => `<span class="ln">${n}</span>${n === 43 ? `<span class="tr-hit">${VC.h.hlYaml(l)}</span>` : VC.h.hlYaml(l)}`).join('\n')}\n<span class="ln"></span><span class="tr-caret">                     ^^^^^^^^^^^^^ expected card_declined</span></pre></section>` : ''}`;
  }
  function exchangesHtml(d) {
    return `<div class="table-wrap glass-dense"><table class="table" aria-label="Exchanges"><thead><tr><th>Phase</th><th>Request</th><th class="r">Status</th><th class="r">Time</th></tr></thead><tbody>
      ${d.exchanges.map((x) => `<tr${x.main ? ' aria-selected="true"' : ''}><td class="mono muted">${esc(x.phase)}</td><td>${method(x.m)} <span class="mono">${esc(x.path)}</span></td><td class="r"><span class="hs" data-class="${String(x.s)[0]}">${x.s}</span></td><td class="r mono muted">${x.ms} ms</td></tr>`).join('')}</tbody></table></div>
      <section aria-label="Request headers"><h4 class="section-label">Request headers · main call</h4><div class="kv glass-dense">${d.headers.map(([k, v, src]) => `<div class="kv-row"><span class="k">${esc(k)}</span><span class="v">${src === 'secret' ? '<span>Bearer <span class="redacted">••••</span></span>' : `<span>${esc(v)}</span>`}</span><span class="s">${chipSrc(src)}</span></div>`).join('')}</div></section>`;
  }
  function drillHtml() {
    const d = drillData(R.drill);
    const r = R.results.find((x) => x.id === R.drill) || { st: (TD.tests[R.drill] || {}).st || 'pass' };
    const tabs = [['checks', 'Checks', `<span class="count${d.checks.some((c) => c.st === 'fail') ? ' is-bad' : ''}">${d.checks.length}</span>`], ['exchanges', 'Exchanges', `<span class="count">${d.exchanges.length}</span>`], ['diff', 'Body diff']];
    const body = R.tab === 'checks' ? checksHtml(d) : R.tab === 'exchanges' ? exchangesHtml(d) : d.diff ? `<p class="subtle">Expected body (from the matchers) against the actual response, at <code>body.error</code>.</p>${VC.diffHtml(d.diff, 'Body diff')}` : '<div class="empty empty-quiet"><span>No body difference. Every body check passed.</span></div>';
    return `<section class="glass" aria-labelledby="trd-title">
      <div class="card-head"><div class="grow row tr-dhead">${badge(r.st)}<h2 class="card-title mono ellipsis" id="trd-title" title="${esc(R.drill)} · ${esc(d.at)}">${esc(R.drill.split('#').pop())}</h2></div>
        <button class="btn btn-sm btn-ghost" data-action="tr-drill-menu" aria-haspopup="menu" data-fk="trd-pick">${ic('flow', 'ico-sm')}Other test${ic('chev-down', 'ico-sm')}</button></div>
      <div class="card-tabs">${VC.tabs('trd', 'Drill-down', tabs, R.tab, 'tr-tab')}</div>
      ${VC.panel('trd', R.tab, body)}</section>`;
  }

  /* ---- screen ------------------------------------------------------------- */
  VC.screens['tests-run'] = {
    enter: () => { S.mode = 'tests'; },
    crumbs: () => [D.project.name, 'Tests', `Run ${R.n}`],
    status: () => '',
    render: () => `<div class="split tr-split"${VC.splitAttrs()}>${leftHtml()}${VC.divider('Resize run and drill-down')}${drillHtml()}</div>`,
    mount: (view) => {
      VC.mountSplit(view, 'trSplit');
      const log = $('#tr-log', view);
      if (log && !R.paused && R.phase !== 'done' && R.phase !== 'cancelled') log.scrollTop = log.scrollHeight;
      if (R.phase === 'running' && !R.timer && !reduced()) R.timer = setInterval(tick, 420);
      if (R.phase === 'stopping' && !R.timer && !reduced()) R.timer = setInterval(tick, 420);
    },
    unmount: () => { if (S.route !== 'tests-run') { clearInterval(R.timer); R.timer = null; } }
  };

  /* ---- actions ------------------------------------------------------------ */
  A['tr-filter'] = (el) => { R.filter = el.dataset.v; VC.render(); };
  A['tr-ev'] = (el) => {
    const k = el.dataset.i, log = $('#tr-log');
    R.open[k] = !R.open[k];
    if (!log) return VC.render();
    const top = log.scrollTop;
    log.outerHTML = logHtml();
    const l2 = $('#tr-log'); l2.scrollTop = top;
    const x = $(`.tr-evx[data-i="${k}"]`, l2); if (x && el.classList.contains('tr-evx')) x.focus({ preventScroll: true });
  };
  A['tr-pause'] = () => { R.paused = !R.paused; R.buffered = 0; VC.render(); };
  A['tr-tab'] = (el) => { R.tab = el.dataset.v; VC.render(); };
  A['tr-drill'] = (el) => { R.drill = el.dataset.id; R.tab = 'checks'; VC.render(); const h = $('#trd-title'); if (h) h.scrollIntoView({ block: 'nearest' }); };
  A['tr-drill-menu'] = (el) => VC.menu(el, `<div class="pop-label">Drill into</div>${[...new Set(R.results.map((r) => r.id))].map((id) => {
    const r = R.results.find((x) => x.id === id);
    return VC.menuItem(`<span class="dot st-${r.st}"></span><span class="mono">${esc(id)}</span>`, 'tr-drill', `data-id="${esc(id)}"`, { radio: true, checked: id === R.drill });
  }).join('')}`);
  A['tr-step'] = () => { tick(); if (R.phase === 'running' || R.phase === 'stopping') { const b = $('[data-fk="tr-step"]'); if (b) b.focus(); } };
  A['tr-finish'] = () => { let n = 0; while ((R.phase === 'running' || R.phase === 'stopping') && n++ < 200) tick(); };
  A['tr-stop'] = () => { clearInterval(R.timer); R.timer = null; stop('stop'); };
  A['tr-skip-clean'] = () => {
    const left = CLEANUPS.slice(R.clean);
    R.events.push({ k: 'run', st: 'blocked', t: clock(R.t0 + 30), txt: `cleanups skipped · ${left.length} left behind`, sub: left.join(' · ') }, { k: 'run', st: 'skipped', t: clock(R.t0 + 40), txt: `${summaryText()} · cancelled`, sub: 'exit 130' });
    finish('cancelled');
    VC.toast(`Left ${left.length} resources on ${R.opts.profile}`, 'Listed in the report under "left behind"; vc clean removes them later.', 'warn');
  };
  A['tr-more'] = (el) => VC.menu(el, `${VC.menuItem('Open report', 'tr-report', '', { ico: 'external' })}${VC.menuItem('Compare with previous', 'tr-compare', '', { ico: 'columns' })}${VC.menuItem('Copy command', 'tr-copy-cmd', '', { ico: 'copy' })}<div class="pop-sep"></div>${VC.menuItem('New run…', 'tx-run-dialog', '', { ico: 'play' })}`);
  A['tr-again'] = () => VC.startRun({ selector: R.opts.selector, only: R.only });
  A['tr-rerun-failed'] = () => VC.startRun({ failedOnly: true, selector: '--failed' });
  A['tr-report'] = () => VC.toast(`Opened .vc/reports/run-${R.n}.html`, 'html reporter · secrets redacted before the file is written');
  A['tr-open-editor'] = () => { VC.edSel && VC.edSel('orders.checkout#rejectsDeclinedCard'); VC.go('tests-editor'); };
  A['tr-compare'] = () => {
    const p = TD.run.prev, c = counts();
    const rows = [['fail', 'Newly failing', 'orders.checkout#rejectsDeclinedCard', 'passed in 481'], ['blocked', 'Newly blocked', 'orders.checkout#refundsPartially', 'carts#addItem 409'], ['flaky', 'Still flaky', 'users.crud#rejectsBadEmail', 'flaky in 3 of the last 5'], ['pass', 'Slower', 'orders.checkout#placesOrder', '412 ms vs 291 ms (+42%)'], ['pass', 'Faster', 'users.crud#renamesUser', '233 ms vs 301 ms']];
    VC.dialog(`<div class="dlg dlg-wide"><div class="dlg-head"><div class="grow"><h2 id="dlg-title">Run ${R.n} vs run ${R.n - 1}</h2><div class="sub">Same selector and profile · ${esc(D.project.branch)} · ${(R.ms / 1000).toFixed(1)} s vs ${(p.ms / 1000).toFixed(1)} s</div></div><button class="btn btn-ghost btn-icon" data-action="close-dialog" aria-label="Close">${ic('x')}</button></div>
      <div class="dlg-body"><div class="table-wrap glass-dense"><table class="table"><thead><tr><th>Status</th><th class="r">Run ${R.n - 1}</th><th class="r">Run ${R.n}</th><th class="r">Change</th></tr></thead><tbody>
        ${ST.map((s) => `<tr><td>${badge(s, LABEL[s], true)}</td><td class="r num">${p[s] || 0}</td><td class="r num">${c[s]}</td><td class="r num">${c[s] - (p[s] || 0) > 0 ? '+' : ''}${c[s] - (p[s] || 0)}</td></tr>`).join('')}</tbody></table></div>
        <div class="table-wrap glass-dense"><table class="table"><thead><tr><th>What changed</th><th>Test</th><th>Detail</th></tr></thead><tbody>${rows.map(([s, w, id, n]) => `<tr><td>${badge(s, w, true)}</td><td class="mono">${esc(id)}</td><td class="muted">${esc(n)}</td></tr>`).join('')}</tbody></table></div></div>
      <div class="dlg-foot"><span class="note">Same as <code>vc report compare ${R.n - 1} ${R.n}</code></span><button class="btn btn-primary" data-action="close-dialog">Done</button></div></div>`, 'is-wide');
  };

  /* ---- the Run dialog (also ⌘R / Ctrl+R in Tests mode) --------------------- */
  /* P2: selector, profile and two key options up front; the rest under "More options" */
  const sw = (id, on, label) => `<label class="switch"><input type="checkbox" id="${id}"${on ? ' checked' : ''}><span class="track"></span><span>${label}</span></label>`;
  const checks = (cls, all, on) => `<div class="row row-wrap tr-checks">${all.map((v) => `<label class="tr-check"><input type="checkbox" class="${cls}" value="${esc(v)}"${on.includes(v) ? ' checked' : ''}>${esc(v)}</label>`).join('')}</div>`;
  function planLine() {
    const ins = TD.run.instances;
    const files = new Set(ins.map((r) => r.id.split('#')[0])).size;
    const secs = Math.max(1, Math.round(ins.reduce((n, r) => n + (r.ms || 0), 0) * R.opts.repeat / Math.max(1, R.opts.concurrency) / 1000 + 1));
    return `<p class="tr-plan" id="trd-plan">${ic('flow', 'ico-sm')}<span><b class="num">${ins.length * R.opts.repeat}</b> tests · <b class="num">${files}</b> files · ~${secs} s</span></p>`;
  }
  function moreOptions(o) {
    return `<details class="more" data-fk="trd-more"${R.moreOpen ? ' open' : ''}><summary>${ic('chev-down', 'chev')}More options</summary><div class="more-body">
      <div class="form-grid">
        <fieldset class="field form-group span-2"><legend>Layers</legend>${checks('trd-layer', D.layers.map((l) => l.id), o.layers)}</fieldset>
        <div class="field"><label for="trd-since">Since commit</label><input class="input mono" id="trd-since" value="${esc(o.since)}" placeholder="origin/master"><span class="hint">Adds tests changed since this ref.</span></div>
        <div class="field"><label for="trd-seed">Seed</label><input class="input mono" id="trd-seed" type="number" value="${o.seed}"><span class="hint">Same seed, same fakes and order.</span></div>
        <div class="field"><label for="trd-rep">Repeat</label><input class="input mono" id="trd-rep" type="number" min="1" max="100" value="${o.repeat}"></div>
        <div class="field tr-switch-cell"><span class="label" id="trd-strict-l">Strict</span>${sw('trd-strict', o.strict, 'Warnings fail the run')}</div>
        <fieldset class="field form-group span-2"><legend>Reporters</legend>${checks('trd-rep', ['pretty', 'json', 'junit', 'html', 'tap'], o.reporters)}</fieldset>
      </div>
      <div class="field"><div class="row"><span class="label" id="trd-cmd-l">Command</span><span class="grow"></span><button class="btn btn-sm btn-ghost" data-action="tr-copy-cmd" data-fk="trd-copy">${ic('copy', 'ico-sm')}Copy command</button></div>
        <pre class="code glass-dense tr-cmd" aria-labelledby="trd-cmd-l" id="trd-cmd">${esc(cmd())}</pre></div>
    </div></details>`;
  }
  function runDialogHtml() {
    const o = R.opts;
    const prod = o.profile === 'prod';
    return `<div class="dlg dlg-form tr-dlg"><div class="dlg-head"><div class="grow"><h2 id="dlg-title">Run tests</h2><div class="sub">Runs the selection with <code>vc run</code> on the profile you pick.</div></div><button class="btn btn-ghost btn-icon" data-action="close-dialog" aria-label="Close">${ic('x')}</button></div>
      <div class="dlg-body">
        <div class="form-grid">
          <div class="field span-2"><label for="trd-sel">Selector</label><input class="input mono" id="trd-sel" value="${esc(o.selector)}" autofocus spellcheck="false" aria-describedby="trd-sel-h"><span class="hint" id="trd-sel-h">Test or file id, glob, <code>tag:smoke</code>; OR joins.</span></div>
          <div class="field"><label for="trd-prof">Profile</label><select class="select" id="trd-prof">${D.profiles.map((p) => `<option${p.id === o.profile ? ' selected' : ''}>${esc(p.id)}</option>`).join('')}</select></div>
          <div class="field"><label for="trd-conc">Concurrency</label><input class="input mono" id="trd-conc" type="number" min="1" max="64" value="${o.concurrency}"></div>
          <div class="span-2">${sw('trd-bail', o.bail, 'Bail on first failure')}</div>
          ${prod ? `<p class="callout span-2" data-tone="danger">${ic('warn')}<span><b>prod · live data.</b> Tests that write ask first, and cleanups delete real records.</span></p>` : ''}
        </div>
        ${planLine()}
        ${moreOptions(o)}
      </div>
      <div class="dlg-foot"><button class="btn" data-action="close-dialog">Cancel</button><button class="btn btn-primary${prod ? ' btn-danger' : ''}" data-action="tr-go" id="trd-go">${ic('play')}Run</button></div></div>`;
  }
  A['tx-run-dialog'] = () => VC.dialog(runDialogHtml());
  /* ⌘R (bound in app.js): Load runs its scenario, anywhere else opens the run dialog */
  VC.openRun = () => (S.route === 'tests-load' && A['ld-run'] ? A['ld-run']() : A['tx-run-dialog']());
  A['tr-go'] = () => VC.startRun({ selector: R.opts.selector });
  A['tr-copy-cmd'] = () => VC.copy(cmd(), 'Copied vc run', cmd());
  function readDialog() {
    const v = (id) => ($('#' + id) || {}).value;
    const o = R.opts;
    o.selector = v('trd-sel') || o.selector; o.since = v('trd-since') || ''; o.seed = +v('trd-seed') || 0; o.repeat = Math.max(1, +v('trd-rep') || 1); o.concurrency = Math.max(1, +v('trd-conc') || 1);
    o.layers = $$('.trd-layer:checked').map((x) => x.value); o.reporters = $$('.trd-rep:checked').map((x) => x.value);
    o.bail = !!($('#trd-bail') || {}).checked; o.strict = !!($('#trd-strict') || {}).checked;
    const c = $('#trd-cmd'); if (c) c.textContent = cmd();
    const pl = $('#trd-plan'); if (pl) pl.outerHTML = planLine();
  }
  document.addEventListener('input', (e) => { if (e.target.closest && e.target.closest('.tr-dlg')) readDialog(); });
  document.addEventListener('change', (e) => {
    if (!e.target.closest || !e.target.closest('.tr-dlg')) return;
    if (e.target.id === 'trd-prof') { R.opts.profile = e.target.value; readDialog(); VC.dialog(runDialogHtml()); $('#trd-prof').focus(); return; }
    readDialog();
  });
  document.addEventListener('toggle', (e) => {
    if (!e.target.matches || !e.target.matches('.tr-dlg details.more')) return;
    R.moreOpen = e.target.open;
    if (e.target.open && e.target.dataset.byUser) e.target.querySelector('summary').scrollIntoView({ block: 'start', behavior: reduced() ? 'auto' : 'smooth' });
  }, true);
  document.addEventListener('click', (e) => { const d = e.target.closest && e.target.closest('.tr-dlg details.more > summary'); if (d) d.parentElement.dataset.byUser = '1'; }, true);
  document.addEventListener('keydown', (e) => {
    const mod = e.metaKey || e.ctrlKey;
    if (mod && e.key === 'Enter' && $('#dlg').open && $('.tr-dlg')) { e.preventDefault(); e.stopPropagation(); A['tr-go'](); }
  }, true);
  /* ⌘↵ on this screen re-runs */
  VC.screens['tests-run'].send = () => { if (R.phase === 'done' || R.phase === 'cancelled') A['tr-again'](); };
})();
