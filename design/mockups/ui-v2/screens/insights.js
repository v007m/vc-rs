/* Insights (#insights): trends over stored runs for one profile and branch. Charts are
   hand-built SVG drawn in mount() at the measured width; every chart has a table view. */
(function () {
  'use strict';
  const VC = window.VC, D = VC.data, A = VC.actions, SH = VC.sh;
  const { $, $$, esc, ic, method } = VC.h;
  const N = D.shared.insights;

  const I = { profile: 'staging', branch: 'master', range: 30, pass: 'chart', dur: 'chart', slow: 'p95', quarantined: {} };
  const NS = 'http://www.w3.org/2000/svg';

  /* ---- charts --------------------------------------------------------------- */
  const CHARTS = {
    pass: { title: 'Pass rate', unit: '%', lo: 90, hi: 100, step: 2, vals: () => N.passRate, fmt: (v) => v.toFixed(1) + '%', sub: 'passed ÷ (passed + failed + blocked), per day' },
    dur: { title: 'Run duration', unit: 's', lo: 60, hi: 100, step: 10, vals: () => N.duration, fmt: (v) => v + ' s', sub: 'median wall time of a full run, per day' }
  };
  const slice = (arr) => arr.slice(-I.range);
  function chartTable(k) {
    const c = CHARTS[k], v = slice(c.vals()), d = slice(N.days);
    return SH.table(['Day', [c.title + ' (' + c.unit + ')', 'r']], d.map((x, i) => `<tr><td class="mono">${x}</td><td class="r mono">${c.fmt(v[i])}</td></tr>`).reverse(), 'in-table-view');
  }
  function chartCard(k) {
    const c = CHARTS[k], v = slice(c.vals());
    const first = v[0], last = v[v.length - 1];
    return `<section class="in-panel" aria-labelledby="in-${k}-t">
      <div class="in-panel-head"><div class="grow"><h3 class="in-h" id="in-${k}-t">${c.title}</h3><span class="subtle">${c.sub}</span></div>
        ${SH.seg(`${c.title} view`, [['chart', 'Chart'], ['table', 'Table']], I[k], 'in-view', ` data-k="${k}"`)}</div>
      ${I[k] === 'table' ? chartTable(k) : `<figure class="in-chart" data-chart="${k}"><div class="in-plot"></div>
        <figcaption class="sr-only">${c.title}, last ${I.range} days: from ${c.fmt(first)} to ${c.fmt(last)}. Use the Table view for every value.</figcaption>
        <div class="in-tip l3" role="status" aria-live="polite" hidden></div></figure>`}
    </section>`;
  }
  function el(tag, attrs, text) {
    const n = document.createElementNS(NS, tag);
    Object.entries(attrs || {}).forEach(([a, v]) => n.setAttribute(a, v));
    if (text !== undefined) n.textContent = text;
    return n;
  }
  function axes(svg, c, g) {
    for (let t = c.lo; t <= c.hi; t += c.step) {
      const y = g.y(t);
      svg.append(el('line', { class: 'grid', x1: g.l, x2: g.w - g.r, y1: y, y2: y }));
      svg.append(el('text', { class: 'tick', x: g.l - 6, y: y + 3.5, 'text-anchor': 'end' }, t + (t === c.hi ? c.unit : '')));
    }
    g.days.forEach((d, i) => {
      if ((g.days.length - 1 - i) % 7) return;
      svg.append(el('text', { class: 'tick', x: g.x(i), y: g.h - 6, 'text-anchor': i === 0 ? 'start' : 'middle' }, d));
    });
  }
  function drawLine(fig) {
    const k = fig.dataset.chart, c = CHARTS[k], plot = $('.in-plot', fig);
    const vals = slice(c.vals()), days = slice(N.days);
    const w = Math.max(280, plot.clientWidth), h = 200;
    const g = { w, h, l: 40, r: 44, t: 12, b: 24, days };
    g.x = (i) => g.l + (i / (vals.length - 1)) * (w - g.l - g.r);
    g.y = (v) => g.t + (1 - (v - c.lo) / (c.hi - c.lo)) * (h - g.t - g.b);
    const svg = el('svg', { width: w, height: h, viewBox: `0 0 ${w} ${h}`, class: 'in-svg', role: 'img', tabindex: '0', 'aria-label': `${c.title} chart. Arrow keys move between days.` });
    axes(svg, c, g);
    const pts = vals.map((v, i) => [g.x(i), g.y(v)]);
    const line = pts.map((p) => p.map((n) => n.toFixed(1)).join(',')).join(' ');
    svg.append(el('polygon', { class: 'area', points: `${g.l},${g.y(c.lo)} ${line} ${g.x(vals.length - 1)},${g.y(c.lo)}` }));
    svg.append(el('polyline', { class: 'line', points: line }));
    const last = pts[pts.length - 1];
    svg.append(el('circle', { class: 'mark', cx: last[0], cy: last[1], r: 4 }));
    svg.append(el('text', { class: 'direct', x: last[0] + 8, y: last[1] + 4 }, c.fmt(vals[vals.length - 1])));
    const cross = el('line', { class: 'cross', x1: 0, x2: 0, y1: g.t, y2: h - g.b, visibility: 'hidden' });
    const dot = el('circle', { class: 'mark', r: 4, visibility: 'hidden' });
    svg.append(cross, dot, el('rect', { class: 'hit', x: g.l, y: 0, width: w - g.l - g.r, height: h }));
    plot.replaceChildren(svg);
    hover(fig, svg, { g, vals, days, c, cross, dot, pts });
  }
  function hover(fig, svg, o) {
    const tip = $('.in-tip', fig);
    let idx = o.vals.length - 1;
    const show = (i) => {
      idx = Math.max(0, Math.min(o.vals.length - 1, i));
      const [x, y] = o.pts[idx];
      o.cross.setAttribute('x1', x); o.cross.setAttribute('x2', x); o.cross.setAttribute('visibility', 'visible');
      o.dot.setAttribute('cx', x); o.dot.setAttribute('cy', y); o.dot.setAttribute('visibility', 'visible');
      const prev = idx ? o.vals[idx] - o.vals[idx - 1] : 0;
      tip.innerHTML = `<b class="mono">${o.days[idx]}</b><span>${o.c.title} <b class="num">${o.c.fmt(o.vals[idx])}</b></span>${idx ? `<span class="subtle num">${prev >= 0 ? '+' : '−'}${Math.abs(prev).toFixed(o.c.unit === '%' ? 1 : 0)} ${o.c.unit === '%' ? 'pts' : 's'} vs the day before</span>` : ''}`;
      tip.hidden = false;
      tip.style.setProperty('--x', Math.min(x, o.g.w - 150) + 'px');
      tip.style.setProperty('--y', Math.max(0, y - 64) + 'px');
    };
    const hide = () => { tip.hidden = true; o.cross.setAttribute('visibility', 'hidden'); o.dot.setAttribute('visibility', 'hidden'); };
    svg.addEventListener('pointermove', (e) => {
      const r = svg.getBoundingClientRect();
      show(Math.round(((e.clientX - r.left - o.g.l) / (o.g.w - o.g.l - o.g.r)) * (o.vals.length - 1)));
    });
    svg.addEventListener('pointerleave', () => { if (document.activeElement !== svg) hide(); });
    svg.addEventListener('focus', () => show(idx));
    svg.addEventListener('blur', hide);
    svg.addEventListener('keydown', (e) => {
      const m = { ArrowLeft: -1, ArrowRight: 1, Home: -999, End: 999 }[e.key];
      if (m === undefined) return;
      e.preventDefault();
      show(Math.abs(m) === 999 ? (m < 0 ? 0 : o.vals.length - 1) : idx + m);
    });
  }

  /* ---- tiles ------------------------------------------------------------------ */
  function kpis() {
    return `<div class="in-kpis" role="list" aria-label="Key numbers">${N.kpi.map((t) => `<div class="in-kpi" role="listitem">
      <span class="in-kpi-k">${esc(t.k)}</span>
      <div class="in-kpi-row"><b class="in-kpi-v num">${esc(t.v)}</b>${t.spark ? SH.spark(slice(t.spark).length > 1 ? slice(t.spark) : t.spark, `${t.k} trend`) : ''}</div>
      <span class="in-kpi-d">${t.good === null ? '' : `<span class="in-delta" data-good="${t.good}">${ic(t.d.startsWith('−') ? 'down' : 'up', 'ico-sm')}${esc(t.d)}</span>`}<span class="subtle">${t.good === null ? esc(t.d) + ' ' : ''}${esc(t.vs)}</span></span>
    </div>`).join('')}</div>`;
  }

  /* ---- flaky, slow, gaps, skips ---------------------------------------------------- */
  /* one tick per run, oldest first; the ticks share the column's whole width */
  const strip = (s) => `<span class="in-strip" role="img" aria-label="${[...s].filter((c) => c === 'f').length} failures in the last ${s.length} runs, oldest first">${[...s].map((c, i) => `<i data-st="${c === 'f' ? 'fail' : 'pass'}" title="run ${482 - s.length + 1 + i} · ${c === 'f' ? 'failed' : 'passed'}${i === s.length - 1 ? ' · newest' : i === 0 ? ' · oldest' : ''}"></i>`).join('')}</span>`;
  function flaky() {
    const rows = N.flaky.map((f) => {
      const q = I.quarantined[f.id];
      return `<tr><td><button class="btn btn-ghost btn-sm mono" data-action="in-open-test" data-id="${esc(f.id)}">${esc(f.id)}</button></td>
        <td class="r mono">${f.rate}%</td><td class="in-strip-cell">${strip(f.strip)}</td><td class="muted hide-narrow">${esc(f.last)}</td>
        <td class="r">${q ? `<span class="badge badge-sm st-allow">${ic('allow')}until ${esc(q)}</span>` : `<button class="btn btn-ghost btn-sm" data-action="in-quarantine" data-id="${esc(f.id)}" data-fk="inq-${esc(f.id)}">${ic('allow')}Quarantine</button>`}</td></tr>`;
    });
    return `<section class="in-panel in-wide" aria-labelledby="in-flaky-t"><div class="in-panel-head"><div class="grow"><h3 class="in-h" id="in-flaky-t">Flaky tests</h3><span class="subtle">Passed on retry, or both passed and failed on the same commit</span></div></div>
      ${SH.table(['Test', ['Fail rate', 'r'], ['Last 24 runs', 'in-strip-cell'], ['Last flake', 'hide-narrow'], ['<span class="sr-only">Action</span>', 'r']], rows, 'in-flat')}</section>`;
  }
  function slow() {
    const max = 650, key = I.slow;
    const list = N.slow.slice().sort((a, b) => b[key] - a[key]);
    const rows = list.map((s) => `<tr><td class="mono">${method(s.m)} ${esc(s.api)}</td>
      <td class="in-range-cell"><span class="in-range" role="img" aria-label="${esc(s.api)}: p50 ${s.p50} ms, p95 ${s.p95} ms" title="p50 ${s.p50} ms · p95 ${s.p95} ms"><i class="bar" data-w="${((s.p95 / max) * 100).toFixed(1)}"></i><i class="p50" data-w="${((s.p50 / max) * 100).toFixed(1)}"></i></span></td>
      <td class="r mono muted">${s.p50}</td><td class="r mono">${s.p95}</td></tr>`);
    const axis = `<span class="in-range-axis" aria-hidden="true">${[0, 200, 400, 600].map((t) => `<span data-w="${(t / max) * 100}">${t}</span>`).join('')}</span>`;
    return `<section class="in-panel" aria-labelledby="in-slow-t"><div class="in-panel-head"><div class="grow"><h3 class="in-h" id="in-slow-t">Slowest apis</h3><span class="subtle">Response time over the range, in ms</span></div>
      ${SH.seg('Sort by', [['p95', 'p95'], ['p50', 'p50']], key, 'in-slow')}</div>
      <div class="in-legend" aria-hidden="true"><span><i class="lg-p50"></i>p50 (median)</span><span><i class="lg-p95"></i>p50 → p95</span></div>
      ${SH.table(['Api', ['Range ' + axis, 'in-range-cell'], ['p50', 'r'], ['p95', 'r']], rows, 'in-flat')}</section>`;
  }
  function gaps() {
    const rows = N.gaps.map((g) => `<li class="in-li"><span class="mono grow ellipsis">${method(g.m)} ${esc(g.api)}</span><span class="subtle">${esc(g.why)}</span>${g.risk ? `<span class="badge badge-sm st-blocked">${ic('warn')}${esc(g.risk)}</span>` : ''}
      <button class="btn btn-ghost btn-sm" data-action="in-scaffold" data-api="${esc(g.api)}">${ic('plus')}Add test</button></li>`).join('');
    return `<section class="in-panel" aria-labelledby="in-gaps-t"><div class="in-panel-head"><div class="grow"><h3 class="in-h" id="in-gaps-t">Coverage gaps</h3><span class="subtle" title="vc coverage">Apis with no running test</span></div></div>
      <ul class="in-list" role="list">${rows}</ul></section>`;
  }
  function skips() {
    const rows = N.skips.map((s, i) => {
      const exp = s.left < 0;
      return `<li class="in-li"><span class="badge badge-sm st-${s.kind === 'skip' ? 'skipped' : 'allow'}">${ic(s.kind === 'skip' ? 'skipped' : 'allow')}${s.kind}</span>
        <span class="stack-tight grow"><button class="btn btn-ghost btn-sm mono in-li-id" data-action="in-open-test" data-id="${esc(s.id)}">${esc(s.id)}</button><span class="subtle ellipsis">"${esc(s.reason)}"</span></span>
        <span class="in-left" data-exp="${exp}"${exp ? ` title="${s.kind === 'skip' ? 'V3104' : 'V4008'}"` : ''}>${exp ? `${ic('warn', 'ico-sm')}expired ${-s.left} days ago` : `${ic('clock', 'ico-sm')}${s.left} days left`}</span>
        <button class="btn btn-sm" data-action="in-extend" data-i="${i}">Extend</button><button class="btn btn-sm btn-ghost" data-action="in-unskip" data-i="${i}">${s.kind === 'skip' ? 'Unskip' : 'Remove'}</button></li>`;
    }).join('');
    return `<section class="in-panel" aria-labelledby="in-skips-t"><div class="in-panel-head"><div class="grow"><h3 class="in-h" id="in-skips-t">Expiring skips</h3><span class="subtle">skip and allowFailure ending in the next 7 days</span></div></div>
      ${rows ? `<ul class="in-list" role="list">${rows}</ul>` : SH.empty('', 'Nothing expires in the next 7 days.')}</section>`;
  }
  function services() {
    return `<section class="in-wide" aria-labelledby="in-svc-t"><h3 class="section-label" id="in-svc-t">By service</h3><div class="in-svcs" role="list">${N.services.map((s) => `<button class="in-svc l2" role="listitem" data-action="in-service" data-id="${s.id}" data-fk="svc-${s.id}">
      <span class="row"><b class="grow">${esc(s.id)}</b><span class="subtle num">${s.tests} tests</span></span>
      <span class="row"><b class="in-svc-v num">${s.rate}%</b>${SH.spark(s.trend, `${s.id} pass rate, last 12 runs`, 84, 22)}</span>
      <span class="row subtle num"><span class="grow">p95 ${s.p95} ms</span>${s.flaky ? `<span class="in-flaky-n">${ic('flaky', 'ico-sm')}${s.flaky} flaky</span>` : ''}</span></button>`).join('')}</div></section>`;
  }

  /* ---- screen ------------------------------------------------------------------- */
  const opt = (v, cur) => `<option value="${esc(v)}"${v === cur ? ' selected' : ''}>${esc(v)}</option>`;
  /* B: profile and branch sit behind one Filters button; range stays visible */
  function scope() {
    return `<div class="sh-filters" role="group" aria-label="Scope">
      ${SH.seg('Range', [['7', '7 days'], ['14', '14 days'], ['30', '30 days']], String(I.range), 'in-range')}
      <button class="btn btn-sm" data-action="in-filters" aria-haspopup="true" aria-label="Filters: profile ${esc(I.profile)}, branch ${esc(I.branch)}" data-fk="in-filters">${ic('filter')}<span class="mono">${esc(I.profile)} · ${esc(I.branch)}</span></button>
      <span class="grow"></span><span class="subtle hide-narrow">${I.range * 6} runs · Oct 25 – Nov 23</span></div>`;
  }
  function openFilters(focusId) {
    const anchor = $('[data-fk="in-filters"]');
    if (!anchor) return;
    VC.pick(anchor, `<div class="rn-filter in-filter" role="group" aria-label="Scope filters"><div class="form-grid">
      <div class="field"><label for="in-profile">Profile</label><select class="select" id="in-profile" data-fk="in-profile">${D.profiles.map((p) => opt(p.id, I.profile)).join('')}</select></div>
      <div class="field"><label for="in-branch">Branch</label><select class="select" id="in-branch" data-fk="in-branch">${['master', 's/checkout-v2', 'any'].map((b) => opt(b, I.branch)).join('')}</select></div></div></div>`);
    const f = focusId && $('#' + focusId); if (f) f.focus();
  }
  VC.screens.insights = {
    crumbs: () => [D.project.name, 'Insights'],
    status: () => '',
    render: () => `<div class="stage-one sh-scope"><section class="glass stage-one" aria-label="Insights">
      <div class="card-head"><div class="grow"><h2 class="card-title">Insights</h2><div class="card-sub" title="Same numbers as vc history --stats">Trends across stored runs.</div></div>
        ${VC.moreBtn('in-more', 'More insights actions')}</div>
      <div class="card-tabs">${scope()}</div>
      <div class="card-body" data-sk="in-body"><div class="in-page">${kpis()}<div class="in-two">${chartCard('pass')}${chartCard('dur')}</div>${flaky()}<div class="in-two">${slow()}${gaps()}</div>${skips()}${services()}</div></div>
    </section></div>`,
    mount(view) {
      SH.sizes(view);
      const figs = $$('.in-chart', view);
      figs.forEach(drawLine);
      if (!figs.length) return;
      let last = figs[0].clientWidth;
      const ro = new ResizeObserver(() => { const w = figs[0].clientWidth; if (Math.abs(w - last) > 2) { last = w; figs.forEach(drawLine); } });
      figs.forEach((f) => ro.observe(f));
      I.ro = ro;
    },
    unmount() { if (I.ro) I.ro.disconnect(); I.ro = null; }
  };

  A['in-view'] = (el) => { I[el.dataset.k] = el.dataset.v; VC.render(); };
  A['in-range'] = (el) => { I.range = +el.dataset.v; VC.render(); };
  A['in-slow'] = (el) => { I.slow = el.dataset.v; VC.render(); };
  A['in-filters'] = () => openFilters();
  A['in-more'] = (el) => VC.menu(el, VC.menuItem('Copy as CSV', 'in-export', '', { ico: 'copy', meta: `${I.range} days` }));
  A['in-export'] = () => VC.copy(['day,passRate,durationS'].concat(slice(N.days).map((d, i) => `${d},${slice(N.passRate)[i]},${slice(N.duration)[i]}`)).join('\n'), 'Copied as CSV', `${I.range} rows · pass rate and duration per day`);
  A['in-open-test'] = (el) => { VC.toast(`Opening ${el.dataset.id}`, 'In Tests, with its history strip'); VC.go('tests'); };
  A['in-service'] = (el) => { VC.toast(`Tests filtered to service:${el.dataset.id}`); VC.go('tests'); };
  A['in-quarantine'] = (el) => SH.quarantine(el.dataset.id, () => { I.quarantined[el.dataset.id] = '2026-12-07'; });
  A['in-scaffold'] = (el) => {
    const [svc, name] = el.dataset.api.split('#');
    VC.diffDialog({
      title: `Add a test for ${el.dataset.api}`, sub: 'A starting test that calls the api and checks its declared response. Edit it after writing.',
      file: `services/${svc}/${name}.test.yaml`, isNew: true, confirm: 'Write test',
      lines: [['@', '@@ new file @@'], ['+', 'tests:'], ['+', `  - id: ${name}sSuccessfully`], ['+', `    name: ${name} succeeds`], ['+', `    call: ${el.dataset.api}`], ['+', '    expect:'], ['+', '      status: 2xx'], ['+', `      body: { matches: ${el.dataset.api} }`]]
    });
  };
  A['in-extend'] = (el) => {
    const s = N.skips[+el.dataset.i], [file, test] = s.id.split('#'), [svc, area] = file.split('.');
    VC.diffDialog({
      title: `Extend ${s.kind} on ${test}`, sub: 'Keep the reason; push the date. Each extension shows in the file\'s git history.',
      fields: `<div class="field"><label for="in-until">New until</label><input class="input mono" id="in-until" value="2026-12-09"></div>`,
      file: `services/${svc}/${area}.test.yaml`, confirm: 'Write',
      lines: [['@', '@@ -12,4 +12,4 @@'], [' ', `  - id: ${test}`], [' ', `    ${s.kind}:`], [' ', `      reason: "${s.reason}"`], ['-', `      until: ${s.until}`], ['+', '      until: 2026-12-09']],
      after: () => { s.until = '2026-12-09'; s.left = 16; }
    });
  };
  A['in-unskip'] = (el) => {
    const i = +el.dataset.i, s = N.skips[i], [file, test] = s.id.split('#'), [svc, area] = file.split('.');
    VC.diffDialog({
      title: `${s.kind === 'skip' ? 'Unskip' : 'Remove allowFailure from'} ${test}`, sub: 'The test counts again from the next run.',
      file: `services/${svc}/${area}.test.yaml`, confirm: 'Write',
      lines: [['@', '@@ -12,5 +12,2 @@'], [' ', `  - id: ${test}`], ['-', `    ${s.kind}:`], ['-', `      reason: "${s.reason}"`], ['-', `      until: ${s.until}`], [' ', '    call: ' + svc + '#get']],
      after: () => { N.skips.splice(i, 1); }
    });
  };
  document.addEventListener('change', (e) => {
    if (e.target.id === 'in-profile' || e.target.id === 'in-branch') { I[e.target.id.slice(3)] = e.target.value; VC.render(); openFilters(e.target.id); }
  });
})();
