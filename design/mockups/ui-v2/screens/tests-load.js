/* Tests mode · Load (#tests-load): one load file, three views.
   Design: stages, executor, thresholds and the mix as forms; Save is a diff.
   Live: SVG charts (iterations/s, p50/p95/p99, error rate) with threshold lines,
   breach markers, a shared crosshair and a table view; a hosts panel.
   Report: breaches, latency histogram, per-scenario split, previous runs. */
(function () {
  'use strict';
  const VC = window.VC, D = VC.data, S = VC.state, A = VC.actions, TD = VC.testsData;
  const { $, $$, esc, ic, badge } = VC.h;
  const LD = TD.load, SER = LD.series, N = SER.length - 1, END = SER[N].t;
  const L = (VC.ld = { view: 'live', phase: 'done', i: N, hover: null, tbl: {}, timer: null, w: 560, dirty: {}, model: 'users', checks: LD.checks });
  const reduced = () => S.motion === 'reduce' || (S.motion === 'system' && matchMedia('(prefers-reduced-motion: reduce)').matches);
  const mmss = (t) => `${Math.floor(t / 60)}:${String(t % 60).padStart(2, '0')}`;
  const fmtN = (v) => (v >= 1000 ? v.toLocaleString('en-US') : String(v));
  const TH = { p99: 800, err: 1 };

  /* ---- chart geometry ------------------------------------------------------ */
  const H = 188, M = { l: 46, r: 70, t: 16, b: 26 };
  function nice(v) { const p = Math.pow(10, Math.floor(Math.log10(v))); const f = v / p; return (f <= 1 ? 1 : f <= 2 ? 2 : f <= 2.5 ? 2.5 : f <= 5 ? 5 : 10) * p; }
  const CH = {
    rps: { title: 'Iterations per second', sub: 'all mix tests', series: [['rps', 'iterations/s', 'one']], unit: '/s', max: () => nice(Math.max(...SER.map((p) => p.rps)) * 1.08) },
    lat: { title: 'Latency', sub: 'per iteration, from its scheduled start', series: [['p50', 'p50', 'p50'], ['p95', 'p95', 'p95'], ['p99', 'p99', 'p99']], unit: ' ms', max: () => nice(Math.max(TH.p99 * 1.25, ...SER.slice(0, L.i + 1).map((p) => p.p99 * 1.05))), th: { k: 'p99', v: TH.p99, label: 'p99 lt 800ms', fail: true } },
    err: { title: 'Error rate', sub: 'rolling 10 s', series: [['err', 'errorRate', 'one']], unit: '%', max: () => 1.25, th: { k: 'err', v: TH.err, label: 'errorRate lt 1% · abortOn', fail: false } }
  };
  function scales(c) {
    const ymax = c.max(), w = L.w;
    return { x: (t) => M.l + (t / END) * (w - M.l - M.r), y: (v) => M.t + (1 - v / ymax) * (H - M.t - M.b), ymax, w };
  }
  function runs(k, v) {
    const out = [];
    let s = -1;
    for (let i = 0; i <= L.i; i++) {
      const over = SER[i][k] > v;
      if (over && s < 0) s = i;
      if ((!over || i === L.i) && s >= 0) { out.push([s, over ? i : i - 1]); s = -1; }
    }
    return out;
  }
  function endLabels(c, sc) {
    const p = SER[L.i];
    const ls = c.series.map(([k, l]) => ({ k, l, y: sc.y(p[k]) })).sort((a, b) => a.y - b.y);
    for (let i = 1; i < ls.length; i++) if (ls[i].y - ls[i - 1].y < 13) ls[i].y = ls[i - 1].y + 13;
    const floor = sc.y(0) - 6;
    for (let i = ls.length - 1; i >= 0; i--) ls[i].y = Math.min(ls[i].y, i === ls.length - 1 ? floor : ls[i + 1].y - 13);
    return ls.map((o) => `<text class="ld-end" x="${sc.x(p.t) + 8}" y="${o.y + 4}">${o.l} ${fmtN(p[o.k])}</text>`).join('');
  }
  function chartSvg(id) {
    const c = CH[id], sc = scales(c), pts = SER.slice(0, L.i + 1);
    const ticks = [0, 0.25, 0.5, 0.75, 1].map((f) => f * sc.ymax);
    let g = ticks.map((v) => `<line class="ld-grid" x1="${M.l}" x2="${sc.w - M.r}" y1="${sc.y(v)}" y2="${sc.y(v)}"/><text class="ld-ax" x="${M.l - 6}" y="${sc.y(v) + 4}" text-anchor="end">${fmtN(+v.toFixed(2))}</text>`).join('');
    g += [0, 60, 120, 180, 240, 300, 360, 420].map((t) => `<text class="ld-ax" x="${sc.x(t)}" y="${H - 8}" text-anchor="middle">${mmss(t)}</text>`).join('');
    g += `<line class="ld-base" x1="${M.l}" x2="${sc.w - M.r}" y1="${sc.y(0)}" y2="${sc.y(0)}"/>`;
    if (c.th) {
      const y = sc.y(c.th.v);
      runs(c.th.k, c.th.v).forEach(([a, b]) => { g += `<rect class="ld-band${c.th.fail ? '' : ' is-warn'}" x="${sc.x(SER[a].t)}" y="${M.t}" width="${Math.max(2, sc.x(SER[b].t) - sc.x(SER[a].t))}" height="${H - M.t - M.b}"/>`; });
      g += `<line class="ld-th" x1="${M.l}" x2="${sc.w - M.r}" y1="${y}" y2="${y}"/><text class="ld-thl" x="${M.l + 6}" y="${y - 5}">${esc(c.th.label)}</text>`;
    }
    c.series.forEach(([k, , col]) => { if (pts.length > 1) g += `<path class="ld-line s-${col}" d="${pts.map((p, i) => `${i ? 'L' : 'M'}${sc.x(p.t).toFixed(1)},${sc.y(p[k]).toFixed(1)}`).join('')}"/>`; });
    if (c.th && c.th.fail) {
      runs(c.th.k, c.th.v).forEach(([a, b]) => {
        let pk = a; for (let i = a; i <= b; i++) if (SER[i][c.th.k] > SER[pk][c.th.k]) pk = i;
        const x = sc.x(SER[pk].t), y = sc.y(SER[pk][c.th.k]);
        g += `<circle class="ld-breach" cx="${x}" cy="${y}" r="5"/><text class="ld-bl" x="${x}" y="${Math.max(M.t + 10, y - 10)}" text-anchor="middle">✕ breach ${mmss(SER[a].t)}–${mmss(SER[b].t)}</text>`;
      });
    }
    if (c.series.length > 1) g += endLabels(c, sc);
    else { const p = SER[L.i]; g += `<circle class="ld-dot s-${c.series[0][2]}" cx="${sc.x(p.t)}" cy="${sc.y(p[c.series[0][0]])}" r="4"/>`; }
    g += `<g class="ld-xh" data-c="${id}">${xhair(id, sc)}</g><rect class="ld-hit" data-c="${id}" x="${M.l}" y="${M.t}" width="${sc.w - M.l - M.r}" height="${H - M.t - M.b}"/>`;
    return `<svg class="ld-svg" viewBox="0 0 ${sc.w} ${H}" width="${sc.w}" height="${H}" aria-hidden="true">${g}</svg>`;
  }
  function xhair(id, sc) {
    if (L.hover == null || L.hover > L.i) return '';
    const p = SER[L.hover], x = sc.x(p.t);
    return `<line class="ld-xline" x1="${x}" x2="${x}" y1="${M.t}" y2="${H - M.b}"/>${CH[id].series.map(([k, , col]) => `<circle class="ld-dot s-${col}" cx="${x}" cy="${sc.y(p[k])}" r="4"/>`).join('')}`;
  }
  function tipHtml(id) {
    if (L.hover == null || L.hover > L.i) return '';
    const p = SER[L.hover], c = CH[id];
    return `<b class="mono">${mmss(p.t)} · ${fmtN(p.users)} users</b>${c.series.map(([k, l, col]) => `<span class="ld-tr"><i class="ld-sw s-${col}"></i>${l}<b class="num">${fmtN(p[k])}${c.unit}</b>${c.th && c.th.k === k && p[k] > c.th.v ? `<span class="ld-over">${ic('fail', 'ico-sm')}over</span>` : ''}</span>`).join('')}`;
  }
  function tableHtml(id) {
    const c = CH[id], rows = SER.filter((p, i) => i <= L.i && i % 6 === 0);
    return `<div class="table-wrap glass-dense ld-tbl"><table class="table"><caption class="sr-only">${esc(c.title)} every 30 s</caption><thead><tr><th>Time</th><th class="r">Users</th>${c.series.map(([, l]) => `<th class="r">${l}</th>`).join('')}</tr></thead><tbody>${rows.map((p) => `<tr><td class="mono">${mmss(p.t)}</td><td class="r num">${fmtN(p.users)}</td>${c.series.map(([k]) => `<td class="r num${c.th && c.th.k === k && p[k] > c.th.v ? ' ld-bad' : ''}">${fmtN(p[k])}${c.unit}</td>`).join('')}</tr>`).join('')}</tbody></table></div>`;
  }
  function chartCard(id) {
    const c = CH[id], p = SER[L.i];
    const leg = c.series.length > 1 ? `<ul class="ld-legend" aria-label="Series">${c.series.map(([, l, col]) => `<li><i class="ld-sw s-${col}"></i>${l}</li>`).join('')}${c.th ? '<li><i class="ld-sw is-th"></i>threshold</li>' : ''}</ul>` : '';
    const br = c.th && c.th.fail ? runs(c.th.k, c.th.v) : [];
    return `<figure class="glass ld-chart" data-c="${id}" aria-labelledby="ldc-${id}"><div class="card-head"><div class="grow"><h3 class="card-title" id="ldc-${id}">${c.title}</h3><span class="card-sub">${c.sub} · now ${fmtN(p[c.series[c.series.length - 1][0]])}${c.unit}</span></div></div>
      ${leg}${L.tbl.live ? tableHtml(id) : `<div class="ld-plot" id="ldp-${id}" tabindex="0" role="img" aria-label="${esc(c.title)} over ${mmss(SER[L.i].t)}; use arrow keys to read values; Table shows every 30 s">${chartSvg(id)}<div class="ld-tip l3" id="ldtip-${id}" role="status">${tipHtml(id)}</div></div>`}
      ${br.length ? `<figcaption class="ld-cap"><span class="badge badge-sm st-fail">${ic('fail')}breach</span>p99 over 800 ms from ${br.map(([a, b]) => `${mmss(SER[a].t)} to ${mmss(SER[b].t)}`).join(', ')} (rolling 10 s)</figcaption>` : c.th ? `<figcaption class="ld-cap">${ic('pass', 'ico-sm')}Held on the rolling window · peak ${Math.max(...SER.slice(0, L.i + 1).map((q) => q.err))}%</figcaption>` : ''}</figure>`;
  }

  /* ---- live view ----------------------------------------------------------- */
  function stage(t) { let acc = 0; for (let i = 0; i < LD.stages.length; i++) { acc += LD.stages[i].duration; if (t <= acc) return [i, LD.stages[i]]; } return [LD.stages.length - 1, LD.stages[LD.stages.length - 1]]; }
  const peak = (k) => SER.slice(0, L.i + 1).reduce((a, q) => (q[k] > a[k] ? q : a), SER[0]);
  function kpis() {
    const p = SER[L.i], [si, st] = stage(p.t), tot = SER.slice(0, L.i + 1).reduce((s, q) => s + q.rps * 5, 0);
    const ended = L.phase === 'done' || L.phase === 'stopped', pu = peak('users'), pr = peak('rps');
    const k = (lab, v, sub, bad) => `<li class="ld-kpi${bad ? ' is-bad' : ''}"><span class="ld-kl">${lab}</span><b class="num">${v}</b><span class="ld-ks">${sub}</span></li>`;
    const users = ended ? k('Peak users', fmtN(pu.users), `at ${mmss(pu.t)} · now ${fmtN(p.users)}`) : k('Users', fmtN(p.users), `${si === 0 || st.users > (LD.stages[si - 1] || {}).users ? 'ramping to' : 'down to'} ${fmtN(st.users)}`);
    const rate = ended ? k('Peak iterations/s', fmtN(pr.rps), `${fmtN(Math.round(tot))} iterations in all`) : k('Iterations/s', fmtN(p.rps), `${fmtN(Math.round(tot))} iterations`);
    return `<ul class="ld-kpis" aria-label="${ended ? 'Run summary' : 'Now'}">${k('Elapsed', mmss(p.t), `of ${mmss(END)} · stage ${si + 1} of ${LD.stages.length}`)}${users}${rate}${k('p99', `${p.p99} ms`, 'limit 800 ms', p.p99 > TH.p99)}${k('Error rate', `${p.err}%`, 'limit 1% · abortOn')}</ul>`;
  }
  function hostsHtml() {
    const p = SER[L.i], drop = SER.slice(0, L.i + 1).reduce((s, q) => s + q.dropped, 0);
    const spark = (k, max) => { const pts = SER.slice(0, L.i + 1); return `<svg class="ld-spark" viewBox="0 0 120 28" aria-hidden="true"><path class="ld-line s-one" d="${pts.map((q, i) => `${i ? 'L' : 'M'}${((q.t / END) * 118 + 1).toFixed(1)},${(26 - (q[k] / max) * 24).toFixed(1)}`).join('')}"/></svg>`; };
    const row = (l, v, k, max, hint, warn) => `<tr${warn ? ' class="is-warn"' : ''}><th scope="row">${l}</th><td class="r num">${v}</td><td class="hide-narrow">${spark(k, max)}</td><td class="subtle">${hint}</td></tr>`;
    return `<section class="glass ld-hosts" aria-labelledby="ld-h-t"><div class="card-head"><h3 class="card-title" id="ld-h-t">Hosts</h3><span class="card-sub">runner-1 · this machine · 8 cores</span></div>
      <div class="card-body"><div class="table-wrap glass-dense"><table class="table"><thead><tr><th>Metric</th><th class="r">Now</th><th class="hide-narrow">Run</th><th>Note</th></tr></thead><tbody>
      ${row('CPU', `${p.cpu}%`, 'cpu', 100, p.cpu > 70 ? 'high; results stay valid below 85%' : 'headroom ok', p.cpu > 70)}${row('Memory', `${fmtN(p.mem)} MB`, 'mem', 1600, 'of 16 GB')}${row('Connections', fmtN(p.conns), 'conns', 3200, 'keep-alive pool')}${row('Dropped iterations', fmtN(drop), 'dropped', 50, drop ? 'started late; latency counts from the scheduled start' : 'none', drop > 0)}</tbody></table></div>
      ${drop ? `<p class="callout" data-tone="warn">${ic('warn')}<span><b>${fmtN(drop)} iterations started late</b> between 4:50 and 5:50: the runner fell behind. Latency is timed from each iteration's scheduled start, so the delay shows in p99 instead of hiding.</span></p>` : ''}</div></section>`;
  }
  function liveHtml() {
    const running = L.phase === 'running' || L.phase === 'stopping';
    return `<div class="ld-live" id="ld-live"><section class="glass ld-now" aria-label="Load run">${kpis()}<progress class="tr-prog ld-prog${L.phase === 'stopping' ? ' is-stopping' : ''}" max="${N}" value="${L.i}" aria-label="Load run progress">${Math.round((L.i / N) * 100)}%</progress></section>
      ${running && reduced() ? `<div class="row"><span class="subtle">Reduced motion: the run advances when you step.</span><button class="btn btn-sm" data-action="ld-step" data-fk="ld-step">${ic('arrow-right', 'ico-sm')}Step 30 s</button><button class="btn btn-sm" data-action="ld-finish">Finish</button></div>` : ''}
      <div class="ld-charts">${chartCard('lat')}${chartCard('rps')}${chartCard('err')}${hostsHtml()}</div></div>`;
  }

  /* ---- report view --------------------------------------------------------- */
  function histogram() {
    const hs = LD.histogram, max = Math.max(...hs.map((h) => h[1])), w = 560, h = 200, l = 50, b = 30, bw = (w - l - 10) / hs.length;
    const y = (v) => 10 + (1 - v / max) * (h - b - 10);
    const lim = hs.findIndex((x) => x[0] > 800);
    let g = [0, 0.5, 1].map((f) => `<line class="ld-grid" x1="${l}" x2="${w - 10}" y1="${y(f * max)}" y2="${y(f * max)}"/><text class="ld-ax" x="${l - 6}" y="${y(f * max) + 4}" text-anchor="end">${f ? `${Math.round((f * max) / 1000)}k` : 0}</text>`).join('');
    g += hs.map(([ms, n], i) => `<g class="ld-bar${ms > 800 ? ' is-over' : ''}" tabindex="0" role="listitem" aria-label="up to ${ms} ms: ${fmtN(n)} iterations"><rect x="${l + i * bw + 1}" y="${y(n)}" width="${bw - 2}" height="${Math.max(1, y(0) - y(n))}" rx="2"/><text class="ld-ax" x="${l + i * bw + bw / 2}" y="${h - 12}" text-anchor="middle">${ms >= 1000 ? ms / 1000 + 's' : ms}</text><text class="ld-bv" x="${l + i * bw + bw / 2}" y="${Math.max(20, y(n) - 6)}" text-anchor="middle">${fmtN(n)}</text></g>`).join('');
    const xl = l + lim * bw;
    g += `<line class="ld-th" x1="${xl}" x2="${xl}" y1="10" y2="${h - b}"/><text class="ld-thl" x="${xl + 4}" y="20">p99 limit 800 ms</text>`;
    return `<figure class="glass ld-chart" aria-labelledby="ld-hist-t"><div class="card-head"><div class="grow"><h3 class="card-title" id="ld-hist-t">Latency histogram</h3><span class="card-sub">iterations per bucket (upper bound, ms) · ${fmtN(hs.reduce((s, x) => s + x[1], 0))} in all</span></div></div>
      <div class="ld-plot"><svg class="ld-svg is-fluid" viewBox="0 0 ${w} ${h}" role="list" aria-label="Latency histogram">${g}</svg></div>
      <figcaption class="ld-cap">${fmtN(hs.filter((x) => x[0] > 800).reduce((s, x) => s + x[1], 0))} iterations (1.0%) slower than 800 ms · hover or focus a bar for its count</figcaption></figure>`;
  }
  function previousHtml() {
    const P = LD.previous, max = 1000, w = 560, rowH = 30, l = 150, h = P.length * rowH + 24, x = (v) => l + (v / max) * (w - l - 60);
    let g = P.map((r, i) => `<g class="ld-hbar${r.cur ? ' is-cur' : ''}"><text class="ld-ax is-name" x="${l - 8}" y="${i * rowH + 19}" text-anchor="end">${esc(r.run)}${r.cur ? ' (this)' : ''}</text><rect x="${l}" y="${i * rowH + 6}" width="${x(r.p99) - l}" height="18" rx="3"/><text class="ld-end" x="${x(r.p99) + 6}" y="${i * rowH + 19}">${r.p99} ms</text></g>`).join('');
    g += `<line class="ld-th" x1="${x(800)}" x2="${x(800)}" y1="2" y2="${h - 18}"/><text class="ld-thl" x="${x(800)}" y="${h - 4}" text-anchor="middle">limit 800</text>`;
    return `<figure class="glass ld-chart" aria-labelledby="ld-prev-t"><div class="card-head"><div class="grow"><h3 class="card-title" id="ld-prev-t">Compared with previous runs</h3><span class="card-sub">p99 per run of checkout.peak</span></div></div>
      ${L.tbl.report ? `<div class="table-wrap glass-dense ld-tbl"><table class="table"><thead><tr><th>Run</th><th>Branch</th><th class="r">p99</th><th class="r">it/s</th><th class="r">errors</th><th>Verdict</th></tr></thead><tbody>${P.map((r) => `<tr${r.cur ? ' aria-selected="true"' : ''}><td>${esc(r.run)}</td><td class="mono">${esc(r.branch)}</td><td class="r num">${r.p99} ms</td><td class="r num">${fmtN(r.rps)}</td><td class="r num">${r.err}%</td><td>${badge(r.verdict, null, true)}</td></tr>`).join('')}</tbody></table></div>`
        : `<div class="ld-plot"><svg class="ld-svg is-fluid" viewBox="0 0 ${w} ${h}" role="img" aria-label="p99 by run: ${P.map((r) => `${r.run} ${r.p99} ms`).join(', ')}">${g}</svg></div>`}
      <figcaption class="ld-cap">p99 is ${P[0].p99 - P[1].p99} ms slower than the last master run (${P[1].run}); iterations/s and errors are within 3%.</figcaption></figure>`;
  }
  function reportHtml() {
    const tot = LD.scenarios.reduce((s, x) => s + x.iters, 0);
    return `<div class="ld-report"><p class="callout" data-tone="danger">${ic('fail')}<span><b>${LD.id} — ${esc(LD.name)} · finished after ${mmss(END)} · breached p99 at 862 ms (lt 800 ms).</b> Exit code 3. 1 of 4 thresholds failed; afterAll and the cleanups ran.</span><span class="grow"></span><button class="btn btn-sm" data-action="ld-open-report">${ic('external', 'ico-sm')}Open HTML report</button></p>
      <section class="glass" aria-labelledby="ld-br-t"><div class="card-head"><h3 class="card-title" id="ld-br-t">Thresholds</h3><span class="card-sub">rolling windows are 10 s; the verdict uses the whole run</span></div>
        <div class="card-body"><div class="table-wrap glass-dense"><table class="table"><thead><tr><th>Verdict</th><th>Metric</th><th>Scope</th><th>Limit</th><th>Worst window</th><th class="hide-narrow">When</th><th>Whole run</th></tr></thead><tbody>
        ${LD.breaches.map((b) => `<tr><td>${badge(b.st, b.st === 'fail' ? 'breached' : 'held', true)}</td><td class="mono">${b.metric}</td><td class="mono ellipsis">${esc(b.scope)}</td><td class="mono">${esc(b.limit)}</td><td class="num">${esc(b.value)}</td><td class="mono hide-narrow">${esc(b.at)}</td><td>${esc(b.final)}</td></tr>`).join('')}</tbody></table></div></div></section>
      <div class="ld-charts">${histogram()}${previousHtml()}</div>
      <section class="glass" aria-labelledby="ld-sc-t"><div class="card-head"><h3 class="card-title" id="ld-sc-t">Per scenario</h3><span class="card-sub">${fmtN(tot)} iterations · checks: ${LD.checks}</span></div>
        <div class="card-body"><div class="table-wrap glass-dense"><table class="table"><thead><tr><th>Test</th><th class="r">Weight</th><th class="r">Iterations</th><th class="r">p50</th><th class="r">p95</th><th class="r">p99</th><th class="r">Errors</th><th>Verdict</th></tr></thead><tbody>
        ${LD.scenarios.map((s) => `<tr><td class="mono"><a href="#tests" data-action="ld-open-test" data-id="${esc(s.test)}">${esc(s.test)}</a></td><td class="r num">${s.weight}</td><td class="r num">${fmtN(s.iters)}</td><td class="r num">${s.p50}</td><td class="r num">${s.p95}</td><td class="r num${s.p99 > 800 ? ' ld-bad' : ''}">${s.p99}</td><td class="r num${s.err > 1 ? ' ld-bad' : ''}">${s.err}%</td><td>${badge(s.verdict, null, true)}</td></tr>`).join('')}</tbody></table></div>
        <p class="hint subtle">placesOrder is 10% of the mix and 61% of iterations over 800 ms. Its own threshold (p95 lt 1200ms) held at 1088 ms.</p></div></section></div>`;
  }

  /* ---- design view (forms) ------------------------------------------------- */
  const inp = (key, v, label, cls) => `<input class="input input-sm mono${cls ? ' ' + cls : ''}${L.dirty[key] ? ' is-dirty' : ''}" data-ld="${key}" data-orig="${esc(v)}" value="${esc(L.dirty[key] ? L.dirty[key][1] : v)}" aria-label="${esc(label)}" data-fk="ldi-${key}">`;
  function stagePreview() {
    const w = 560, h = 90, tot = LD.stages.reduce((s, x) => s + x.duration, 0), max = Math.max(...LD.stages.map((s) => s.users));
    let t = 0, u = 0;
    const pts = [[0, 0]].concat(LD.stages.map((s) => { t += s.duration; u = s.users; return [t, u]; }));
    const x = (v) => 40 + (v / tot) * (w - 50), y = (v) => 8 + (1 - v / max) * (h - 26);
    return `<svg class="ld-svg is-fluid ld-stages" viewBox="0 0 ${w} ${h}" role="img" aria-label="Users over time: ${LD.stages.map((s) => `${s.users} users over ${s.duration} s`).join(', ')}"><line class="ld-base" x1="40" x2="${w - 10}" y1="${y(0)}" y2="${y(0)}"/><text class="ld-ax" x="34" y="${y(max) + 4}" text-anchor="end">${fmtN(max)}</text><text class="ld-ax" x="34" y="${y(0) + 4}" text-anchor="end">0</text>
      <path class="ld-area" d="M${x(0)},${y(0)}${pts.map(([a, b]) => `L${x(a)},${y(b)}`).join('')}L${x(tot)},${y(0)}Z"/><path class="ld-line s-one" d="${pts.map(([a, b], i) => `${i ? 'L' : 'M'}${x(a)},${y(b)}`).join('')}"/>${pts.slice(1).map(([a, b], i) => `<circle class="ld-dot s-p95" cx="${x(a)}" cy="${y(b)}" r="4"><title>stage ${i + 1}: ${b} users at ${mmss(a)}</title></circle><text class="ld-ax" x="${x(a)}" y="${h - 4}" text-anchor="middle">${mmss(a)}</text>`).join('')}</svg>`;
  }
  function designHtml() {
    let prev = 0;
    const stages = LD.stages.map((s, i) => { const shape = s.users === prev ? 'hold' : s.users > prev ? `ramp up ${fmtN(prev)} → ${fmtN(s.users)}` : `ramp down → ${fmtN(s.users)}`; prev = s.users; return `<tr><td class="r mono muted">${i + 1}</td><td>${inp(`stages.${i}.duration`, mmss(s.duration).replace(/^(\d+):00$/, '$1m'), `Stage ${i + 1} duration`)}</td><td>${inp(`stages.${i}.users`, String(s.users), `Stage ${i + 1} users`)}</td><td class="subtle">${shape}</td><td><button class="btn btn-ghost btn-icon btn-sm" data-action="ld-rm-stage" data-i="${i}" aria-label="Remove stage ${i + 1}">${ic('x')}</button></td></tr>`; }).join('');
    const ths = LD.thresholds.map((t, i) => `<tr><td><select class="select input-sm mono" aria-label="Metric">${['p50', 'p90', 'p95', 'p99', 'max', 'errorRate', 'rps'].map((m) => `<option${m === t.metric ? ' selected' : ''}>${m}</option>`).join('')}</select></td><td><select class="select input-sm mono" aria-label="Matcher">${['lt', 'lte', 'gt', 'gte'].map((m) => `<option${m === t.op ? ' selected' : ''}>${m}</option>`).join('')}</select></td><td>${inp(`th.${i}`, `${t.v}${t.unit}`, `${t.metric} limit`)}</td><td class="mono ellipsis">${t.test ? esc(t.test) : '<span class="subtle">whole mix</span>'}</td>
      <td><label class="switch"><input type="checkbox" ${LD.abortOn.includes(t.metric) ? 'checked' : ''} ${t.test ? 'disabled' : ''} aria-label="Abort when ${t.metric} breaches"><span class="track"></span></label></td></tr>`).join('');
    const tw = LD.mix.reduce((s, m) => s + m.weight, 0);
    const mix = LD.mix.map((m, i) => `<tr><td class="mono">${esc(m.test)}</td><td>${inp(`mix.${i}.weight`, String(m.weight), `${m.test} weight`, 'ld-num')}</td><td><meter class="ld-meter" min="0" max="100" value="${Math.round((m.weight / tw) * 100)}" aria-label="share">${Math.round((m.weight / tw) * 100)}%</meter><span class="num subtle"> ${Math.round((m.weight / tw) * 100)}%</span></td><td class="mono subtle">${m.p95 ? `p95 lt ${m.p95}` : '—'}</td></tr>`).join('');
    return `<div class="ld-design">
      <section class="glass" aria-labelledby="ld-st-t"><div class="card-head"><h3 class="card-title" id="ld-st-t">Stages</h3><span class="card-sub">users ramp from the previous stage's count</span><span class="grow"></span><button class="btn btn-sm btn-ghost" data-action="ld-add-stage" data-v="ramp">${ic('plus', 'ico-sm')}Ramp</button><button class="btn btn-sm btn-ghost" data-action="ld-add-stage" data-v="hold">${ic('plus', 'ico-sm')}Hold</button></div>
        <div class="card-body">${stagePreview()}<div class="table-wrap glass-dense"><table class="table ld-form"><thead><tr><th class="r">#</th><th>duration</th><th>users</th><th>Shape</th><th><span class="sr-only">Remove</span></th></tr></thead><tbody>${stages}</tbody></table></div></div></section>
      <section class="glass" aria-labelledby="ld-ex-t"><div class="card-head"><h3 class="card-title" id="ld-ex-t">Executor</h3></div><div class="card-body ld-exec">
        <div class="field"><span class="ed-lbl">Model</span><div class="seg seg-sm" role="group" aria-label="Executor">${[['users', 'Users (closed)'], ['rate', 'Arrival rate (open)']].map(([k, l]) => `<button aria-pressed="${L.model === k}" data-action="ld-model" data-v="${k}">${l}</button>`).join('')}</div><span class="hint">${L.model === 'users' ? 'Each user runs one mix test, waits thinkTime, repeats. The stages set the user count.' : 'Not in this version: open-model stages are a proposal. The file keeps users.'}</span></div>
        <div class="field"><label for="ld-tmin">thinkTime min</label>${inp('think.min', LD.thinkTime.min, 'thinkTime min')}</div><div class="field"><label for="ld-tmax">thinkTime max</label>${inp('think.max', LD.thinkTime.max, 'thinkTime max')}</div>
        <div class="field"><span class="ed-lbl">checks</span><div class="seg seg-sm" role="group" aria-label="Checks">${[['api', 'api'], ['full', 'full']].map(([k, l]) => `<button aria-pressed="${L.checks === k}" data-action="ld-checks" data-v="${k}">${l}</button>`).join('')}</div><span class="hint">${L.checks === 'api' ? 'Only the apis’ own expect; fastest.' : 'Every test’s expect; slower runner.'}</span></div></div></section>
      <section class="glass" aria-labelledby="ld-th-t"><div class="card-head"><h3 class="card-title" id="ld-th-t">Thresholds</h3><span class="card-sub">abortOn stops early on the rolling window</span><span class="grow"></span><button class="btn btn-sm btn-ghost" data-action="noop">${ic('plus', 'ico-sm')}Threshold</button></div>
        <div class="card-body"><div class="table-wrap glass-dense"><table class="table ld-form"><thead><tr><th>Metric</th><th>Matcher</th><th>Limit</th><th>Scope</th><th>abortOn</th></tr></thead><tbody>${ths}</tbody></table></div></div></section>
      <section class="glass" aria-labelledby="ld-mx-t"><div class="card-head"><h3 class="card-title" id="ld-mx-t">Mix · scenarios</h3><span class="card-sub">tests chosen by weight</span><span class="grow"></span><button class="btn btn-sm btn-ghost" data-action="noop">${ic('plus', 'ico-sm')}Test</button></div>
        <div class="card-body"><div class="table-wrap glass-dense"><table class="table ld-form"><thead><tr><th>Test</th><th>weight</th><th>Share</th><th>Own thresholds</th></tr></thead><tbody>${mix}</tbody></table></div></div></section></div>`;
  }

  /* ---- screen -------------------------------------------------------------- */
  function headHtml() {
    const running = L.phase === 'running' || L.phase === 'stopping';
    const st = running ? badge('running', L.phase === 'stopping' ? 'stopping' : 'running', true) : L.phase === 'stopped' ? badge('skipped', 'stopped · exit 130', true) : badge('fail', 'breached · exit 3', true);
    const nd = Object.keys(L.dirty).length;
    return `<div class="ld-head on-l0"><div class="grow row ld-title"><h1 class="card-title mono ellipsis" title="${LD.path}">${LD.id}</h1>${st}<span class="subtle ellipsis ld-sub" title="${esc(LD.name)} · profile ${esc(S.profile)}">${esc(LD.name)} · profile ${esc(S.profile)}</span></div>
      ${L.view === 'design' ? '' : `<button class="btn btn-ghost btn-sm" data-action="ld-tbl" aria-pressed="${!!L.tbl[L.view]}" data-fk="ld-tbl" title="Show the charts as tables" aria-label="Table">${ic('rows', 'ico-sm')}<span class="hide-narrow">Table</span></button>`}
      <div class="seg seg-sm" role="group" aria-label="View">${[['design', 'Design'], ['live', 'Live'], ['report', 'Report']].map(([k, l]) => `<button aria-pressed="${L.view === k}" data-action="ld-view" data-v="${k}" data-fk="ldv-${k}">${l}</button>`).join('')}</div>
      ${L.view === 'design' ? `<button class="btn btn-sm" data-action="ld-save" data-fk="ld-save"${nd ? ` title="${nd} unsaved"` : ''}>${ic('check', 'ico-sm')}Save${nd ? ` (${nd})` : ''}</button>` : ''}
      ${running ? `<button class="btn btn-sm btn-danger" data-action="ld-stop" data-fk="ld-stop"${L.phase === 'stopping' ? ' disabled' : ''}>${ic('stop', 'ico-sm')}Stop</button>` : `<button class="btn btn-sm btn-primary${S.profile === 'prod' ? ' btn-danger' : ''}" data-action="ld-run" data-fk="ld-run">${ic('play', 'ico-sm')}Run load<span class="kbd hide-narrow">⌘R</span></button>`}</div>`;
  }
  VC.screens['tests-load'] = {
    enter: () => { S.mode = 'tests'; },
    crumbs: () => [D.project.name, 'Tests', 'Load', LD.id],
    status: () => '',
    render: () => `${headHtml()}<div class="ld-body" data-sk="ld-body">${L.view === 'design' ? designHtml() : L.view === 'report' ? reportHtml() : liveHtml()}</div>`,
    mount: (view) => {
      const c = $('.ld-chart[data-c] .ld-plot', view);
      if (c) { const w = Math.round(c.clientWidth); if (w > 200 && Math.abs(w - L.w) > 4) { L.w = w; paint(); } }
    },
    unmount: () => {}
  };
  function paint() {
    const live = $('#ld-live');
    if (!live) return;
    const k = document.activeElement && document.activeElement.id;
    live.outerHTML = liveHtml();
    if (k) { const el = document.getElementById(k); if (el) el.focus({ preventScroll: true }); }
    const hp = L.hover != null && L.hc && $(`#ldp-${L.hc}`);
    if (hp) { hp.classList.add('is-hover'); placeTip(L.hc); }
    const h = $('.ld-head');
    if (h) h.outerHTML = headHtml();
  }
  function paintHover() {
    Object.keys(CH).forEach((id) => {
      const g = $(`.ld-xh[data-c="${id}"]`), tip = $(`#ldtip-${id}`);
      if (g) g.innerHTML = xhair(id, scales(CH[id]));
      if (tip) tip.innerHTML = tipHtml(id);
    });
  }
  new ResizeObserver(() => { if (S.route !== 'tests-load' || L.view !== 'live') return; const c = $('.ld-chart[data-c] .ld-plot'); if (c && Math.abs(c.clientWidth - L.w) > 4) { L.w = Math.round(c.clientWidth); paint(); } }).observe(document.documentElement);

  /* ---- run lifecycle ------------------------------------------------------- */
  function tick(n) {
    L.i = Math.min(N, L.i + (n || 1));
    if (L.phase === 'stopping' && SER[L.i].users === 0) L.i = N;
    if (L.i >= N) return finish(L.phase === 'stopping' ? 'stopped' : 'done');
    paint();
  }
  function finish(ph) {
    clearInterval(L.timer); L.timer = null; L.phase = ph;
    if (ph === 'done') { L.view = 'report'; VC.render(); VC.toast('Load run finished · exit 3', 'breached p99 at 862 ms (lt 800 ms) · report ready', 'warn'); }
    else { VC.render(); VC.toast('Load run stopped · exit 130', 'afterAll and the cleanups ran; partial results are in the report'); }
  }
  function start() {
    VC.closeDialog();
    L.i = 0; L.phase = 'running'; L.view = 'live'; L.hover = null;
    VC.render();
    if (!reduced()) L.timer = setInterval(() => tick(1), 160);
  }
  A['ld-run'] = () => {
    if (L.phase === 'running' || L.phase === 'stopping') return;
    if (S.route !== 'tests-load') VC.go('tests-load');
    const prod = S.profile === 'prod';
    VC.dialog(`<div class="dlg dlg-narrow"><div class="dlg-head"><div class="grow"><h2 id="dlg-title">Run ${LD.id}</h2><div class="sub">Same as <code>vc load ${LD.id} --profile ${esc(S.profile)}</code></div></div><button class="btn btn-ghost btn-icon" data-action="close-dialog" aria-label="Close">${ic('x')}</button></div>
      <div class="dlg-body"><div class="kv glass-dense"><div class="kv-row"><span class="k">target</span><span class="v mono">${esc((D.profiles.find((p) => p.id === S.profile) || D.profiles[0]).baseUrl)}</span></div><div class="kv-row"><span class="k">peak</span><span class="v">5,000 users · ${mmss(END)}</span></div><div class="kv-row"><span class="k">before</span><span class="v">beforeRun, then beforeAll of 3 files</span></div></div>
        ${prod ? `<p class="callout" data-tone="danger">${ic('warn')}<span><b>This sends about 1.2 million requests to live prod.</b> Type the profile name to confirm.</span></p><div class="field"><label for="ld-conf">Profile name</label><input class="input mono" id="ld-conf" autocomplete="off" placeholder="prod"></div>` : ''}</div>
      <div class="dlg-foot"><button class="btn" data-action="close-dialog">Cancel</button><button class="btn btn-primary${prod ? ' btn-danger' : ''}" data-action="ld-go" id="ld-go"${prod ? ' disabled' : ''}>${ic('play')}Start</button></div></div>`);
    const c = $('#ld-conf');
    if (c) c.addEventListener('input', () => { $('#ld-go').disabled = c.value !== 'prod'; });
  };
  A['ld-go'] = start;
  A['ld-stop'] = () => { L.phase = 'stopping'; paint(); VC.toast('Stopping', 'No new iterations; in-flight ones finish, then afterAll and the cleanups run'); if (!L.timer && !reduced()) L.timer = setInterval(() => tick(1), 160); L.i = Math.max(L.i, N - 12); };
  A['ld-step'] = () => tick(6);
  A['ld-finish'] = () => tick(N);
  A['ld-view'] = (el) => { L.view = el.dataset.v; L.hover = null; VC.render(); };
  A['ld-tbl'] = () => { L.tbl[L.view] = !L.tbl[L.view]; VC.render(); };
  A['ld-open-report'] = () => VC.toast('Opened report.html', 'reports/load/checkout.peak-2026-11-23T1420.html');
  A['ld-open-test'] = (el) => { VC.tx.sel = el.dataset.id; VC.go('tests'); };
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

  /* ---- hover + keyboard crosshair (shared across the three charts) ---------- */
  function idxAt(ev, rect) {
    const r = rect.getBoundingClientRect(), f = Math.min(1, Math.max(0, (ev.clientX - r.left) / r.width));
    return Math.min(L.i, Math.round((f * END) / 5));
  }
  function placeTip(id) {
    const plot = $(`#ldp-${id}`), tip = $(`#ldtip-${id}`);
    if (!plot || !tip || L.hover == null) return;
    const sc = scales(CH[id]), x = sc.x(SER[L.hover].t);
    tip.style.setProperty('--tx', `${Math.round(x > sc.w / 2 ? x - 12 : x + 12)}px`);
    tip.classList.toggle('is-left', x > sc.w / 2);
  }
  document.addEventListener('pointermove', (e) => {
    const hit = e.target.closest && e.target.closest('.ld-hit');
    if (!hit) return;
    const i = idxAt(e, hit);
    if (i === L.hover) return;
    L.hover = i; L.hc = hit.dataset.c;
    $$('.ld-plot.is-hover').forEach((p) => p.classList.remove('is-hover'));
    hit.closest('.ld-plot').classList.add('is-hover');
    paintHover(); placeTip(hit.dataset.c);
  });
  document.addEventListener('pointerout', (e) => { if (e.target.closest && e.target.closest('.ld-hit') && !(e.relatedTarget && e.relatedTarget.closest && e.relatedTarget.closest('.ld-hit'))) { L.hover = null; $$('.ld-plot.is-hover').forEach((p) => p.classList.remove('is-hover')); paintHover(); } });
  document.addEventListener('keydown', (e) => {
    const p = e.target.closest && e.target.closest('.ld-plot[id]');
    if (!p || !['ArrowLeft', 'ArrowRight', 'Home', 'End', 'Escape'].includes(e.key)) return;
    e.preventDefault();
    const cur = L.hover == null ? L.i : L.hover;
    L.hover = e.key === 'Escape' ? null : e.key === 'Home' ? 0 : e.key === 'End' ? L.i : Math.max(0, Math.min(L.i, cur + (e.key === 'ArrowRight' ? 1 : -1)));
    p.classList.toggle('is-hover', L.hover != null); L.hc = p.id.replace('ldp-', '');
    paintHover(); placeTip(p.id.replace('ldp-', ''));
  });
  document.addEventListener('focusout', (e) => { if (e.target.matches && e.target.matches('.ld-plot[id]')) { e.target.classList.remove('is-hover'); L.hover = null; paintHover(); } });
})();
