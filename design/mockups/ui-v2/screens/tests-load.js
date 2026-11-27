/* Tests space · Load (#tests-load): one load file, two views.
   Read: what the file says (stages, thresholds, the mix) and how its last runs went.
   Quick check runs it inline with 1 user for 10 s and is not saved to Runs; "Run in
   Runs…" in the ⋯ starts a real load run (runs-load, owned by screens/runs-load.js).
   Design (Edit): stages, executor, thresholds and the mix as forms; Save is a diff. */
(function () {
  'use strict';
  const VC = window.VC, D = VC.data, S = VC.state, A = VC.actions, TD = VC.testsData;
  const { $, esc, ic, badge } = VC.h;
  const LD = TD.load, Q = TD.quick;
  /* merged, not replaced: other screens may keep their own keys on VC.ld */
  const L = (VC.ld = Object.assign(VC.ld || {}, { view: 'read', cur: LD.id, dirty: {}, model: 'users', checks: LD.checks, quick: null }));
  const reduced = () => matchMedia('(prefers-reduced-motion: reduce)').matches || document.documentElement.dataset.motion === 'reduced';
  const mmss = (t) => `${Math.floor(t / 60)}:${String(t % 60).padStart(2, '0')}`;
  const fmtN = (v) => (v >= 1000 ? v.toLocaleString('en-US') : String(v));
  const meta = () => TD.loadList.find((l) => l.id === LD.id) || { st: 'pass' };
  const share = (m) => Math.round((m.weight / LD.mix.reduce((s, x) => s + x.weight, 0)) * 100);

  VC.ldOpen = (id) => {
    const l = TD.loadList.find((x) => x.id === id);
    if (l && l.sample) return VC.toast(`${l.id} is a sample row`, 'The mockup models checkout.peak only; it opens instead', 'info', { label: 'Open checkout.peak', ico: 'chart', action: () => VC.ldOpen(LD.id) });
    Object.assign(L, { cur: LD.id, view: 'read', quick: null });
    S.drawer = false;
    if (S.route === 'tests-load') VC.render(); else VC.go('tests-load');
  };

  /* ---- the stage shape, drawn at the figure's measured width ----------------- */
  function stageSvg(w) {
    const h = 96, tot = LD.stages.reduce((s, x) => s + x.duration, 0), max = Math.max(...LD.stages.map((s) => s.users));
    let t = 0;
    const pts = [[0, 0]].concat(LD.stages.map((s) => { t += s.duration; return [t, s.users]; }));
    const x = (v) => 40 + (v / tot) * (w - 56), y = (v) => 8 + (1 - v / max) * (h - 28);
    return `<line class="ld-base" x1="40" x2="${w - 16}" y1="${y(0)}" y2="${y(0)}"/><text class="ld-ax" x="34" y="${y(max) + 4}" text-anchor="end">${fmtN(max)}</text><text class="ld-ax" x="34" y="${y(0) + 4}" text-anchor="end">0</text>
      <path class="ld-area" d="M${x(0)},${y(0)}${pts.map(([a, b]) => `L${x(a).toFixed(1)},${y(b).toFixed(1)}`).join('')}L${x(tot)},${y(0)}Z"/><path class="ld-line s-one" d="${pts.map(([a, b], i) => `${i ? 'L' : 'M'}${x(a).toFixed(1)},${y(b).toFixed(1)}`).join('')}"/>
      ${pts.slice(1).map(([a, b], i) => `<circle class="ld-dot s-p95" cx="${x(a).toFixed(1)}" cy="${y(b).toFixed(1)}" r="4"><title>stage ${i + 1}: ${b} users at ${mmss(a)}</title></circle><text class="ld-ax" x="${x(a).toFixed(1)}" y="${h - 4}" text-anchor="middle">${mmss(a)}</text>`).join('')}`;
  }
  const stagePreview = () => `<svg class="ld-svg is-fluid ld-stages" viewBox="0 0 600 96" preserveAspectRatio="none" role="img" aria-label="Users over time: ${LD.stages.map((s) => `${s.users} users over ${s.duration} s`).join(', ')}">${stageSvg(600)}</svg>`;
  function measure(view) {
    $$in(view, '.ld-stages').forEach((svg) => {
      const w = Math.round(svg.parentElement.clientWidth);
      if (w > 120 && svg.getAttribute('viewBox') !== `0 0 ${w} 96`) { svg.setAttribute('viewBox', `0 0 ${w} 96`); svg.innerHTML = stageSvg(w); }
    });
  }
  const $$in = (root, sel) => Array.from(root.querySelectorAll(sel));

  /* ---- read view ------------------------------------------------------------- */
  const kv = (rows) => `<div class="kv is-readonly" role="table" aria-label="Summary">${rows.map(([k, v]) => `<div class="kv-row" role="row"><span class="k" role="cell">${k}</span><span class="v" role="cell">${v}</span><span class="s" role="cell"></span></div>`).join('')}</div>`;
  function quickHtml() {
    if (!L.quick) return '';
    const head = (inner) => `<div class="tx-res-head">${inner}<span class="grow"></span><span class="tx-res-note">${ic('info', 'ico-sm')}Check run, not saved to Runs</span><button class="btn btn-ghost btn-icon btn-sm" data-action="ld-quick-x" aria-label="Dismiss the quick check" data-fk="ld-quick-x">${ic('x')}</button></div>`;
    if (!L.quick.done) return `<section class="tx-res l2" aria-label="Quick check">${head(`${badge('running', 'Checking')}<span class="subtle">${Q.users} user · ${Q.secs} s on ${esc(S.profile)}</span>`)}</section>`;
    const k = [['req/s', Q.rps], ['p95', `${Q.p95} ms`], ['p50', `${Q.p50} ms`], ['errors', `${Q.err}%`], ['iterations', Q.iters]];
    return `<section class="tx-res l2" aria-label="Quick check result">${head(`${badge(Q.err ? 'fail' : 'pass')}<span class="subtle">${Q.users} user · ${Q.secs} s · every mix test ran</span>`)}
      <dl class="ld-kpis">${k.map(([a, b]) => `<div><dt>${a}</dt><dd class="num">${b}</dd></div>`).join('')}</dl>
      <p class="subtle">Thresholds are not judged at 1 user. The quick check proves the mix runs end to end.</p></section>`;
  }
  function summary() {
    const peak = Math.max(...LD.stages.map((s) => s.users)), dur = LD.stages.reduce((s, x) => s + x.duration, 0);
    return kv([['peak', `<span class="num">${fmtN(peak)} users</span>`], ['duration', `<span class="num">${mmss(dur)}</span> over ${LD.stages.length} stages`], ['executor', 'users (closed model)'],
      ['thinkTime', `<span class="mono">${esc(LD.thinkTime.min)} to ${esc(LD.thinkTime.max)}</span>`], ['checks', `<span class="mono">${esc(LD.checks)}</span>`], ['abortOn', `<span class="mono">${esc(LD.abortOn.join(', '))}</span>`]]);
  }
  const thList = () => `<ul class="ld-list">${LD.thresholds.map((t) => `<li><span class="mono">${esc(t.metric)} ${esc(t.op)} ${t.v}${esc(t.unit)}</span><span class="subtle mono ellipsis grow">${t.test ? esc(t.test) : 'whole mix'}</span>${LD.abortOn.includes(t.metric) ? '<span class="chip">abortOn</span>' : ''}</li>`).join('')}</ul>`;
  const mixList = () => `<ul class="ld-list">${LD.mix.map((m) => `<li><button class="link mono ellipsis" data-action="ld-open-test" data-id="${esc(m.test)}" data-fk="ldm-${esc(m.test)}">${esc(m.test)}</button><span class="grow"></span><meter class="ld-meter" min="0" max="100" value="${share(m)}" aria-label="${esc(m.test)} share">${share(m)}%</meter><span class="num subtle">${share(m)}%</span></li>`).join('')}</ul>`;
  const prevHtml = () => `<div class="table-wrap glass-dense"><table class="table"><thead><tr><th>Run</th><th>Branch</th><th class="r">p99</th><th class="r">it/s</th><th class="r">errors</th><th>Verdict</th></tr></thead>
    <tbody>${LD.previous.map((r) => `<tr${r.cur ? ' aria-current="true"' : ''}><td>${esc(r.run)}${r.cur ? ' <span class="subtle">(last)</span>' : ''}</td><td class="mono">${esc(r.branch)}</td><td class="r num">${r.p99} ms</td><td class="r num">${fmtN(r.rps)}</td><td class="r num">${r.err}%</td><td>${badge(r.verdict, null, true)}</td></tr>`).join('')}</tbody></table></div>`;
  const sec = (id, title, side, body) => `<section aria-labelledby="${id}"><div class="row"><h3 class="section-label" id="${id}">${title}</h3><span class="grow"></span>${side}</div>${body}</section>`;
  function readHtml() {
    const m = meta();
    return `<div class="stage-one tx-one"><section class="glass tx-card" aria-labelledby="ldr-title">
      <div class="card-head"><div class="grow stack-tight">
          <div class="row">${badge(m.st)}<h2 class="card-title mono ellipsis" id="ldr-title" title="${esc(LD.path)}">${esc(LD.id)}</h2><span class="chip ts-load-chip">load</span></div>
          <div class="card-sub"><span class="ellipsis">${esc(LD.name)}</span><span aria-hidden="true">·</span><span class="mono ellipsis">${esc(LD.path)}</span></div></div>
        <div class="row tx-acts"><button class="btn btn-primary btn-sm" data-action="ld-quick" data-fk="ldr-run" title="1 user for 10 s, not saved to Runs">${ic('play', 'ico-sm')}Quick check</button>
          <button class="btn btn-sm" data-action="ld-edit" data-fk="ldr-edit">${ic('pencil', 'ico-sm')}Edit</button>${VC.moreBtn('ld-more', 'More load test actions', 'ldr-more')}</div></div>
      <div class="card-body tx-detail" data-sk="ldr">
        <div id="ld-res" aria-live="polite">${quickHtml()}</div>
        <div class="tx-cols">${sec('ldr-sum', 'Summary', '', summary())}${sec('ldr-st', 'Stages', `<span class="subtle">users ramp from the previous stage</span>`, `<figure class="ld-stagefig">${stagePreview()}</figure>`)}</div>
        <div class="tx-cols">${sec('ldr-th', 'Thresholds', `<span class="subtle">${LD.thresholds.length}</span>`, thList())}${sec('ldr-mx', 'Mix', `<span class="subtle">tests chosen by weight</span>`, mixList())}</div>
        ${sec('ldr-prev', 'Last runs', `<button class="link" data-action="ld-report" data-fk="ldr-report">Open the last report</button>`, prevHtml())}
      </div></section></div>`;
  }

  /* ---- design view (forms) ----------------------------------------------------- */
  const inp = (key, v, label, cls) => `<input class="input input-sm mono${cls ? ' ' + cls : ''}${L.dirty[key] ? ' is-dirty' : ''}" data-ld="${key}" data-orig="${esc(v)}" value="${esc(L.dirty[key] ? L.dirty[key][1] : v)}" aria-label="${esc(label)}" data-fk="ldi-${key}">`;
  const card = (id, title, sub, acts, body) => `<section class="glass" aria-labelledby="${id}"><div class="card-head"><h3 class="card-title" id="${id}">${title}</h3>${sub ? `<span class="card-sub ellipsis" title="${esc(sub)}">${esc(sub)}</span>` : ''}<span class="grow"></span><span class="ld-acts">${acts}</span></div><div class="card-body">${body}</div></section>`;
  const table = (head, rows) => `<div class="table-wrap glass-dense"><table class="table ld-form"><thead><tr>${head}</tr></thead><tbody>${rows}</tbody></table></div>`;
  function stagesCard() {
    let prev = 0;
    const rows = LD.stages.map((s, i) => { const shape = s.users === prev ? 'hold' : s.users > prev ? `ramp up ${fmtN(prev)} → ${fmtN(s.users)}` : `ramp down → ${fmtN(s.users)}`; prev = s.users; return `<tr><td class="r mono muted">${i + 1}</td><td>${inp(`stages.${i}.duration`, mmss(s.duration).replace(/^(\d+):00$/, '$1m'), `Stage ${i + 1} duration`)}</td><td>${inp(`stages.${i}.users`, String(s.users), `Stage ${i + 1} users`)}</td><td class="subtle">${shape}</td><td><button class="btn btn-ghost btn-icon btn-sm" data-action="ld-rm-stage" data-i="${i}" aria-label="Remove stage ${i + 1}" data-fk="ldx-${i}">${ic('x')}</button></td></tr>`; }).join('');
    const acts = `<button class="btn btn-sm btn-ghost" data-action="ld-add-stage" data-v="ramp" data-fk="ld-add-ramp">${ic('plus', 'ico-sm')}Ramp</button><button class="btn btn-sm btn-ghost" data-action="ld-add-stage" data-v="hold" data-fk="ld-add-hold">${ic('plus', 'ico-sm')}Hold</button>`;
    return card('ld-st-t', 'Stages', "users ramp from the previous stage's count", acts, `<figure class="ld-stagefig">${stagePreview()}</figure>${table('<th class="r">#</th><th>duration</th><th>users</th><th>Shape</th><th><span class="sr-only">Remove</span></th>', rows)}`);
  }
  function execCard() {
    const seg = (label, key, opts) => `<div class="seg seg-sm" role="group" aria-label="${label}">${opts.map(([k, l]) => `<button aria-pressed="${L[key] === k}" data-action="ld-${key}" data-v="${k}" data-fk="ld-${key}-${k}">${l}</button>`).join('')}</div>`;
    return card('ld-ex-t', 'Executor', '', '', `<div class="ld-exec">
      <div class="field"><span class="ed-lbl">Model</span>${seg('Executor', 'model', [['users', 'Users (closed)'], ['rate', 'Arrival rate (open)']])}<span class="hint">${L.model === 'users' ? 'Each user runs one mix test, waits thinkTime, repeats. The stages set the user count.' : 'Not in this version: open-model stages are a proposal. The file keeps users.'}</span></div>
      <div class="field"><span class="ed-lbl">thinkTime min</span>${inp('think.min', LD.thinkTime.min, 'thinkTime min')}</div><div class="field"><span class="ed-lbl">thinkTime max</span>${inp('think.max', LD.thinkTime.max, 'thinkTime max')}</div>
      <div class="field"><span class="ed-lbl">checks</span>${seg('Checks', 'checks', [['api', 'api'], ['full', 'full']])}<span class="hint">${L.checks === 'api' ? 'Only the apis’ own expect; fastest.' : 'Every test’s expect; slower runner.'}</span></div></div>`);
  }
  function thCard() {
    const sel = (opts, v, label) => `<select class="select input-sm mono" aria-label="${label}">${opts.map((m) => `<option${m === v ? ' selected' : ''}>${m}</option>`).join('')}</select>`;
    const rows = LD.thresholds.map((t, i) => `<tr><td>${sel(['p50', 'p90', 'p95', 'p99', 'max', 'errorRate', 'rps'], t.metric, 'Metric')}</td><td>${sel(['lt', 'lte', 'gt', 'gte'], t.op, 'Matcher')}</td><td>${inp(`th.${i}`, `${t.v}${t.unit}`, `${t.metric} limit`)}</td><td class="mono ellipsis">${t.test ? esc(t.test) : '<span class="subtle">whole mix</span>'}</td>
      <td><label class="switch"><input type="checkbox" ${LD.abortOn.includes(t.metric) ? 'checked' : ''} ${t.test ? 'disabled' : ''} aria-label="Abort when ${t.metric} breaches"><span class="track"></span></label></td></tr>`).join('');
    return card('ld-th-t', 'Thresholds', 'abortOn stops early on the rolling window', `<button class="btn btn-sm btn-ghost" data-action="noop" data-fk="ld-add-th">${ic('plus', 'ico-sm')}Threshold</button>`, table('<th>Metric</th><th>Matcher</th><th>Limit</th><th>Scope</th><th>abortOn</th>', rows));
  }
  function mixCard() {
    const rows = LD.mix.map((m, i) => `<tr><td class="mono">${esc(m.test)}</td><td>${inp(`mix.${i}.weight`, String(m.weight), `${m.test} weight`, 'ld-num')}</td><td><meter class="ld-meter" min="0" max="100" value="${share(m)}" aria-label="share">${share(m)}%</meter><span class="num subtle"> ${share(m)}%</span></td><td class="mono subtle">${m.p95 ? `p95 lt ${m.p95}` : '—'}</td></tr>`).join('');
    return card('ld-mx-t', 'Mix · scenarios', 'tests chosen by weight', `<button class="btn btn-sm btn-ghost" data-action="noop" data-fk="ld-add-mix">${ic('plus', 'ico-sm')}Test</button>`, table('<th>Test</th><th>weight</th><th>Share</th><th>Own thresholds</th>', rows));
  }
  function designHtml() {
    const nd = Object.keys(L.dirty).length;
    return `<div class="ld-head on-l0"><div class="grow row ld-title"><h1 class="card-title mono ellipsis" title="${esc(LD.path)}">${esc(LD.id)}</h1>${nd ? `<span class="chip ed-dirty" title="Unsaved changes">${nd} unsaved</span>` : ''}<span class="subtle ellipsis ld-sub hide-narrow">${esc(LD.name)}</span></div>
      <button class="btn btn-sm" data-action="ld-done" data-fk="ld-done" title="Back to the read view (Esc)">Done</button>
      <button class="btn btn-sm btn-primary" data-action="ld-save" data-fk="ld-save">${ic('check', 'ico-sm')}Save<span class="kbd hide-narrow">⌘S</span></button></div>
      <div class="ld-body" data-sk="ld-body"><div class="ld-design">${stagesCard()}${execCard()}${thCard()}${mixCard()}</div></div>`;
  }

  /* ---- screen -------------------------------------------------------------------- */
  let ro = null;
  VC.screens['tests-load'] = {
    space: 'tests',
    crumbs: () => [D.project.name, 'Tests', LD.id],
    status: () => (L.view === 'design' ? 'editing' : `last run ${esc(LD.previous[0].run)} · ${esc(meta().last || '')}`),
    primary: () => '',
    render: () => (L.view === 'design' ? designHtml() : readHtml()),
    mount: (view) => {
      measure(view);
      if (window.ResizeObserver) { ro = new ResizeObserver(() => measure(view)); $$in(view, '.ld-stagefig').forEach((f) => ro.observe(f)); }
    },
    unmount: () => { if (ro) ro.disconnect(); ro = null; }
  };

  /* ---- actions ------------------------------------------------------------------- */
  const runLoad = () => (A['rl-run'] ? A['rl-run']() : VC.go('runs-load'));
  A['ld-run'] = runLoad; /* ⌘R on this screen (VC.openRun) */
  A['ld-edit'] = () => { L.view = 'design'; L.quick = null; VC.render(); };
  A['ld-done'] = () => { L.view = 'read'; VC.render(); const b = $('[data-fk="ldr-edit"]'); if (b) b.focus(); };
  A['ld-report'] = () => (VC.openLoadRun ? VC.openLoadRun() : VC.go('runs-load'));
  A['ld-open-test'] = (el) => (VC.txOpen ? VC.txOpen(el.dataset.id) : VC.go('tests'));
  A['ld-copy'] = () => VC.copy(`vc load ${LD.id} --profile ${S.profile}`, 'Copied', `vc load ${LD.id} --profile ${S.profile}`);
  A['ld-more'] = (el) => VC.menu(el, `${VC.menuItem('Run in Runs…', 'ld-run', '', { ico: 'runs', meta: 'full stages, saved' })}${VC.menuItem('Open the last report', 'ld-report', '', { ico: 'chart' })}
    <div class="pop-sep"></div>${VC.menuItem('Copy vc load command', 'ld-copy', '', { ico: 'terminal' })}`);
  A['ld-quick'] = () => {
    L.quick = { done: false };
    const paint = () => { const r = $('#ld-res'); if (r) r.innerHTML = quickHtml(); };
    paint();
    setTimeout(() => { if (!L.quick) return; L.quick.done = true; paint(); }, reduced() ? 0 : 900);
  };
  A['ld-quick-x'] = () => { L.quick = null; const r = $('#ld-res'); if (r) r.innerHTML = ''; const b = $('[data-fk="ldr-run"]'); if (b) b.focus(); };
  A['ld-model'] = (el) => { L.model = el.dataset.v; VC.render(); };
  A['ld-checks'] = (el) => { L.checks = el.dataset.v; if (L.checks !== LD.checks) L.dirty.checks = [LD.checks, L.checks]; else delete L.dirty.checks; VC.render(); };
  A['ld-add-stage'] = (el) => { const last = LD.stages[LD.stages.length - 1]; LD.stages.push({ duration: 60, users: el.dataset.v === 'hold' ? last.users : last.users + 500 }); L.dirty[`stages.${LD.stages.length - 1}`] = ['', 'new']; VC.render(); };
  A['ld-rm-stage'] = (el) => { if (LD.stages.length < 2) return VC.toast('A load file needs at least one stage', 'V1xxx would flag an empty stages list', 'warn'); LD.stages.splice(+el.dataset.i, 1); L.dirty[`stages.rm${el.dataset.i}`] = ['stage', '']; VC.render(); };
  A['ld-save'] = () => {
    const keys = Object.keys(L.dirty);
    if (!keys.length) return VC.toast('Nothing to save', `${LD.path} matches what is on disk`);
    const lines = [['@', '@@ -14,12 +14,12 @@ stages:']];
    keys.forEach((k) => { const [o, n] = L.dirty[k], f = k.split('.').pop(); if (o) lines.push(['-', `  ${f}: ${o}`]); if (n) lines.push(['+', `  ${f}: ${n}`]); });
    VC.diffDialog({ title: `Save ${LD.id}`, sub: 'vc check runs on write', file: LD.path, lines, after: () => { L.dirty = {}; VC.render(); } });
  };
  document.addEventListener('input', (e) => {
    const el = e.target;
    if (!el.dataset || !el.dataset.ld) return;
    if (el.value === el.dataset.orig) delete L.dirty[el.dataset.ld]; else L.dirty[el.dataset.ld] = [el.dataset.orig, el.value];
    el.classList.toggle('is-dirty', !!L.dirty[el.dataset.ld]);
  });
  document.addEventListener('keydown', (e) => {
    if (S.route !== 'tests-load' || L.view !== 'design') return;
    if ((e.metaKey || e.ctrlKey) && e.code === 'KeyS') { e.preventDefault(); return A['ld-save'](); }
    const overlay = $('#dlg').open || $('#menu').matches(':popover-open') || $('#pick').matches(':popover-open') || S.drawer;
    if (e.key === 'Escape' && !overlay && !e.defaultPrevented) { e.preventDefault(); A['ld-done'](); }
  });
})();
