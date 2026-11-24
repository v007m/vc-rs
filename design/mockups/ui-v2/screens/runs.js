/* Runs (#runs): the stored-runs list, one stored run (same layout as a live run's drill-down)
   and Compare two runs. Also owns VC.sh, the small helpers the other shared screens reuse. */
(function () {
  'use strict';
  const VC = window.VC, D = VC.data, X = D.shared, A = VC.actions;
  const { $, $$, esc, ic, badge, method } = VC.h;

  /* ---- shared helpers (Insights, Agents and Settings use these too) ------------ */
  const SH = VC.sh = {};
  SH.AGENTS = ['claude-code', 'cursor'];
  SH.isAgent = (who) => SH.AGENTS.includes(who);
  SH.agent = (who) => `<span class="sh-agent" title="An AI agent, over MCP">${ic('bot', 'ico-sm')}${esc(who)}</span>`;
  SH.by = (who) => (SH.isAgent(who) ? SH.agent(who) : who === 'ci' ? '<span class="chip">CI</span>' : `<span class="muted">${esc(who)}</span>`);
  SH.ORDER = ['pass', 'flaky', 'allow', 'fail', 'blocked', 'skipped'];
  SH.WORD = { pass: 'passed', flaky: 'flaky', allow: 'allowed to fail', fail: 'failed', blocked: 'blocked', skipped: 'skipped' };
  SH.totLabel = (tot) => SH.ORDER.filter((k) => tot[k]).map((k) => `${tot[k]} ${SH.WORD[k]}`).join(', ');
  /* thin segmented bar: one whole split into status parts; widths are set in mount via --w */
  SH.segbar = (tot, cls) => `<span class="sh-seg${cls ? ' ' + cls : ''}" role="img" aria-label="${esc(SH.totLabel(tot))}">${SH.ORDER.filter((k) => tot[k]).map((k) => `<i data-st="${k}" data-w="${tot[k]}"></i>`).join('')}</span>`;
  SH.counts = (tot, only) => SH.ORDER.filter((k) => tot[k] && (!only || only.includes(k))).map((k) => `<span class="sh-count" data-st="${k}">${ic(k, 'ico-sm')}<b>${tot[k]}</b><span class="sr-only"> ${SH.WORD[k]}</span></span>`).join('');
  SH.sizes = (root) => $$('[data-w]', root).forEach((el) => el.style.setProperty('--w', el.dataset.w));
  SH.spark = (vals, label, w, h) => {
    w = w || 96; h = h || 24;
    const lo = Math.min(...vals), hi = Math.max(...vals), sp = hi - lo || 1;
    const pts = vals.map((v, i) => [2 + (i / (vals.length - 1)) * (w - 6), 3 + (1 - (v - lo) / sp) * (h - 6)]);
    const last = pts[pts.length - 1];
    return `<svg class="sh-spark" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}" role="img" aria-label="${esc(label)}"><polyline points="${pts.map((p) => p.map((n) => n.toFixed(1)).join(',')).join(' ')}"/><circle cx="${last[0].toFixed(1)}" cy="${last[1].toFixed(1)}" r="3"/></svg>`;
  };
  SH.tabs = (id, label, items, cur, action) => `<div class="tabs" role="tablist" aria-label="${esc(label)}">${items.map(([k, l, n, tone]) =>
    `<button class="tab" role="tab" id="${id}-${k}" aria-selected="${cur === k}" aria-controls="${id}-panel" tabindex="${cur === k ? 0 : -1}" data-action="${action}" data-v="${k}" data-fk="${id}-${k}">${esc(l)}${n !== undefined ? `<span class="count${tone ? ' ' + tone : ''}">${n}</span>` : ''}</button>`).join('')}</div>`;
  SH.seg = (label, opts, cur, action, extra) => `<div class="seg seg-sm" role="group" aria-label="${esc(label)}">${opts.map(([k, l]) => `<button aria-pressed="${cur === k}" data-action="${action}" data-v="${k}" data-fk="${action}-${k}"${extra || ''}>${l}</button>`).join('')}</div>`;
  SH.table = (head, rows, cls) => `<div class="table-wrap glass-dense${cls ? ' ' + cls : ''}"><table class="table"><thead><tr>${head.map((h) => (Array.isArray(h) ? `<th scope="col" class="${h[1]}">${h[0]}</th>` : `<th scope="col">${h}</th>`)).join('')}</tr></thead><tbody>${rows.join('')}</tbody></table></div>`;
  SH.empty = (icon, title, sub) => `<div class="empty">${ic(icon)}<b>${esc(title)}</b><span>${sub}</span></div>`;
  SH.store = {
    get(k, d) { try { const v = JSON.parse(localStorage.getItem('vc-ui-v2-shared') || '{}'); return v[k] === undefined ? d : v[k]; } catch (e) { return d; } },
    set(k, val) { try { const v = JSON.parse(localStorage.getItem('vc-ui-v2-shared') || '{}'); v[k] = val; localStorage.setItem('vc-ui-v2-shared', JSON.stringify(v)); } catch (e) { /* storage may be blocked */ } }
  };
  A['sh-noop'] = () => {};

  /* ---- state --------------------------------------------------------------------- */
  const R = { view: 'list', run: 482, test: 'orders.checkout#rejectsDeclinedCard', dtab: 'checks', show: 'all', q: '', st: 'all', profile: 'all', branch: 'all', kind: 'all', who: 'all', pick: [481, 482], base: 481, head: 482, ctab: 'failing', thr: 20 };
  VC.runsState = R;
  const runOf = (n) => X.runs.find((r) => r.n === +n);
  const STAT = { pass: 'passed', fail: 'failed', blocked: 'blocked' };

  /* ---- list ------------------------------------------------------------------------ */
  function matches(r) {
    const q = R.q.trim().toLowerCase();
    if (q && !`#${r.n} ${r.sel} ${r.branch} ${r.commit} ${r.by} ${r.profile}`.toLowerCase().includes(q)) return false;
    if (R.st !== 'all' && (R.st === 'fail' ? r.st === 'pass' : r.st !== 'pass')) return false;
    if (R.profile !== 'all' && r.profile !== R.profile) return false;
    if (R.branch !== 'all' && r.branch !== R.branch) return false;
    if (R.kind !== 'all' && r.kind !== R.kind) return false;
    if (R.who === 'agents' && !SH.isAgent(r.by)) return false;
    if (R.who === 'people' && (SH.isAgent(r.by) || r.by === 'ci')) return false;
    if (R.who === 'ci' && r.by !== 'ci') return false;
    return true;
  }
  function results(r) {
    if (r.kind === 'adhoc') return `<span class="hs" data-class="${String(r.code)[0]}">${r.code}</span> <span class="subtle">one call</span>`;
    if (r.kind === 'load') return `<span class="stack-tight"><span class="num">${r.load.iters.toLocaleString('en-US')} iterations · ${r.load.errs} errors</span>${r.load.breach ? `<span class="rn-breach">${ic('warn', 'ico-sm')}${esc(r.load.breach)}</span>` : '<span class="subtle">thresholds held</span>'}</span>`;
    return `<span class="rn-res">${SH.segbar(r.tot)}<span class="rn-counts">${SH.counts(r.tot, ['pass', 'fail', 'blocked'])}</span></span>`;
  }
  function statusCell(r) {
    if (r.kind === 'load') return badge(r.st === 'pass' ? 'pass' : 'fail', r.st === 'pass' ? 'Held' : 'Breached', true);
    if (r.kind === 'adhoc') return badge(r.st, r.st === 'pass' ? 'OK' : 'Error', true);
    return badge(r.st, null, true);
  }
  function listRows() {
    const rows = X.runs.filter(matches);
    if (!rows.length) return '';
    return rows.map((r) => `<tr class="rn-row" data-action="rn-open" data-n="${r.n}"${R.pick.includes(r.n) ? ' aria-selected="true"' : ''}>
      <td data-action="sh-noop"><input type="checkbox" class="rn-pick" data-n="${r.n}" aria-label="Pick run ${r.n} to compare" ${R.pick.includes(r.n) ? 'checked' : ''} ${r.kind !== 'test' ? 'disabled' : ''}></td>
      <td><button class="btn btn-ghost btn-sm mono" data-action="rn-open" data-n="${r.n}" data-fk="rn-${r.n}" aria-label="Open run ${r.n}">#${r.n}</button></td>
      <td>${statusCell(r)}</td>
      <td><span class="rn-sel"><span class="mono ellipsis">${esc(r.sel)}</span><span class="chip">${r.kind}</span></span></td>
      <td class="rn-opt">${SH.by(r.by)}</td>
      <td class="rn-opt"><span class="chip-src chip" data-src="profile">${esc([r.profile].concat(r.layers).join(' + '))}</span></td>
      <td class="hide-narrow"><span class="rn-branch" title="${esc(r.branch)} @ ${esc(r.commit)}">${ic('branch', 'ico-sm')}<span class="ellipsis">${esc(r.branch)}</span><span class="subtle mono">${esc(r.commit)}</span></span></td>
      <td>${results(r)}</td>
      <td class="r mono muted">${esc(r.dur)}</td>
      <td class="r muted hide-narrow" title="${esc(r.when)}">${esc(r.ago)}</td></tr>`).join('');
  }
  function listBody() {
    const rows = listRows();
    if (!rows) return SH.empty('runs', 'No runs match', 'Clear a filter, or run something: <code>vc run users.crud</code>.');
    return SH.table(['<span class="sr-only">Compare</span>', 'Run', 'Status', 'Selection', ['By', 'rn-opt'], ['Profile', 'rn-opt'], ['Branch', 'hide-narrow'], 'Results', ['Duration', 'r'], ['Started', 'r hide-narrow']], [rows], 'rn-table');
  }
  const opt = (v, cur, l) => `<option value="${esc(v)}"${v === cur ? ' selected' : ''}>${esc(l || v)}</option>`;
  function filters() {
    const branches = [...new Set(X.runs.map((r) => r.branch))];
    return `<div class="sh-filters" role="group" aria-label="Filter runs">
      <div class="input-group rn-search">${ic('search')}<input class="input" id="rn-q" value="${esc(R.q)}" placeholder="Run, selector, commit or agent" aria-label="Search runs" data-fk="rn-q"></div>
      ${SH.seg('Status', [['all', 'All'], ['fail', 'Not passed'], ['pass', 'Passed']], R.st, 'rn-st')}
      ${SH.seg('Kind', [['all', 'All'], ['test', 'Test'], ['load', 'Load'], ['adhoc', 'Adhoc']], R.kind, 'rn-kind')}
      <label class="sh-sel"><span class="subtle">Profile</span><select class="select" id="rn-profile" data-fk="rn-profile">${opt('all', R.profile, 'Any')}${D.profiles.map((p) => opt(p.id, R.profile)).join('')}</select></label>
      <label class="sh-sel"><span class="subtle">Branch</span><select class="select" id="rn-branch" data-fk="rn-branch">${opt('all', R.branch, 'Any')}${branches.map((b) => opt(b, R.branch)).join('')}</select></label>
      <label class="sh-sel"><span class="subtle">By</span><select class="select" id="rn-who" data-fk="rn-who">${opt('all', R.who, 'Anyone')}${opt('people', R.who, 'People')}${opt('agents', R.who, 'Agents')}${opt('ci', R.who, 'CI')}</select></label>
    </div>`;
  }
  function listView() {
    const [a, b] = R.pick.slice().sort((x, y) => x - y);
    return `<div class="stage-one sh-scope"><section class="glass stage-one" aria-label="Runs">
      <div class="card-head"><div class="grow"><h2 class="card-title">Runs</h2><div class="card-sub">Every run in history.db: tests, load and adhoc calls, from you, CI and agents. A stored run opens in the same view as a live one.</div></div>
        <button class="btn btn-sm" data-action="rn-compare" data-fk="rn-compare" ${R.pick.length === 2 ? '' : 'disabled'} title="Pick two test runs with the checkboxes">${ic('columns')}${R.pick.length === 2 ? `Compare #${a} → #${b}` : `Compare · pick ${2 - R.pick.length} more`}</button>
        <button class="btn btn-primary btn-sm" data-action="rn-run" data-fk="rn-run">${ic('play')}Run…<span class="kbd hide-narrow">⌘R</span></button></div>
      <div class="card-tabs">${filters()}</div>
      <div class="card-body" data-sk="rn-list" id="rn-list">${listBody()}</div>
      <div class="card-foot">${ic('db', 'ico-sm')}<span>${X.runs.length} of 482 runs shown · history.keep: 50 · bodies of failures only</span><span class="grow"></span><span>${ic('bot', 'ico-sm')} agent runs carry their agent's name</span></div>
    </section></div>`;
  }

  /* ---- one stored run ---------------------------------------------------------------- */
  const det = () => X.runDetail[R.run] || X.runDetail[482];
  const testRec = (id) => det().tests[id];
  function fileTot(f) { const t = {}; f.tests.forEach(([, st]) => { t[st] = (t[st] || 0) + 1; }); return t; }
  function filesPane() {
    const files = det().files.map((f) => {
      const tests = f.tests.filter(([, st]) => R.show === 'all' || st !== 'pass');
      if (!tests.length) return '';
      const tot = fileTot(f), passed = (tot.pass || 0) + (tot.flaky || 0) + (tot.allow || 0);
      return `<li class="rn-file"><div class="rn-file-head"><span class="mono ellipsis grow">${esc(f.id)}</span><span class="subtle num">${passed}/${f.tests.length}</span><span class="subtle mono num">${f.ms ? (f.ms / 1000).toFixed(1) + ' s' : '—'}</span></div>${SH.segbar(tot, 'sh-seg-dense')}
        <ul role="list">${tests.map(([id, st, ms]) => { const full = `${f.id}#${id}`; return `<li><button class="rn-t" data-st="${st}" data-action="rn-test" data-id="${esc(full)}" aria-current="${R.test === full}" data-fk="rt-${esc(full)}">${ic(st, 'ico-sm')}<span class="sr-only">${SH.WORD[st]}: </span><span class="mono ellipsis grow">${esc(id)}</span><span class="subtle mono num">${ms ? ms + ' ms' : ''}</span></button></li>`; }).join('')}</ul></li>`;
    }).join('');
    return `<div class="rn-files glass-dense" data-sk="rn-files"><div class="rn-files-head">${SH.seg('Show', [['all', 'All tests'], ['bad', 'Not passed']], R.show, 'rn-show')}</div><ul class="rn-file-list" role="list" aria-label="Tests by file">${files}</ul></div>`;
  }
  function sourceSpan(sp) {
    return `<div class="rn-span glass-dense" role="region" aria-label="Source ${esc(sp.file)}:${sp.line}"><div class="rn-span-head">${ic('file', 'ico-sm')}<span class="mono">${esc(sp.file)}:${sp.line}</span></div>
      <pre class="code">${sp.lines.map(([n, txt, hit]) => `<span class="rn-ln${hit ? ' is-hit' : ''}"><span class="n">${n}</span>${VC.h.hlYaml(txt)}</span>`).join('\n')}</pre></div>`;
  }
  function generic(id) {
    const st = det().files.flatMap((f) => f.tests.map(([t, s, ms]) => [`${f.id}#${t}`, s, ms])).find((x) => x[0] === id) || [id, 'pass', 0];
    return { st: st[1], ms: st[2], checks: [{ st: 'pass', what: 'status', exp: '2xx', got: '200' }, { st: 'pass', what: 'body', exp: 'matches users#get schema', got: 'ok' }], exchanges: [{ m: 'GET', url: '${baseUrl}/users/u_8f2c41', s: 200, ms: st[2] || 61, note: 'final' }], reqHeaders: [['authorization', 'Bearer ••••']] };
  }
  function checksPanel(t, st) {
    if (st === 'blocked') return `<p class="callout" data-tone="warn">${ic('blocked')}<span><b>${esc(t.code)} · ${esc(t.title)}</b> It sent nothing. <button class="btn btn-sm" data-action="rn-test" data-id="${esc(t.blockedBy)}">${ic('arrow-right')}Open ${esc(t.blockedBy.split('#')[1])}</button></span></p>`;
    if (st === 'skipped') return `<p class="callout" data-tone="info">${ic('skipped')}<span><b>Skipped.</b> reason: "identity sandbox rotates keys, see #198" · until 2026-11-25. <code>vc run --include-skipped</code> runs it anyway.</span></p>`;
    const rows = t.checks.map((c) => `<tr><td>${badge(c.st, null, true)}</td><td class="mono">${esc(c.what)}</td><td class="mono">${VC.h.hlVars(c.exp)}</td><td class="mono${c.st === 'fail' ? ' rn-got' : ''}">${esc(c.got)}</td></tr>`);
    return `${t.code ? `<p class="callout" data-tone="danger">${ic('fail')}<span><b>${esc(t.code)} · ${esc(t.title)}</b> help: ${esc(t.help)}</span></p>` : ''}
      ${SH.table(['Result', 'Check', 'Expected', 'Actual'], rows)}${t.span ? sourceSpan(t.span) : ''}`;
  }
  function exchangesPanel(t) {
    const rows = t.exchanges.map((x) => `<tr><td>${method(x.m)}</td><td class="mono ellipsis">${VC.h.hlVars(x.url)}</td><td><span class="hs" data-class="${String(x.s)[0]}">${x.s}</span></td><td class="r mono muted">${x.ms} ms</td><td class="muted">${esc(x.note || '')}</td></tr>`);
    return `${SH.table(['Method', 'URL', 'Status', ['Time', 'r'], 'Step'], rows)}
      <section class="layer" aria-label="Final request headers"><div class="layer-head"><h3 class="section-label">Final request · headers</h3><span class="subtle">redacted before they were stored</span></div>
      <div class="kv glass-dense">${t.reqHeaders.map(([k, v]) => `<div class="kv-row"><span class="k">${esc(k)}</span><span class="v">${v.includes('••••') ? esc(v).replace('••••', '<span class="redacted">••••</span>') : esc(v)}</span><span class="s"></span></div>`).join('')}</div></section>`;
  }
  function detailPane() {
    const rec = testRec(R.test), g = generic(R.test), t = rec || g, st = g.st;
    const tabs = [['checks', 'Checks', t.checks.length, st === 'fail' ? 'is-bad' : st === 'pass' ? 'is-good' : '']];
    if (t.exchanges) tabs.push(['exchanges', 'Exchanges', t.exchanges.length]);
    if (t.diff) tabs.push(['diff', 'Body diff']);
    if (!tabs.find((x) => x[0] === R.dtab)) R.dtab = 'checks';
    const panel = R.dtab === 'exchanges' ? exchangesPanel(t) : R.dtab === 'diff' ? `<p class="subtle">Expected body (from the api's example and this test's expect) against what came back on the final attempt.</p>${VC.diffHtml(t.diff, 'Body diff')}` : checksPanel(t, st);
    return `<section class="rn-detail" aria-label="Test ${esc(R.test)}">
      <div class="rn-detail-head"><div class="grow stack-tight"><div class="row row-wrap">${badge(st, null, true)}<h3 class="card-title mono ellipsis">${esc(R.test)}</h3></div>
        <span class="subtle mono">${t.span ? `${esc(t.span.file)}:${t.span.line}` : 'services/' + esc(R.test.split('.')[0]) + '/' + esc(R.test.split('.')[1].split('#')[0]) + '.test.yaml'}${g.ms ? ` · ${g.ms} ms` : ''}</span></div>
        <button class="btn btn-sm" data-action="rn-rerun-one" data-fk="rn-r1">${ic('play')}Run</button>
        ${st === 'fail' || st === 'flaky' ? `<button class="btn btn-sm" data-action="rn-quarantine" data-fk="rn-q1">${ic('allow')}Quarantine</button>` : ''}</div>
      <div class="rn-detail-tabs">${SH.tabs('rd', 'Test result', tabs, R.dtab, 'rn-dtab')}</div>
      <div class="rn-detail-body" role="tabpanel" id="rd-panel" aria-labelledby="rd-${R.dtab}" tabindex="0" data-sk="rd-panel">${panel}</div></section>`;
  }
  function runView() {
    const r = runOf(R.run), tot = r.tot;
    return `<div class="stage-one sh-scope"><section class="glass stage-one" aria-label="Run ${r.n}">
      <div class="card-head"><button class="btn btn-ghost btn-sm" data-action="rn-back" data-fk="rn-back">${ic('arrow-left')}Runs</button>
        <div class="grow stack-tight"><div class="row row-wrap"><h2 class="card-title">Run #${r.n}</h2>${badge(r.st, null, true)}${SH.isAgent(r.by) ? SH.agent(r.by) : ''}</div>
          <div class="card-sub"><span class="mono">vc run ${esc(r.sel)} --profile ${esc([r.profile].concat(r.layers).join(','))}</span><span>·</span>${ic('branch', 'ico-sm')}<span>${esc(r.branch)} <span class="mono">${esc(r.commit)}</span></span><span>·</span><span>${esc(r.when)} · ${esc(r.dur)}</span></div></div>
        <button class="btn btn-sm" data-action="rn-compare-prev" data-fk="rn-cp">${ic('columns')}Compare with #${r.n - 1}</button>
        <button class="btn btn-sm hide-narrow" data-action="rn-report" data-fk="rn-rep">${ic('external')}Open report</button>
        <button class="btn btn-primary btn-sm" data-action="rn-rerun" data-fk="rn-rerun">${ic('refresh')}Re-run failed</button></div>
      <div class="card-tabs rn-summary" aria-label="Summary"><span class="rn-sum-counts">${SH.counts(tot)}</span>${SH.segbar(tot, 'rn-sum-bar')}<span class="subtle num">${Object.values(tot).reduce((a, b) => a + b, 0)} tests · concurrency 32 · seed 4127</span></div>
      <div class="card-body rn-run">${filesPane()}${detailPane()}</div>
    </section></div>`;
  }

  /* ---- compare ------------------------------------------------------------------------ */
  const C = X.compare;
  const pct = (x) => Math.round(((x.b - x.a) / x.a) * 100);
  const slower = () => C.timing.filter((x) => pct(x) >= R.thr && x.b - x.a >= 50);
  const faster = () => C.timing.filter((x) => -pct(x) >= R.thr && x.a - x.b >= 30);
  function idCell(id) { return `<td><button class="btn btn-ghost btn-sm mono" data-action="rn-test-in" data-id="${esc(id)}">${esc(id)}</button></td>`; }
  function cmpPanel() {
    if (R.ctab === 'failing' || R.ctab === 'fixed') {
      const list = R.ctab === 'failing' ? C.newlyFailing : C.fixed;
      return SH.table(['Test', `#${R.base}`, `#${R.head}`, 'What changed'], list.map((x) => `<tr>${idCell(x.id)}<td>${badge(x.was, null, true)}</td><td>${badge(x.now, null, true)}</td><td class="muted">${esc(x.why)}</td></tr>`));
    }
    if (R.ctab === 'shapes') {
      const tone = { added: 'pass', removed: 'fail', type: 'blocked' };
      return `<p class="subtle">Fields whose presence or type differs between the two runs' stored bodies, per api. A shape change with no failing check often means a check is missing.</p>
        ${SH.table(['Api', 'Path', 'Change', 'Type', 'Seen in'], C.shapes.map((x) => `<tr><td class="mono">${esc(x.api)}</td><td class="mono">${esc(x.path)}</td><td><span class="badge st-${tone[x.change]} badge-sm">${x.change === 'added' ? '+ added' : x.change === 'removed' ? '− removed' : '~ type'}</span></td><td class="mono muted">${esc(x.type)}</td><td class="muted">${esc(x.seen)}</td></tr>`))}`;
    }
    const list = R.ctab === 'slower' ? slower() : faster();
    const thr = `<div class="row row-wrap"><span class="subtle">Threshold</span>${SH.seg('Threshold', [[10, '10%'], [20, '20%'], [50, '50%']].map(([k, l]) => [String(k), l]), String(R.thr), 'rn-thr')}<span class="subtle">and at least ${R.ctab === 'slower' ? 50 : 30} ms</span></div>`;
    if (!list.length) return thr + SH.empty('clock', `Nothing ${R.ctab} by ${R.thr}% or more`, 'Lower the threshold to see smaller changes.');
    return thr + SH.table(['Test', [`#${R.base}`, 'r'], [`#${R.head}`, 'r'], ['Change', 'r']], list.map((x) => `<tr>${idCell(x.id)}<td class="r mono muted">${x.a} ms</td><td class="r mono">${x.b} ms</td><td class="r"><span class="rn-delta" data-dir="${x.b > x.a ? 'up' : 'down'}">${ic(x.b > x.a ? 'up' : 'down', 'ico-sm')}${x.b > x.a ? '+' : '−'}${Math.abs(pct(x))}%</span></td></tr>`));
  }
  function compareView() {
    const tests = X.runs.filter((r) => r.kind === 'test');
    const sel = (id, cur, label) => `<label class="sh-sel"><span class="subtle">${label}</span><select class="select mono" id="${id}" data-fk="${id}">${tests.map((r) => `<option value="${r.n}"${r.n === cur ? ' selected' : ''}>#${r.n} · ${esc(r.ago)} · ${esc(r.branch)}</option>`).join('')}</select></label>`;
    const tabs = [['failing', 'Newly failing', C.newlyFailing.length, 'is-bad'], ['fixed', 'Fixed', C.fixed.length, 'is-good'], ['slower', 'Slower', slower().length], ['faster', 'Faster', faster().length], ['shapes', 'Response shapes', C.shapes.length]];
    const a = runOf(R.base), b = runOf(R.head);
    return `<div class="stage-one sh-scope"><section class="glass stage-one" aria-label="Compare runs">
      <div class="card-head"><button class="btn btn-ghost btn-sm" data-action="rn-back" data-fk="rn-back">${ic('arrow-left')}Runs</button>
        <div class="grow"><h2 class="card-title">Compare runs</h2><div class="card-sub">Same as <code>vc history compare ${R.base} ${R.head}</code>. Tests are matched by full ID and example row.</div></div>
        <div class="row row-wrap rn-pair">${sel('rn-base', R.base, 'Base')}<button class="btn btn-ghost btn-icon btn-sm" data-action="rn-swap" aria-label="Swap base and head" title="Swap" data-fk="rn-swap">${ic('arrow-right')}</button>${sel('rn-head', R.head, 'Head')}</div></div>
      <div class="rn-cmp-sum">
        <div class="rn-cmp-run"><span class="subtle">Base</span><b class="mono">#${a.n}</b>${badge(a.st, null, true)}<span class="rn-counts">${SH.counts(a.tot, ['pass', 'fail', 'blocked'])}</span><span class="subtle mono">${esc(a.dur)}</span></div>
        <span class="rn-cmp-arrow" aria-hidden="true">${ic('arrow-right')}</span>
        <div class="rn-cmp-run"><span class="subtle">Head</span><b class="mono">#${b.n}</b>${badge(b.st, null, true)}<span class="rn-counts">${SH.counts(b.tot, ['pass', 'fail', 'blocked'])}</span><span class="subtle mono">${esc(b.dur)}</span></div>
      </div>
      <div class="card-tabs">${SH.tabs('rc', 'Differences', tabs, R.ctab, 'rn-ctab')}</div>
      <div class="card-body" role="tabpanel" id="rc-panel" aria-labelledby="rc-${R.ctab}" tabindex="0" data-sk="rc-panel">${cmpPanel()}</div>
    </section></div>`;
  }

  /* ---- screen ---------------------------------------------------------------------------- */
  VC.screens.runs = {
    crumbs: () => [D.project.name, 'Runs'].concat(R.view === 'run' ? [`#${R.run}`] : R.view === 'compare' ? [`#${R.base} → #${R.head}`] : []),
    status: () => (R.view === 'run' ? `${ic('runs', 'ico-sm')} stored run · snapshot of the compiled program · report in .vibranium/reports/${R.run}/` : `${ic('db', 'ico-sm')} history.db · 482 runs · keep 50`),
    render: () => (R.view === 'run' ? runView() : R.view === 'compare' ? compareView() : listView()),
    mount: (view) => SH.sizes(view),
    view: (v) => { R.view = v === 'compare' || v === 'run' ? v : 'list'; R.ctab = 'failing'; R.dtab = 'checks'; }
  };
  const rerender = () => VC.render();
  A['rn-open'] = (el) => {
    const r = runOf(el.dataset.n);
    if (r.kind === 'adhoc') return VC.openApi('payments#charge', { toast: `Opened call #${r.n} from history` });
    if (r.kind === 'load') return VC.toast(`Load run #${r.n} · ${r.sel}`, 'Load reports open in Tests → Load (latency, RPS, breaches).');
    R.view = 'run'; R.run = X.runDetail[r.n] ? r.n : 482; if (r.n !== 482) VC.toast(`Run #${r.n}`, 'Sample detail: the mockup stores one full run (#482).');
    R.test = 'orders.checkout#rejectsDeclinedCard'; R.dtab = 'checks'; rerender();
  };
  A['rn-back'] = () => { R.view = 'list'; rerender(); };
  A['rn-st'] = (el) => { R.st = el.dataset.v; rerender(); };
  A['rn-kind'] = (el) => { R.kind = el.dataset.v; rerender(); };
  A['rn-show'] = (el) => { R.show = el.dataset.v; rerender(); };
  A['rn-test'] = (el) => { R.test = el.dataset.id; R.dtab = 'checks'; rerender(); };
  A['rn-test-in'] = (el) => { R.view = 'run'; R.run = R.head; R.test = el.dataset.id; R.dtab = 'checks'; rerender(); };
  A['rn-dtab'] = (el) => { R.dtab = el.dataset.v; rerender(); };
  A['rn-ctab'] = (el) => { R.ctab = el.dataset.v; rerender(); };
  A['rn-thr'] = (el) => { R.thr = +el.dataset.v; rerender(); };
  A['rn-compare'] = () => { if (R.pick.length !== 2) return; [R.base, R.head] = R.pick.slice().sort((x, y) => x - y); R.view = 'compare'; R.ctab = 'failing'; rerender(); };
  A['rn-compare-prev'] = () => { R.base = R.run - 1; R.head = R.run; R.view = 'compare'; R.ctab = 'failing'; rerender(); };
  A['rn-swap'] = () => { [R.base, R.head] = [R.head, R.base]; rerender(); };
  A['rn-run'] = () => VC.toast('Run dialog', 'Opens from Tests mode: selector, profiles, seed, repeat, bail, concurrency.');
  A['rn-report'] = () => VC.toast('Opened report', `.vibranium/reports/${R.run}/index.html`);
  A['rn-rerun'] = () => VC.toast('Re-running 3 tests', `vc run --failed ${R.run} --profile local,eu · results stream into a new run #483`);
  A['rn-rerun-one'] = () => VC.toast(`Running ${R.test}`, 'vc run ' + R.test + ' · its beforeAll runs first');
  A['rn-quarantine'] = () => SH.quarantine(R.test);
  /* Quarantine = allowFailure with a reason and an until date, written as a diff (ui.md §5) */
  SH.quarantine = (id, after) => {
    const [file, test] = id.split('#'), [svc, area] = file.split('.');
    const until = '2026-12-07';
    VC.diffDialog({
      title: `Quarantine ${test}`, sub: 'It still runs and is reported, but a failure no longer fails the run, until the date below.',
      fields: `<div class="sh-fields"><div class="field"><label for="q-reason">Reason</label><input class="input" id="q-reason" value="flaky on staging, see #352"><span class="hint">Shown in every report next to ⚠.</span></div>
        <div class="field"><label for="q-until">Until</label><input class="input mono" id="q-until" value="${until}"><span class="hint">After this day it fails the run again (V4008).</span></div></div>`,
      file: `services/${svc}/${area}.test.yaml`,
      lines: [['@', '@@ -36,4 +36,7 @@'], [' ', `  - id: ${test}`], [' ', '    call: payments#charge'], ['+', '    allowFailure:'], ['+', '      reason: "flaky on staging, see #352"'], ['+', `      until: ${until}`], [' ', '    expect:']],
      confirm: 'Write and quarantine', callout: 'The file digest is checked before writing; if it changed since you opened it, you get Reload or Keep mine.',
      after: () => { if (after) after(); VC.render(); }
    });
  };

  document.addEventListener('input', (e) => {
    if (e.target.id !== 'rn-q') return;
    R.q = e.target.value;
    const b = $('#rn-list');
    if (b) { b.innerHTML = listBody(); SH.sizes(b); }
  });
  document.addEventListener('change', (e) => {
    const id = e.target.id;
    if (id === 'rn-profile' || id === 'rn-branch' || id === 'rn-who') { R[id.slice(3)] = e.target.value; rerender(); }
    if (id === 'rn-base' || id === 'rn-head') { R[id.slice(3)] = +e.target.value; rerender(); }
    if (e.target.classList.contains('rn-pick')) {
      const n = +e.target.dataset.n;
      R.pick = e.target.checked ? R.pick.filter((x) => x !== n).concat(n).slice(-2) : R.pick.filter((x) => x !== n);
      rerender();
      const cb = $(`.rn-pick[data-n="${n}"]`); if (cb) cb.focus();
    }
  });
})();
