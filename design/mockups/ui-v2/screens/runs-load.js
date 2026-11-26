/* Runs space · load run (#runs-load): one execution of a load file, two views.
   Live: KPIs, then iterations/s, p50/p95/p99 and error rate with threshold lines, breach
   markers, a shared crosshair and a table view; a hosts panel. A finished run shows its
   whole timeline here. Report: thresholds, latency histogram, previous runs, per scenario.
   Every SVG is drawn in mount() at its plot's measured width and height, so charts fill
   their card. The load file itself is designed in Tests › Load. */
(function () {
  'use strict';
  const VC = window.VC, D = VC.data, S = VC.state, A = VC.actions, SH = VC.sh, TD = VC.testsData;
  const { $, $$, esc, ic, badge } = VC.h;
  const LD = TD.load, SER = LD.series, N = SER.length - 1, END = SER[N].t;
  const R = (VC.rl = { n: 478, view: 'live', phase: 'done', i: N, hover: null, hc: null, tbl: {}, timer: null, ro: null });
  const reduced = () => S.motion === 'reduce' || (S.motion === 'system' && matchMedia('(prefers-reduced-motion: reduce)').matches);
  const mmss = (t) => `${Math.floor(t / 60)}:${String(t % 60).padStart(2, '0')}`;
  const fmtN = (v) => (v >= 1000 ? v.toLocaleString('en-US') : String(v));
  const running = () => R.phase === 'running' || R.phase === 'stopping';
  const run = () => D.shared.runs.find((r) => r.n === R.n) || { profile: S.profile, when: 'Today 14:12', by: 'you', branch: D.project.branch };
  const TH = { p99: 800, err: 1 };

  /* ---- geometry: sizes come from the plot, never a fixed viewBox ---------------- */
  const M = { l: 46, r: 76, t: 16, b: 26 };
  function nice(v) { const p = Math.pow(10, Math.floor(Math.log10(v))); const f = v / p; return (f <= 1 ? 1 : f <= 2 ? 2 : f <= 2.5 ? 2.5 : f <= 5 ? 5 : 10) * p; }
  const CH = {
    lat: { title: 'Latency', sub: 'per iteration, from its scheduled start', series: [['p50', 'p50', 'p50'], ['p95', 'p95', 'p95'], ['p99', 'p99', 'p99']], unit: ' ms', max: () => nice(Math.max(TH.p99 * 1.25, ...SER.slice(0, R.i + 1).map((p) => p.p99 * 1.05))), th: { k: 'p99', v: TH.p99, label: 'p99 lt 800ms', fail: true } },
    rps: { title: 'Iterations per second', sub: 'all mix tests', series: [['rps', 'iterations/s', 'one']], unit: '/s', max: () => nice(Math.max(...SER.map((p) => p.rps)) * 1.08) },
    err: { title: 'Error rate', sub: 'rolling 10 s', series: [['err', 'errorRate', 'one']], unit: '%', max: () => 1.25, th: { k: 'err', v: TH.err, label: 'errorRate lt 1% · abortOn', fail: false } }
  };
  const size = (el) => [Math.max(240, Math.round(el.clientWidth)), Math.max(160, Math.round(el.clientHeight))];
  function scales(c, [w, h]) {
    const ymax = c.max();
    return { x: (t) => M.l + (t / END) * (w - M.l - M.r), y: (v) => M.t + (1 - v / ymax) * (h - M.t - M.b), ymax, w, h };
  }
  function breaches(k, v) {
    const out = [];
    let s = -1;
    for (let i = 0; i <= R.i; i++) {
      const over = SER[i][k] > v;
      if (over && s < 0) s = i;
      if ((!over || i === R.i) && s >= 0) { out.push([s, over ? i : i - 1]); s = -1; }
    }
    return out;
  }
  function endLabels(c, sc) {
    const p = SER[R.i];
    const ls = c.series.map(([k, l]) => ({ k, l, y: sc.y(p[k]) })).sort((a, b) => a.y - b.y);
    for (let i = 1; i < ls.length; i++) if (ls[i].y - ls[i - 1].y < 13) ls[i].y = ls[i - 1].y + 13;
    const floor = sc.y(0) - 6;
    for (let i = ls.length - 1; i >= 0; i--) ls[i].y = Math.min(ls[i].y, i === ls.length - 1 ? floor : ls[i + 1].y - 13);
    return ls.map((o) => `<text class="rl-end" x="${sc.x(p.t) + 8}" y="${o.y + 4}">${o.l} ${fmtN(p[o.k])}</text>`).join('');
  }
  function thresholdSvg(c, sc) {
    if (!c.th) return '';
    const y = sc.y(c.th.v), h = sc.h - M.t - M.b;
    let g = breaches(c.th.k, c.th.v).map(([a, b]) => `<rect class="rl-band${c.th.fail ? '' : ' is-warn'}" x="${sc.x(SER[a].t)}" y="${M.t}" width="${Math.max(2, sc.x(SER[b].t) - sc.x(SER[a].t))}" height="${h}"/>`).join('');
    return g + `<line class="rl-th" x1="${M.l}" x2="${sc.w - M.r}" y1="${y}" y2="${y}"/><text class="rl-thl" x="${M.l + 6}" y="${y - 5}">${esc(c.th.label)}</text>`;
  }
  function breachMarks(c, sc) {
    if (!c.th || !c.th.fail) return '';
    return breaches(c.th.k, c.th.v).map(([a, b]) => {
      let pk = a; for (let i = a; i <= b; i++) if (SER[i][c.th.k] > SER[pk][c.th.k]) pk = i;
      const x = sc.x(SER[pk].t), y = sc.y(SER[pk][c.th.k]);
      return `<circle class="rl-breach" cx="${x}" cy="${y}" r="5"/><text class="rl-bl" x="${x}" y="${Math.max(M.t + 10, y - 10)}" text-anchor="middle">✕ breach ${mmss(SER[a].t)}–${mmss(SER[b].t)}</text>`;
    }).join('');
  }
  function chartSvg(id, sz) {
    const c = CH[id], sc = scales(c, sz), pts = SER.slice(0, R.i + 1);
    const every = sc.w < 520 ? 120 : 60;
    let g = [0, 0.25, 0.5, 0.75, 1].map((f) => f * sc.ymax).map((v) => `<line class="rl-grid" x1="${M.l}" x2="${sc.w - M.r}" y1="${sc.y(v)}" y2="${sc.y(v)}"/><text class="rl-ax" x="${M.l - 6}" y="${sc.y(v) + 4}" text-anchor="end">${fmtN(+v.toFixed(2))}</text>`).join('');
    for (let t = 0; t <= END; t += every) g += `<text class="rl-ax" x="${sc.x(t)}" y="${sc.h - 8}" text-anchor="middle">${mmss(t)}</text>`;
    g += `<line class="rl-base" x1="${M.l}" x2="${sc.w - M.r}" y1="${sc.y(0)}" y2="${sc.y(0)}"/>` + thresholdSvg(c, sc);
    c.series.forEach(([k, , col]) => { if (pts.length > 1) g += `<path class="rl-line s-${col}" d="${pts.map((p, i) => `${i ? 'L' : 'M'}${sc.x(p.t).toFixed(1)},${sc.y(p[k]).toFixed(1)}`).join('')}"/>`; });
    g += breachMarks(c, sc);
    if (c.series.length > 1) g += endLabels(c, sc);
    else { const p = SER[R.i]; g += `<circle class="rl-dot s-${c.series[0][2]}" cx="${sc.x(p.t)}" cy="${sc.y(p[c.series[0][0]])}" r="4"/>`; }
    g += `<g class="rl-xh" data-c="${id}">${xhair(id, sc)}</g><rect class="rl-hit" data-c="${id}" x="${M.l}" y="${M.t}" width="${sc.w - M.l - M.r}" height="${sc.h - M.t - M.b}"/>`;
    return `<svg class="rl-svg" viewBox="0 0 ${sc.w} ${sc.h}" width="${sc.w}" height="${sc.h}" aria-hidden="true">${g}</svg>`;
  }
  function xhair(id, sc) {
    if (R.hover == null || R.hover > R.i) return '';
    const p = SER[R.hover], x = sc.x(p.t);
    return `<line class="rl-xline" x1="${x}" x2="${x}" y1="${M.t}" y2="${sc.h - M.b}"/>${CH[id].series.map(([k, , col]) => `<circle class="rl-dot s-${col}" cx="${x}" cy="${sc.y(p[k])}" r="4"/>`).join('')}`;
  }
  function tipHtml(id) {
    if (R.hover == null || R.hover > R.i) return '';
    const p = SER[R.hover], c = CH[id];
    return `<b class="mono">${mmss(p.t)} · ${fmtN(p.users)} users</b>${c.series.map(([k, l, col]) => `<span class="rl-tr"><i class="rl-sw s-${col}"></i>${l}<b class="num">${fmtN(p[k])}${c.unit}</b>${c.th && c.th.k === k && p[k] > c.th.v ? `<span class="rl-over">${ic('fail', 'ico-sm')}over</span>` : ''}</span>`).join('')}`;
  }
  function tableHtml(id) {
    const c = CH[id], rows = SER.filter((p, i) => i <= R.i && i % 6 === 0);
    return `<div class="table-wrap glass-dense rl-tbl"><table class="table"><caption class="sr-only">${esc(c.title)} every 30 s</caption><thead><tr><th scope="col">Time</th><th scope="col" class="r">Users</th>${c.series.map(([, l]) => `<th scope="col" class="r">${l}</th>`).join('')}</tr></thead><tbody>${rows.map((p) => `<tr><td class="mono">${mmss(p.t)}</td><td class="r num">${fmtN(p.users)}</td>${c.series.map(([k]) => `<td class="r num${c.th && c.th.k === k && p[k] > c.th.v ? ' rl-bad' : ''}">${fmtN(p[k])}${c.unit}</td>`).join('')}</tr>`).join('')}</tbody></table></div>`;
  }
  function chartCard(id) {
    const c = CH[id], p = SER[R.i], br = c.th && c.th.fail ? breaches(c.th.k, c.th.v) : [];
    const leg = c.series.length > 1 ? `<ul class="rl-legend" aria-label="Series">${c.series.map(([, l, col]) => `<li><i class="rl-sw s-${col}"></i>${l}</li>`).join('')}${c.th ? '<li><i class="rl-sw is-th"></i>threshold</li>' : ''}</ul>` : '';
    const cap = br.length ? `<figcaption class="rl-cap"><span class="badge badge-sm st-fail">${ic('fail')}breach</span>p99 over 800 ms from ${br.map(([a, b]) => `${mmss(SER[a].t)} to ${mmss(SER[b].t)}`).join(', ')} (rolling 10 s)</figcaption>`
      : c.th ? `<figcaption class="rl-cap">${ic('pass', 'ico-sm')}Held on the rolling window · peak ${Math.max(...SER.slice(0, R.i + 1).map((q) => q.err))}%</figcaption>` : '';
    return `<figure class="glass rl-chart" aria-labelledby="rlc-${id}"><div class="card-head"><div class="grow"><h3 class="card-title" id="rlc-${id}">${c.title}</h3><span class="card-sub">${c.sub} · now ${fmtN(p[c.series[c.series.length - 1][0]])}${c.unit}</span></div></div>
      ${leg}${R.tbl.live ? tableHtml(id) : `<div class="rl-plot" id="rlp-${id}" data-c="${id}" tabindex="0" role="img" aria-label="${esc(c.title)} over ${mmss(p.t)}; arrow keys read values; Table shows every 30 s"><div class="rl-tip l3" id="rltip-${id}" role="status">${tipHtml(id)}</div></div>`}${cap}</figure>`;
  }

  /* ---- live ------------------------------------------------------------------ */
  function stage(t) { let acc = 0; for (let i = 0; i < LD.stages.length; i++) { acc += LD.stages[i].duration; if (t <= acc) return [i, LD.stages[i]]; } return [LD.stages.length - 1, LD.stages[LD.stages.length - 1]]; }
  const peak = (k) => SER.slice(0, R.i + 1).reduce((a, q) => (q[k] > a[k] ? q : a), SER[0]);
  const kpi = (lab, v, sub, bad) => `<li class="rl-kpi${bad ? ' is-bad' : ''}"><span class="rl-kl">${lab}</span><b class="num">${v}</b><span class="rl-ks">${sub}</span></li>`;
  /* live: the value now; finished: the worst window and when it was */
  function worst(k, lab, unit, lim, th) {
    if (running()) { const v = SER[R.i][k]; return kpi(lab[0].toUpperCase() + lab.slice(1), `${v}${unit}`, lim, v > th); }
    const w = peak(k);
    return kpi(`Worst ${lab}`, `${w[k]}${unit}`, `at ${mmss(w.t)} · ${lim}`, w[k] > th);
  }
  function kpis() {
    const p = SER[R.i], [si, st] = stage(p.t), tot = SER.slice(0, R.i + 1).reduce((s, q) => s + q.rps * 5, 0), pu = peak('users'), pr = peak('rps');
    const users = running() ? kpi('Users', fmtN(p.users), `${si === 0 || st.users > (LD.stages[si - 1] || {}).users ? 'ramping to' : 'down to'} ${fmtN(st.users)}`) : kpi('Peak users', fmtN(pu.users), `at ${mmss(pu.t)}`);
    const rate = running() ? kpi('Iterations/s', fmtN(p.rps), `${fmtN(Math.round(tot))} iterations`) : kpi('Peak iterations/s', fmtN(pr.rps), `${fmtN(Math.round(tot))} iterations in all`);
    return `<ul class="rl-kpis" aria-label="${running() ? 'Now' : 'Run summary'}">${kpi('Elapsed', mmss(p.t), `of ${mmss(END)} · stage ${si + 1} of ${LD.stages.length}`)}${users}${rate}${worst('p99', 'p99', ' ms', 'limit 800 ms', TH.p99)}${worst('err', 'error rate', '%', 'limit 1% · abortOn', TH.err)}</ul>`;
  }
  function hostsHtml() {
    const p = SER[R.i], drop = SER.slice(0, R.i + 1).reduce((s, q) => s + q.dropped, 0);
    const spark = (k) => SH.spark(SER.slice(0, R.i + 1).filter((q, i) => i % 3 === 0).map((q) => q[k]), `${k} over the run`, 96, 22);
    const row = (l, v, k, hint, warn) => `<tr${warn ? ' class="is-warn"' : ''}><th scope="row">${l}</th><td class="r num">${v}</td><td class="hide-narrow">${spark(k)}</td><td class="subtle">${hint}</td></tr>`;
    return `<section class="glass rl-hosts" aria-labelledby="rl-h-t"><div class="card-head"><div class="grow"><h3 class="card-title" id="rl-h-t">Hosts</h3><span class="card-sub">runner-1 · this machine · 8 cores</span></div></div>
      <div class="card-body">${SH.table(['Metric', ['Now', 'r'], ['Run', 'hide-narrow'], 'Note'], [row('CPU', `${p.cpu}%`, 'cpu', p.cpu > 70 ? 'high; results stay valid below 85%' : 'headroom ok', p.cpu > 70), row('Memory', `${fmtN(p.mem)} MB`, 'mem', 'of 16 GB'), row('Connections', fmtN(p.conns), 'conns', 'keep-alive pool'), row('Dropped iterations', fmtN(drop), 'dropped', drop ? 'started late; timed from the scheduled start' : 'none', drop > 0)])}
      ${drop ? `<p class="callout" data-tone="warn">${ic('warn')}<span><b>${fmtN(drop)} iterations started late</b>: the runner fell behind. Latency is timed from each iteration's scheduled start, so the delay shows in p99.</span></p>` : ''}</div></section>`;
  }
  const liveHtml = () => `<div class="rl-charts" id="rl-live">${chartCard('lat')}${chartCard('rps')}${chartCard('err')}${hostsHtml()}</div>`;

  /* ---- report ------------------------------------------------------------------ */
  /* the per-test counts are a sample; scale them to the timeline's iteration total so every number agrees */
  const total = () => SER.reduce((s, q) => s + q.rps * 5, 0);
  const K = () => total() / LD.scenarios.reduce((s, x) => s + x.iters, 0);
  const hist = () => { const k = total() / LD.histogram.reduce((s, x) => s + x[1], 0); return LD.histogram.map(([ms, n]) => [ms, Math.round(n * k)]); };
  function histogramSvg([w, h]) {
    const hs = hist(), max = Math.max(...hs.map((x) => x[1])), l = 50, b = 30, bw = (w - l - 10) / hs.length, y = (v) => 10 + (1 - v / max) * (h - b - 10), xl = l + hs.findIndex((x) => x[0] > 800) * bw;
    let g = [0, 0.5, 1].map((f) => `<line class="rl-grid" x1="${l}" x2="${w - 10}" y1="${y(f * max)}" y2="${y(f * max)}"/><text class="rl-ax" x="${l - 6}" y="${y(f * max) + 4}" text-anchor="end">${f ? `${Math.round((f * max) / 1000)}k` : 0}</text>`).join('');
    g += hs.map(([ms, n], i) => `<g class="rl-bar${ms > 800 ? ' is-over' : ''}" tabindex="0" role="listitem" aria-label="up to ${ms} ms: ${fmtN(n)} iterations"><rect x="${l + i * bw + 1}" y="${y(n)}" width="${Math.max(1, bw - 2)}" height="${Math.max(1, y(0) - y(n))}" rx="2"/><text class="rl-ax" x="${l + i * bw + bw / 2}" y="${h - 12}" text-anchor="middle">${ms >= 1000 ? ms / 1000 + 's' : ms}</text><text class="rl-bv" x="${l + i * bw + bw / 2}" y="${Math.max(20, y(n) - 6)}" text-anchor="middle">${fmtN(n)}</text></g>`).join('');
    g += `<line class="rl-th" x1="${xl}" x2="${xl}" y1="10" y2="${h - b}"/><text class="rl-thl" x="${xl + 4}" y="20">p99 limit 800 ms</text>`;
    return `<svg class="rl-svg" viewBox="0 0 ${w} ${h}" width="${w}" height="${h}" role="list" aria-label="Latency histogram">${g}</svg>`;
  }
  function previousSvg([w, h]) {
    const P = LD.previous, l = 150, rowH = Math.min(40, (h - 24) / P.length), x = (v) => l + (v / 1000) * (w - l - 60);
    let g = P.map((r, i) => `<g class="rl-hbar${r.cur ? ' is-cur' : ''}"><text class="rl-ax is-name" x="${l - 8}" y="${i * rowH + rowH / 2 + 4}" text-anchor="end">${r.cur ? 'this run' : esc(r.run)}</text><rect x="${l}" y="${i * rowH + (rowH - 18) / 2}" width="${x(r.p99) - l}" height="18" rx="3"/><text class="rl-end" x="${x(r.p99) + 6}" y="${i * rowH + rowH / 2 + 4}">${r.p99} ms</text></g>`).join('');
    g += `<line class="rl-th" x1="${x(800)}" x2="${x(800)}" y1="2" y2="${P.length * rowH + 4}"/><text class="rl-thl" x="${x(800)}" y="${P.length * rowH + 18}" text-anchor="middle">limit 800</text>`;
    return `<svg class="rl-svg" viewBox="0 0 ${w} ${h}" width="${w}" height="${h}" role="img" aria-label="p99 by run: ${P.map((r) => `${r.run} ${r.p99} ms`).join(', ')}">${g}</svg>`;
  }
  const DRAW = { lat: (sz) => chartSvg('lat', sz), rps: (sz) => chartSvg('rps', sz), err: (sz) => chartSvg('err', sz), hist: histogramSvg, prev: previousSvg };
  const figure = (id, title, sub, cap) => `<figure class="glass rl-chart" aria-labelledby="rlc-${id}"><div class="card-head"><div class="grow"><h3 class="card-title" id="rlc-${id}">${title}</h3><span class="card-sub">${sub}</span></div></div><div class="rl-plot" data-c="${id}"></div><figcaption class="rl-cap">${cap}</figcaption></figure>`;
  function reportHtml() {
    const hs = hist(), P = LD.previous, k = K();
    const th = LD.breaches.map((b) => `<tr><td>${badge(b.st, b.st === 'fail' ? 'breached' : 'held', true)}</td><td class="mono">${b.metric}</td><td class="mono ellipsis">${esc(b.scope)}</td><td class="mono">${esc(b.limit)}</td><td class="num">${esc(b.value)}</td><td class="mono hide-narrow">${esc(b.at)}</td><td class="wrap">${esc(b.final)}</td></tr>`);
    const sc = LD.scenarios.map((s) => `<tr><td class="mono"><a href="#tests" data-action="rl-open-test" data-id="${esc(s.test)}">${esc(s.test)}</a></td><td class="r num">${s.weight}</td><td class="r num">${fmtN(Math.round(s.iters * k))}</td><td class="r num">${s.p50}</td><td class="r num">${s.p95}</td><td class="r num${s.p99 > 800 ? ' rl-bad' : ''}">${s.p99}</td><td class="r num${s.err > 1 ? ' rl-bad' : ''}">${s.err}%</td><td>${badge(s.verdict, null, true)}</td></tr>`);
    return `<section class="glass" aria-labelledby="rl-br-t"><div class="card-head"><div class="grow"><h3 class="card-title" id="rl-br-t">Thresholds</h3><span class="card-sub">rolling windows are 10 s; the verdict uses the whole run</span></div></div>
        <div class="card-body">${SH.table(['Verdict', 'Metric', 'Scope', 'Limit', 'Worst window', ['When', 'hide-narrow'], 'Whole run'], th)}</div></section>
      <div class="rl-charts">${figure('hist', 'Latency histogram', `iterations per bucket (upper bound, ms) · ${fmtN(hs.reduce((s, x) => s + x[1], 0))} in all`, `${fmtN(hs.filter((x) => x[0] > 800).reduce((s, x) => s + x[1], 0))} iterations (1.0%) slower than 800 ms · hover or focus a bar for its count`)}
        ${figure('prev', 'Compared with previous runs', `p99 per run of ${esc(LD.id)}`, `p99 is ${P[0].p99 - P[1].p99} ms slower than the last master run (${esc(P[1].run)}); iterations/s and errors are within 3%.`)}</div>
      <section class="glass" aria-labelledby="rl-sc-t"><div class="card-head"><div class="grow"><h3 class="card-title" id="rl-sc-t">Per scenario</h3><span class="card-sub">${fmtN(Math.round(total()))} iterations · checks: ${LD.checks}</span></div></div>
        <div class="card-body">${SH.table(['Test', ['Weight', 'r'], ['Iterations', 'r'], ['p50', 'r'], ['p95', 'r'], ['p99', 'r'], ['Errors', 'r'], 'Verdict'], sc)}
        <p class="hint subtle">placesOrder is 10% of the mix and 61% of iterations over 800 ms. Its own threshold (p95 lt 1200ms) held at 1088 ms.</p></div></section>`;
  }

  /* ---- screen ---------------------------------------------------------------- */
  function verdict() {
    if (running()) return badge('running', R.phase === 'stopping' ? 'stopping' : 'running', true);
    if (R.phase === 'stopped') return badge('skipped', 'stopped · exit 130', true);
    return badge('fail', 'breached · exit 3', true);
  }
  function headHtml() {
    const r = run(), views = [['live', running() ? 'Live' : 'Timeline'], ['report', 'Report']];
    const act = running() ? `<button class="btn btn-sm btn-danger" data-action="rl-stop" data-fk="rl-stop"${R.phase === 'stopping' ? ' disabled' : ''}>${ic('stop', 'ico-sm')}Stop</button>`
      : `<button class="btn btn-sm" data-action="rl-run" data-fk="rl-run">${ic('refresh', 'ico-sm')}Run again</button>`;
    const step = running() && reduced() ? `<p class="rl-step"><span class="subtle grow">Reduced motion: the run advances when you step.</span><button class="btn btn-sm" data-action="rl-step" data-fk="rl-step">${ic('arrow-right', 'ico-sm')}Step 30 s</button><button class="btn btn-sm btn-ghost" data-action="rl-finish">Finish</button></p>` : '';
    return `<section class="glass rl-head" id="rl-head" aria-labelledby="rl-title"><div class="card-head"><div class="grow"><h2 class="card-title" id="rl-title"><span class="mono">${esc(LD.id)}</span>${verdict()}</h2>
        <span class="card-sub ellipsis">${esc(LD.name)} · ${esc(r.profile)} · ${esc(r.when)} · by ${r.by === 'ci' ? 'CI' : esc(r.by)}${r.sched ? ` · schedule ${esc(r.sched)}` : ''}</span></div>
        ${R.view === 'live' ? `<button class="btn btn-ghost btn-sm" data-action="rl-tbl" aria-pressed="${!!R.tbl.live}" data-fk="rl-tbl" title="Show the charts as tables">${ic('rows', 'ico-sm')}<span class="hide-narrow">Table</span></button>` : ''}
        ${SH.seg('View', views, R.view, 'rl-view')}${act}</div>
      <div class="card-body rl-now">${kpis()}<progress class="tr-prog rl-prog${R.phase === 'stopping' ? ' is-stopping' : ''}" max="${N}" value="${R.i}" aria-label="Load run progress">${Math.round((R.i / N) * 100)}%</progress>${step}</div></section>`;
  }
  function draw(root) {
    $$('.rl-plot[data-c]', root).forEach((p) => {
      const svg = DRAW[p.dataset.c](size(p)), old = $('svg.rl-svg', p);
      if (old) old.outerHTML = svg; else p.insertAdjacentHTML('afterbegin', svg);
    });
  }
  VC.screens['runs-load'] = {
    space: 'runs',
    crumbs: () => [D.project.name, 'Runs', `#${R.n} ${running() ? 'live' : 'load'}`],
    status: () => '',
    primary: () => SH.runBtn(),
    render: () => `<div class="rl-page sh-scope" data-sk="rl-body">${headHtml()}${R.view === 'report' ? reportHtml() : liveHtml()}</div>`,
    mount(view) {
      SH.sizes(view);
      draw(view);
      let last = '';
      R.ro = new ResizeObserver(() => { const k = $$('.rl-plot', view).map((p) => size(p).join('x')).join(','); if (k !== last) { last = k; draw(view); } });
      $$('.rl-plot', view).forEach((p) => R.ro.observe(p));
    },
    unmount() { if (R.ro) R.ro.disconnect(); R.ro = null; }
  };
  /* a tick repaints the head and the live charts, keeping focus and the crosshair */
  function paint() {
    if (S.route !== 'runs-load') return;
    const k = document.activeElement && document.activeElement.id;
    const head = $('#rl-head'), live = $('#rl-live');
    if (head) head.outerHTML = headHtml();
    if (live) { live.outerHTML = liveHtml(); VC.screens['runs-load'].unmount(); VC.screens['runs-load'].mount($('#view')); }
    if (k) { const el = document.getElementById(k); if (el) el.focus({ preventScroll: true }); }
    if (R.hover != null && R.hc) { const hp = $(`#rlp-${R.hc}`); if (hp) { hp.classList.add('is-hover'); placeTip(R.hc); } }
  }
  function paintHover() {
    Object.keys(CH).forEach((id) => {
      const p = $(`#rlp-${id}`), g = $(`.rl-xh[data-c="${id}"]`), tip = $(`#rltip-${id}`);
      if (g && p) g.innerHTML = xhair(id, scales(CH[id], size(p)));
      if (tip) tip.innerHTML = tipHtml(id);
    });
  }

  /* ---- run lifecycle ------------------------------------------------------------ */
  function record() {
    if (D.shared.runs.some((r) => r.n === R.n)) return;
    const tot = Math.round(SER.reduce((s, q) => s + q.rps * 5, 0));
    D.shared.runs.unshift({ n: R.n, kind: 'load', st: R.phase === 'done' ? 'fail' : 'skipped', when: 'Today 14:12', ago: 'just now', dur: mmss(SER[R.i].t).replace(':', 'm ') + 's', profile: S.profile, layers: [], branch: D.project.branch, commit: 'a41c9e2', by: 'you', sel: LD.id, load: { iters: tot, errs: 4120, breach: R.phase === 'done' ? 'p99 lt 800ms · got 862 ms' : null } });
  }
  function tick(n) {
    R.i = Math.min(N, R.i + (n || 1));
    if (R.phase === 'stopping' && SER[R.i].users === 0) R.i = N;
    if (R.i >= N) return finish(R.phase === 'stopping' ? 'stopped' : 'done');
    paint();
  }
  function finish(ph) {
    clearInterval(R.timer); R.timer = null; R.phase = ph; record();
    if (ph === 'done') { R.view = 'report'; VC.toast(`Load run #${R.n} finished · exit 3`, 'breached p99 at 862 ms (lt 800 ms) · report ready', 'warn'); }
    else VC.toast(`Load run #${R.n} stopped · exit 130`, 'afterAll and the cleanups ran; partial results are in the report');
    if (S.route === 'runs-load') VC.render();
  }
  function start() {
    VC.closeDialog();
    clearInterval(R.timer);
    R.n = Math.max(...D.shared.runs.map((r) => r.n), VC.run ? VC.run.n : 0) + 1;
    Object.assign(R, { i: 0, phase: 'running', view: 'live', hover: null, timer: null });
    VC.go('runs-load');
    if (!reduced()) R.timer = setInterval(() => tick(1), 160);
  }
  VC.openLoadRun = (n) => {
    if (!(running() && n === R.n)) Object.assign(R, { n: +n || 478, phase: 'done', i: N, view: 'report', hover: null });
    VC.go('runs-load');
  };
  A['rl-run'] = () => {
    if (running()) return;
    const prod = S.profile === 'prod', p = VC.profile();
    VC.dialog(`<div class="dlg dlg-narrow"><div class="dlg-head"><div class="grow"><h2 id="dlg-title">Run ${esc(LD.id)} again</h2><div class="sub">Same as <code>vc load ${esc(LD.id)} --profile ${esc(p.id)}</code></div></div><button class="btn btn-ghost btn-icon" data-action="close-dialog" aria-label="Close">${ic('x')}</button></div>
      <div class="dlg-body"><div class="kv glass-dense"><div class="kv-row"><span class="k">target</span><span class="v mono">${esc(p.baseUrl)}</span></div><div class="kv-row"><span class="k">peak</span><span class="v">5,000 users · ${mmss(END)}</span></div></div>
        ${prod ? `<p class="callout" data-tone="danger">${ic('warn')}<span><b>This sends about 1.2 million requests to live prod.</b> Type the profile name to confirm.</span></p><div class="field"><label for="rl-conf">Profile name</label><input class="input mono" id="rl-conf" autocomplete="off" placeholder="prod"></div>` : ''}</div>
      <div class="dlg-foot"><button class="btn" data-action="close-dialog">Cancel</button><button class="btn btn-primary${prod ? ' btn-danger' : ''}" data-action="rl-go" id="rl-go"${prod ? ' disabled' : ''}>${ic('play')}Start</button></div></div>`);
    const c = $('#rl-conf');
    if (c) c.addEventListener('input', () => { $('#rl-go').disabled = c.value !== 'prod'; });
  };
  A['rl-go'] = start;
  A['rl-stop'] = () => { R.phase = 'stopping'; R.i = Math.max(R.i, N - 12); paint(); VC.toast('Stopping', 'No new iterations; in-flight ones finish, then afterAll and the cleanups run'); if (!R.timer && !reduced()) R.timer = setInterval(() => tick(1), 160); };
  A['rl-step'] = () => tick(6);
  A['rl-finish'] = () => tick(N);
  A['rl-view'] = (el) => { R.view = el.dataset.v; R.hover = null; VC.render(); };
  A['rl-tbl'] = () => { R.tbl.live = !R.tbl.live; VC.render(); };
  A['rl-open-test'] = (el) => { if (VC.tx) VC.tx.sel = el.dataset.id; VC.go('tests'); };

  /* ---- hover and keyboard crosshair, shared by the three live charts ---------------- */
  function placeTip(id) {
    const plot = $(`#rlp-${id}`), tip = $(`#rltip-${id}`);
    if (!plot || !tip || R.hover == null) return;
    const sc = scales(CH[id], size(plot)), x = sc.x(SER[R.hover].t);
    tip.style.setProperty('--tx', `${Math.round(x > sc.w / 2 ? x - 12 : x + 12)}px`);
    tip.classList.toggle('is-left', x > sc.w / 2);
  }
  const hoverOn = (id, i) => {
    R.hover = i; R.hc = id;
    $$('.rl-plot.is-hover').forEach((p) => p.classList.remove('is-hover'));
    if (i != null) { const p = $(`#rlp-${id}`); if (p) p.classList.add('is-hover'); }
    paintHover(); placeTip(id);
  };
  document.addEventListener('pointermove', (e) => {
    const hit = e.target.closest && e.target.closest('.rl-hit');
    if (!hit) return;
    const r = hit.getBoundingClientRect(), f = Math.min(1, Math.max(0, (e.clientX - r.left) / r.width)), i = Math.min(R.i, Math.round((f * END) / 5));
    if (i !== R.hover) hoverOn(hit.dataset.c, i);
  });
  document.addEventListener('pointerout', (e) => { if (e.target.closest && e.target.closest('.rl-hit') && !(e.relatedTarget && e.relatedTarget.closest && e.relatedTarget.closest('.rl-hit'))) hoverOn(R.hc, null); });
  document.addEventListener('keydown', (e) => {
    const p = e.target.closest && e.target.closest('.rl-plot[id]');
    if (!p || !['ArrowLeft', 'ArrowRight', 'Home', 'End', 'Escape'].includes(e.key)) return;
    e.preventDefault();
    const cur = R.hover == null ? R.i : R.hover;
    hoverOn(p.dataset.c, e.key === 'Escape' ? null : e.key === 'Home' ? 0 : e.key === 'End' ? R.i : Math.max(0, Math.min(R.i, cur + (e.key === 'ArrowRight' ? 1 : -1))));
  });
  document.addEventListener('focusout', (e) => { if (e.target.matches && e.target.matches('.rl-plot[id]')) hoverOn(e.target.dataset.c, null); });
})();
