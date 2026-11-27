/* Runs space · All runs (#runs): every API, load, scheduled and ad hoc run, one stored run
   (same layout as a live run's drill-down) and Compare two runs. Also the Runs sidebar
   (Overview, All runs, Schedules, Recent) and VC.sh, the helpers the shared screens reuse. */
(function () {
  'use strict';
  const VC = window.VC, D = VC.data, X = D.shared, A = VC.actions, St = VC.state;
  const { $, $$, esc, ic, badge, method } = VC.h;

  /* ---- shared helpers (Insights, Agents and Settings use these too) ------------ */
  const SH = VC.sh = {};
  SH.AGENTS = ['claude-code', 'cursor'];
  SH.isAgent = (who) => SH.AGENTS.includes(who);
  SH.agent = (who) => `<span class="sh-agent" title="An AI agent, over MCP">${ic('bot', 'ico-sm')}${esc(who)}</span>`;
  /* plain text in rows (D3: the row's one badge is its status); the pill is for headers */
  SH.by = (who) => (SH.isAgent(who) ? `<span class="sh-by" title="An AI agent, over MCP">${ic('bot', 'ico-sm')}${esc(who)}</span>` : `<span class="muted">${who === 'ci' ? 'CI' : esc(who)}</span>`);
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
  SH.empty = (icon, title, sub) => `<div class="empty empty-quiet"><span>${esc(title)}</span>${sub ? `<span class="subtle">${sub}</span>` : ''}</div>`;
  SH.store = {
    get(k, d) { try { const v = JSON.parse(localStorage.getItem('vc-ui-v2-shared') || '{}'); return v[k] === undefined ? d : v[k]; } catch (e) { return d; } },
    set(k, val) { try { const v = JSON.parse(localStorage.getItem('vc-ui-v2-shared') || '{}'); v[k] = val; localStorage.setItem('vc-ui-v2-shared', JSON.stringify(v)); } catch (e) { /* storage may be blocked */ } }
  };
  A['sh-noop'] = () => {};
  /* the Runs space's one primary, in the top row: opens the Run dialog (⌘R) */
  SH.runBtn = () => `<button class="btn btn-primary btn-sm" data-action="rn-run" data-fk="top-run">${ic('play', 'ico-sm')}Run…<span class="kbd hide-narrow">⌘R</span></button>`;

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
    if (R.kind === 'sched' ? !r.sched : R.kind !== 'all' && r.kind !== R.kind) return false;
    if (R.who === 'agents' && !SH.isAgent(r.by)) return false;
    if (R.who === 'people' && (SH.isAgent(r.by) || r.by === 'ci')) return false;
    if (R.who === 'ci' && r.by !== 'ci') return false;
    return true;
  }
  function results(r) {
    if (r.kind === 'adhoc') return `<span class="hs" data-class="${String(r.code)[0]}">${r.code}</span> <span class="subtle">one call</span>`;
    if (r.kind === 'load') return `<span class="rn-res" title="${esc(r.load.breach ? 'Breached: ' + r.load.breach : 'Thresholds held')}"><span class="num">${r.load.iters.toLocaleString('en-US')} iterations</span><span class="rn-counts"><span class="sh-count" data-st="fail">${ic('fail', 'ico-sm')}<b>${r.load.errs}</b><span class="sr-only"> errors</span></span></span>${r.load.breach ? `<span class="rn-breach">${ic('warn', 'ico-sm')}<span class="sr-only">${esc(r.load.breach)}</span></span>` : ''}</span>`;
    return `<span class="rn-res">${SH.segbar(r.tot)}<span class="rn-counts">${SH.counts(r.tot, ['pass', 'fail', 'blocked'])}</span></span>`;
  }
  function statusCell(r) {
    if (r.kind === 'load') return badge(r.st === 'pass' ? 'pass' : 'fail', r.st === 'pass' ? 'Held' : 'Breached', true);
    if (r.kind === 'adhoc') return badge(r.st, r.st === 'pass' ? 'OK' : 'Error', true);
    return badge(r.st, null, true);
  }
  const kindTag = (r) => (r.sched ? `<span class="subtle rn-kind" title="Schedule ${esc(r.sched)}">${ic('clock', 'ico-sm')}${esc(r.sched)}</span>` : r.kind === 'test' ? '' : `<span class="subtle">${r.kind === 'load' ? 'load' : 'one call'}</span>`);
  /* the live run sits on top until it finishes and joins the history */
  /* the branch rides under the selection, so the table keeps Started in view at any width */
  const branchLine = (b, c) => `<span class="rn-branch" title="${esc(c ? `${b} @ ${c}` : b)}">${ic('branch', 'ico-sm')}<span class="ellipsis">${esc(b)}</span></span>`;
  function liveRow() {
    const L = VC.run;
    if (!L || !(L.phase === 'running' || L.phase === 'stopping') || (R.kind !== 'all' && R.kind !== 'test') || R.st === 'pass') return '';
    return `<tr class="rn-row" data-action="rn-recent" data-n="${L.n}"><td class="rn-pickc"></td><td><button class="btn btn-ghost btn-sm mono" data-action="rn-recent" data-n="${L.n}" data-fk="rn-${L.n}" aria-label="Open live run ${L.n}">#${L.n}</button></td>
      <td><span class="badge badge-sm st-running"><span class="dot is-live" aria-hidden="true"></span>Running</span></td><td><span class="rn-sel2"><span class="rn-sel"><span class="mono ellipsis">${esc(L.opts.selector)}</span></span>${branchLine(D.project.branch)}</span></td>
      <td class="rn-opt rn-by">${SH.by('you')}</td><td class="rn-opt rn-wide"><span class="mono muted">${esc([L.opts.profile].concat(L.opts.layers).join(' + '))}</span></td>
      <td><span class="num">${L.results.length} of ${L.plan.length}</span> <span class="subtle">so far</span></td><td class="r mono muted rn-opt">…</td><td class="r muted">now</td><td class="r rn-act"><span class="rn-go rn-go-narrow" aria-hidden="true">${ic('chev-right', 'ico-sm')}</span></td></tr>`;
  }
  function listRows() {
    const rows = X.runs.filter(matches);
    if (!rows.length) return liveRow();
    return liveRow() + rows.map((r) => `<tr class="rn-row" data-action="rn-open" data-n="${r.n}"${R.pick.includes(r.n) ? ' aria-selected="true"' : ''}>
      <td class="rn-pickc" data-action="sh-noop"><input type="checkbox" class="rn-pick" data-n="${r.n}" aria-label="Pick run ${r.n} to compare" ${R.pick.includes(r.n) ? 'checked' : ''} ${r.kind !== 'test' ? 'disabled' : ''}></td>
      <td><button class="btn btn-ghost btn-sm mono" data-action="rn-open" data-n="${r.n}" data-fk="rn-${r.n}" aria-label="Open run ${r.n}" title="${esc(`${r.dur} · ${r.branch} · ${r.when}`)}">#${r.n}</button></td>
      <td>${statusCell(r)}</td>
      <td><span class="rn-sel2"><span class="rn-sel" title="${esc(cmdOf(r))}"><span class="mono ellipsis">${esc(r.sel)}</span>${kindTag(r)}</span>${branchLine(r.branch, r.commit)}</span></td>
      <td class="rn-opt rn-by">${SH.by(r.by)}</td>
      <td class="rn-opt rn-wide"><span class="mono muted">${esc([r.profile].concat(r.layers).join(' + '))}</span></td>
      <td>${results(r)}</td>
      <td class="r mono muted rn-opt">${esc(r.dur)}</td>
      <td class="r muted" title="${esc(r.when)}"><span class="rn-swap"><span class="rn-ago">${esc(r.ago)}</span><span class="rn-go" aria-hidden="true">Open${ic('chev-right', 'ico-sm')}</span></span></td>
      <td class="r rn-act"><span class="rn-go rn-go-narrow" aria-hidden="true">${ic('chev-right', 'ico-sm')}</span>${VC.moreBtn('rn-row-more', `Run ${r.n} actions`, `rn-m-${r.n}`).replace('data-action', `data-n="${r.n}" data-action`)}</td></tr>`).join('');
  }
  function listBody() {
    const rows = listRows();
    if (!rows) return SH.empty('runs', 'No runs match these filters.', '<button class="link" data-action="rn-clear" data-fk="rn-clear">Clear filters</button>');
    return SH.table([['<span class="sr-only">Compare</span>', 'rn-pickc'], 'Run', 'Status', 'Selection', ['By', 'rn-opt rn-by'], ['Profile', 'rn-opt rn-wide'], 'Results', ['Duration', 'r rn-opt'], ['Started', 'r'], ['<span class="sr-only">Actions</span>', 'r rn-act']], [rows], 'rn-table fade-end' + (R.pick.length === 1 ? ' is-selecting' : ''));
  }
  const opt = (v, cur, l) => `<option value="${esc(v)}"${v === cur ? ' selected' : ''}>${esc(l || v)}</option>`;
  /* D1 (round 5): search and kind stay visible; status, profile, branch and who sit behind Filters */
  const KINDS = [['all', 'All'], ['test', 'API'], ['load', 'Load'], ['sched', 'Scheduled'], ['adhoc', 'Ad hoc']];
  const MORE_F = ['st', 'profile', 'branch', 'who'];
  const nFilters = () => MORE_F.filter((k) => R[k] !== 'all').length;
  function filters() {
    const n = nFilters();
    return `<div class="sh-filters" role="group" aria-label="Filter runs">
      <div class="input-group rn-search">${ic('search')}<input class="input" id="rn-q" value="${esc(R.q)}" placeholder="Run, selector, commit or agent" aria-label="Search runs" data-fk="rn-q"></div>
      ${SH.seg('Kind', KINDS, R.kind, 'rn-kind')}
      <button class="btn btn-sm" data-action="rn-filters" aria-haspopup="true" data-fk="rn-filters">${ic('filter')}Filters${n ? ` · ${n}` : ''}</button>
    </div>`;
  }
  function openFilters(focusK) {
    const anchor = $('[data-fk="rn-filters"]');
    if (!anchor) return;
    const branches = [...new Set(X.runs.map((r) => r.branch))];
    const field = (k, label, opts) => `<div class="field"><label for="rn-f-${k}">${label}</label><select class="select" id="rn-f-${k}" data-rnf="${k}">${opts.map(([v, l]) => `<option value="${esc(v)}"${R[k] === v ? ' selected' : ''}>${esc(l)}</option>`).join('')}</select></div>`;
    VC.pick(anchor, `<div class="rn-filter" role="group" aria-label="More filters"><div class="form-grid">
      ${field('st', 'Status', [['all', 'Any status'], ['fail', 'Not passed'], ['pass', 'Passed']])}
      ${field('who', 'Run by', [['all', 'Anyone'], ['people', 'People'], ['agents', 'Agents'], ['ci', 'CI']])}
      ${field('profile', 'Profile', [['all', 'Any profile']].concat(D.profiles.map((p) => [p.id, p.id])))}
      ${field('branch', 'Branch', [['all', 'Any branch']].concat(branches.map((x) => [x, x])))}</div>
      <div class="rn-filter-foot"><button class="btn btn-ghost btn-sm" data-action="rn-clear"${nFilters() ? '' : ' disabled'}>${ic('x')}Clear filters</button></div></div>`);
    const f = focusK && $(`#rn-f-${focusK}`); if (f) f.focus();
  }
  function listView() {
    const [a, b] = R.pick.slice().sort((x, y) => x - y);
    return `<div class="stage-one sh-scope"><section class="glass stage-one" aria-label="Runs">
      <div class="card-head"><div class="grow"><h2 class="card-title">All runs</h2><div class="card-sub">API, load, scheduled and ad hoc runs from you, CI, schedules and agents.</div></div>
        <button class="btn btn-sm" data-action="rn-compare" data-fk="rn-compare" ${R.pick.length === 2 ? '' : 'disabled'} title="Pick two API runs with the checkboxes">${ic('columns')}${R.pick.length === 2 ? `Compare #${a} → #${b}` : `Compare · pick ${2 - R.pick.length} more`}</button></div>
      <div class="card-tabs">${filters()}</div>
      <div class="card-body" data-sk="rn-list" id="rn-list">${listBody()}</div>
      <div class="card-foot">${ic('db', 'ico-sm')}<span title="history.keep: 50 · bodies of failures only">482 runs in history.db</span></div>
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
  const disc = (label, meta, body) => `<details class="disc"><summary>${ic('chev-down', 'chev')}${label}${meta ? `<span class="meta">${meta}</span>` : ''}</summary><div class="disc-body">${body}</div></details>`;
  function sourceSpan(sp) {
    return disc('Source', `<span class="mono">${esc(sp.file)}:${sp.line}</span>`, `<div class="rn-span glass-dense" role="region" aria-label="Source ${esc(sp.file)}:${sp.line}">
      <pre class="code">${sp.lines.map(([n, txt, hit]) => `<span class="rn-ln${hit ? ' is-hit' : ''}"><span class="n">${n}</span>${VC.h.hlYaml(txt)}</span>`).join('\n')}</pre></div>`);
  }
  function generic(id) {
    const st = det().files.flatMap((f) => f.tests.map(([t, s, ms]) => [`${f.id}#${t}`, s, ms])).find((x) => x[0] === id) || [id, 'pass', 0];
    return { st: st[1], ms: st[2], checks: [{ st: 'pass', what: 'status', exp: '2xx', got: '200' }, { st: 'pass', what: 'body', exp: 'matches users#get schema', got: 'ok' }], exchanges: [{ m: 'GET', url: '${baseUrl}/users/u_8f2c41', s: 200, ms: st[2] || 61, note: 'final' }], reqHeaders: [['authorization', 'Bearer ••••']] };
  }
  /* D4: the one line that says what happened, above the tabs */
  function lead(t, st) {
    if (st === 'blocked') return `<p class="callout" data-tone="warn">${ic('blocked')}<span><b>${esc(t.code)} · ${esc(t.title)}</b> It sent nothing. <button class="link" data-action="rn-test" data-id="${esc(t.blockedBy)}">Open ${esc(t.blockedBy.split('#')[1])}</button></span></p>`;
    if (st === 'skipped') return `<p class="callout" data-tone="info">${ic('skipped')}<span><b>Skipped</b> until 2026-11-25: "identity sandbox rotates keys, see #198".</span></p>`;
    if (!t.code) return '';
    const [help, fix] = t.help.split(' fix: ');
    const cap = (x) => x.charAt(0).toUpperCase() + x.slice(1);
    return `<p class="callout" data-tone="danger">${ic('fail')}<span class="stack-tight"><b>${esc(t.code)} · ${esc(t.title)}</b><span>${esc(cap(help))}${fix ? ` Fix: ${esc(fix)}` : ''}</span></span></p>`;
  }
  function checksPanel(t, st) {
    if (st === 'blocked' || st === 'skipped') return SH.empty('', 'No checks ran.', st === 'skipped' ? '<code>vc run --include-skipped</code> runs it anyway.' : '');
    const rows = t.checks.map((c) => `<tr><td>${badge(c.st, null, true)}</td><td class="mono">${esc(c.what)}</td><td class="mono" title="${esc(c.exp)}">${VC.h.hlVars(c.exp)}</td><td class="mono${c.st === 'fail' ? ' rn-got' : ''}" title="${esc(c.got)}">${esc(c.got)}</td></tr>`);
    return `${SH.table([['Result', 'rn-c-st'], 'Check', 'Expected', 'Actual'], rows, 'rn-fit')}${t.span ? sourceSpan(t.span) : ''}`;
  }
  function exchangesPanel(t) {
    const rows = t.exchanges.map((x) => `<tr><td>${method(x.m)}</td><td class="mono ellipsis">${VC.h.hlVars(x.url)}</td><td><span class="hs" data-class="${String(x.s)[0]}">${x.s}</span></td><td class="r mono muted">${x.ms} ms</td><td class="muted">${esc(x.note || '')}</td></tr>`);
    return `${SH.table(['Method', 'URL', 'Status', ['Time', 'r'], 'Step'], rows)}
      ${disc('Final request headers', 'redacted before they were stored', `<div class="kv glass-dense">${t.reqHeaders.map(([k, v]) => `<div class="kv-row"><span class="k">${esc(k)}</span><span class="v">${v.includes('••••') ? esc(v).replace('••••', '<span class="redacted">••••</span>') : esc(v)}</span><span class="s"></span></div>`).join('')}</div>`)}`;
  }
  function detailPane() {
    const rec = testRec(R.test), g = generic(R.test), t = rec || g, st = g.st;
    const tabs = [['checks', 'Checks', t.checks.length, st === 'fail' ? 'is-bad' : st === 'pass' ? 'is-good' : '']];
    if (t.exchanges) tabs.push(['exchanges', 'Exchanges', t.exchanges.length]);
    if (t.diff) tabs.push(['diff', 'Body diff']);
    if (!tabs.find((x) => x[0] === R.dtab)) R.dtab = 'checks';
    const panel = R.dtab === 'exchanges' ? exchangesPanel(t) : R.dtab === 'diff' ? `<p class="subtle">Expected body (from the api's example and this test's expect) against what came back on the final attempt.</p>${VC.diffHtml(t.diff, 'Body diff')}` : checksPanel(t, st);
    const path = t.span ? `${t.span.file}:${t.span.line}` : `services/${R.test.split('.')[0]}/${R.test.split('.')[1].split('#')[0]}.test.yaml`;
    return `<section class="rn-detail" aria-label="Test ${esc(R.test)}">
      <div class="rn-detail-head"><div class="grow stack-tight"><div class="row">${badge(st, null, true)}<h3 class="card-title mono ellipsis" title="${esc(path)}">${esc(R.test)}</h3></div>
        ${g.ms ? `<span class="subtle num">${g.ms} ms</span>` : ''}</div>
        <button class="btn btn-sm" data-action="rn-rerun-one" data-fk="rn-r1">${ic('play')}Run</button>
        ${st === 'fail' || st === 'flaky' ? `<button class="btn btn-sm" data-action="rn-quarantine" data-fk="rn-q1">${ic('allow')}Quarantine</button>` : ''}</div>
      ${lead(t, st)}
      <div class="rn-detail-tabs">${SH.tabs('rd', 'Test result', tabs, R.dtab, 'rn-dtab')}</div>
      <div class="rn-detail-body" role="tabpanel" id="rd-panel" aria-labelledby="rd-${R.dtab}" tabindex="0" data-sk="rd-panel">${panel}</div></section>`;
  }
  /* the run's summary: four status counts, then where and when it ran */
  function facts(r) {
    const tot = r.tot, extra = ['flaky', 'allow'].filter((k) => tot[k]).map((k) => `${tot[k]} ${SH.WORD[k]}`).join(', ');
    const fact = (label, value, title) => `<div class="rn-fact"${title ? ` title="${esc(title)}"` : ''}><dt>${label}</dt><dd>${value}</dd></div>`;
    const count = (k, label) => fact(label, `<span class="rn-fact-n" data-st="${k}"${tot[k] ? '' : ' data-zero'}>${ic(k, 'ico-sm')}<b class="num">${tot[k] || 0}</b></span>`, k === 'pass' && extra ? `Also ${extra}` : '');
    return `<dl class="rn-facts" aria-label="Run summary">
      ${count('pass', 'Passed')}${count('fail', 'Failed')}${count('blocked', 'Blocked')}${count('skipped', 'Skipped')}
      <span class="rn-facts-gap" aria-hidden="true"></span>
      ${fact('Duration', `<span class="num">${esc(r.dur)}</span>`, 'concurrency 32 · seed 4127')}
      ${fact('Profile', `<span class="mono">${esc([r.profile].concat(r.layers).join(' + '))}</span>`)}
      ${fact('Branch', `<span class="mono ellipsis">${esc(r.branch)}</span><span class="mono subtle">@ ${esc(r.commit)}</span>`)}
      ${fact('Started', `<span>${esc(r.when)}</span>`, r.ago)}</dl>`;
  }
  function runView() {
    const r = runOf(R.run);
    return `<div class="stage-one sh-scope"><section class="glass stage-one" aria-label="Run ${r.n}">
      <div class="card-head"><button class="btn btn-ghost btn-sm" data-action="rn-back" data-fk="rn-back">${ic('arrow-left')}Runs</button>
        <div class="grow stack-tight"><div class="row row-wrap"><h2 class="card-title">Run #${r.n}</h2>${badge(r.st, null, true)}</div>
          <div class="card-sub"><span class="mono" title="${esc(cmdOf(r))}">${esc(r.sel)}</span><span>·</span><span>by</span>${SH.by(r.by)}</div></div>
        <button class="btn btn-primary btn-sm" data-action="rn-report" data-n="${r.n}" data-fk="rn-report">${ic('external')}Open report</button>
        <button class="btn btn-sm" data-action="rn-export" data-n="${r.n}" aria-haspopup="menu" data-fk="rn-export">Export${ic('chev-down', 'ico-sm')}</button>
        ${VC.moreBtn('rn-more', 'More run actions', 'rn-more').replace('data-action', `data-n="${r.n}" data-action`)}</div>
      <div class="card-tabs rn-summary">${facts(r)}</div>
      <div class="card-body rn-run">${filesPane()}${detailPane()}</div>
    </section></div>`;
  }
  const cmdOf = (r) => `vc run ${r.sel} --profile ${[r.profile].concat(r.layers).join(',')}`;
  const prevTest = (n) => X.runs.filter((r) => r.kind === 'test' && r.n < n).sort((a, b) => b.n - a.n)[0];

  /* ---- compare ------------------------------------------------------------------------ */
  const C = X.compare;
  const pct = (x) => Math.round(((x.b - x.a) / x.a) * 100);
  const slower = () => C.timing.filter((x) => pct(x) >= R.thr && x.b - x.a >= 50);
  const faster = () => C.timing.filter((x) => -pct(x) >= R.thr && x.a - x.b >= 30);
  function idCell(id) { return `<td><button class="btn btn-ghost btn-sm mono rn-id" title="${esc(id)}" data-action="rn-test-in" data-id="${esc(id)}"><span class="ellipsis">${esc(id)}</span></button></td>`; }
  function cmpPanel() {
    if (R.ctab === 'failing' || R.ctab === 'fixed') {
      const list = R.ctab === 'failing' ? C.newlyFailing : C.fixed;
      /* B: one chip per row, the head's; the base status is a dot before it */
      return SH.table(['Test', [`#${R.base} → #${R.head}`, 'rn-c-st2'], ['What changed', 'rn-c-why']], list.map((x) => `<tr>${idCell(x.id)}<td><span class="rn-was" title="#${R.base}: ${esc(STAT[x.was] || x.was)}"><span class="dot st-${x.was}" aria-hidden="true"></span><span class="sr-only">was ${esc(STAT[x.was] || x.was)}, now</span>${ic('arrow-right', 'ico-sm')}</span>${badge(x.now, null, true)}</td><td class="muted" title="${esc(x.why)}">${esc(x.why)}</td></tr>`), 'rn-fit');
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
    const sum = (r) => `#${r.n} · ${SH.totLabel({ pass: r.tot.pass, fail: r.tot.fail, blocked: r.tot.blocked }) || 'no tests'} · ${r.dur}`;
    return `<div class="stage-one sh-scope"><section class="glass stage-one rn-hug" aria-label="Compare runs">
      <div class="card-head"><button class="btn btn-ghost btn-sm" data-action="rn-back" data-fk="rn-back">${ic('arrow-left')}Runs</button>
        <div class="grow"><h2 class="card-title">Compare runs</h2><p class="card-sub rn-cmp-sum" title="Same as vc history compare ${R.base} ${R.head}. Tests matched by full ID and example row.">${esc(sum(a))} → ${esc(sum(b))}</p></div></div>
      <div class="rn-pair">${sel('rn-base', R.base, 'Base')}<button class="btn btn-ghost btn-icon btn-sm" data-action="rn-swap" aria-label="Swap base and head" title="Swap" data-fk="rn-swap">${ic('arrow-right')}</button>${sel('rn-head', R.head, 'Head')}</div>
      <div class="card-tabs">${SH.tabs('rc', 'Differences', tabs, R.ctab, 'rn-ctab')}</div>
      <div class="card-body" role="tabpanel" id="rc-panel" aria-labelledby="rc-${R.ctab}" tabindex="0" data-sk="rc-panel">${cmpPanel()}</div>
    </section></div>`;
  }

  /* ---- screen ---------------------------------------------------------------------------- */
  VC.screens.runs = {
    space: 'runs',
    crumbs: () => [D.project.name, 'Runs'].concat(R.view === 'run' ? [`#${R.run}`] : R.view === 'compare' ? [`#${R.base} → #${R.head}`] : ['All runs']),
    primary: () => SH.runBtn(),
    status: () => (R.view === 'run' ? `${ic('runs', 'ico-sm')} Stored run` : ''),
    render: () => (R.view === 'run' ? runView() : R.view === 'compare' ? compareView() : listView()),
    mount: (view) => SH.sizes(view),
    view: (v) => { R.view = v === 'compare' || v === 'run' ? v : 'list'; R.ctab = 'failing'; R.dtab = 'checks'; }
  };
  const rerender = () => VC.render();
  A['rn-open'] = (el) => {
    const r = runOf(el.dataset.n);
    if (r.kind === 'adhoc') return VC.openApi('payments#charge', { toast: `Opened call #${r.n} from history` });
    if (r.kind === 'load') return VC.openLoadRun ? VC.openLoadRun(r.n) : VC.go('runs-load');
    if (VC.run && r.n === VC.run.n) return VC.go('runs-live');
    R.view = 'run'; R.run = X.runDetail[r.n] ? r.n : 482; if (r.n !== 482) VC.toast(`Run #${r.n}`, 'Sample detail: the mockup stores one full run (#482).');
    R.test = 'orders.checkout#rejectsDeclinedCard'; R.dtab = 'checks'; rerender();
  };
  A['rn-back'] = () => { R.view = 'list'; rerender(); };
  A['rn-st'] = (el) => { R.st = el.dataset.v; rerender(); };
  A['rn-kind'] = (el) => { R.kind = el.dataset.v; if (R.kind !== 'all' && R.kind !== 'test') R.pick = R.pick.filter((n) => (runOf(n) || {}).kind === 'test'); rerender(); };
  A['rn-filters'] = () => openFilters();
  A['rn-clear'] = () => { MORE_F.forEach((k) => { R[k] = 'all'; }); R.kind = 'all'; R.q = ''; VC.hidePopovers(); rerender(); };
  /* report and export: the stored-run header and every list row's ⋯ reach the same actions */
  const nOf = (el) => +(el.dataset.n || R.run);
  const EXPORTS = [
    ['html', 'HTML report', 'index.html', 'Wrote the HTML report'],
    ['ctrf', 'CTRF JSON', 'ctrf.json', 'Wrote CTRF JSON'],
    ['junit', 'JUnit XML', 'junit.xml', 'Wrote JUnit XML'],
    ['ndjson', 'NDJSON events', 'events.ndjson', 'Wrote NDJSON events']
  ];
  const exportMenu = (n) => `<div class="pop-label">Export run #${n}</div>${EXPORTS.map(([k, l, f]) => VC.menuItem(l, 'rn-export-as', `data-n="${n}" data-k="${k}"`, { meta: f })).join('')}
    <div class="pop-sep"></div>${VC.menuItem('Copy <code>vc report</code> command', 'rn-copy-report', `data-n="${n}"`, { ico: 'terminal' })}`;
  A['rn-export'] = (el) => VC.menu(el, exportMenu(nOf(el)));
  A['rn-export-as'] = (el) => {
    const [, , file, msg] = EXPORTS.find((x) => x[0] === el.dataset.k);
    VC.toast(msg, `.vibranium/reports/${nOf(el)}/${file}`);
  };
  const reportCmd = (n) => `vc report ${n} --format html,ctrf,junit --out .vibranium/reports/${n}`;
  A['rn-copy-report'] = (el) => { const c = reportCmd(nOf(el)); VC.copy(c, 'Copied the vc report command', c); };
  A['rn-more'] = (el) => {
    const r = runOf(nOf(el)), p = prevTest(r.n);
    VC.menu(el, VC.menuItem('Re-run failed tests', 'rn-rerun', `data-n="${r.n}"`, { ico: 'refresh' })
      + VC.menuItem(p ? `Compare with #${p.n}` : 'Compare with previous', 'rn-compare-prev', `data-n="${r.n}"`, { ico: 'columns', disabled: !p })
      + VC.menuItem('Copy run command', 'rn-copy-cmd', `data-n="${r.n}"`, { ico: 'terminal' }));
  };
  A['rn-row-more'] = (el) => {
    const r = runOf(el.dataset.n), p = r.kind === 'test' && prevTest(r.n);
    VC.menu(el, VC.menuItem('Open report', 'rn-report', `data-n="${r.n}"`, { ico: 'external' })
      + VC.menuItem('Export…', 'rn-export-from', `data-n="${r.n}"`, { ico: 'file' })
      + VC.menuItem(p ? `Compare with #${p.n}` : 'Compare with previous', 'rn-compare-prev', `data-n="${r.n}"`, { ico: 'columns', disabled: !p })
      + VC.menuItem('Re-run', 'rn-rerun', `data-n="${r.n}"`, { ico: 'refresh' }));
  };
  /* Export… swaps the row menu for the export menu, on the same ⋯ anchor */
  A['rn-export-from'] = (el) => { const a = $(`[data-fk="rn-m-${el.dataset.n}"]`); if (a) VC.menu(a, exportMenu(+el.dataset.n)); };
  A['rn-copy-cmd'] = (el) => { const c = cmdOf(runOf(nOf(el))); VC.copy(c, 'Copied the run command', c); };
  A['rn-show'] = (el) => { R.show = el.dataset.v; rerender(); };
  A['rn-test'] = (el) => { R.test = el.dataset.id; R.dtab = 'checks'; rerender(); };
  A['rn-test-in'] = (el) => { R.view = 'run'; R.run = R.head; R.test = el.dataset.id; R.dtab = 'checks'; rerender(); };
  A['rn-dtab'] = (el) => { R.dtab = el.dataset.v; rerender(); };
  A['rn-ctab'] = (el) => { R.ctab = el.dataset.v; rerender(); };
  A['rn-thr'] = (el) => { R.thr = +el.dataset.v; rerender(); };
  A['rn-compare'] = () => { if (R.pick.length !== 2) return; [R.base, R.head] = R.pick.slice().sort((x, y) => x - y); R.view = 'compare'; R.ctab = 'failing'; rerender(); };
  A['rn-compare-prev'] = (el) => { const n = nOf(el), p = prevTest(n); if (!p) return; R.base = p.n; R.head = n; R.view = 'compare'; R.ctab = 'failing'; rerender(); };
  A['rn-swap'] = () => { [R.base, R.head] = [R.head, R.base]; rerender(); };
  A['rn-run'] = () => VC.openRun();
  /* other screens (the Tests explorer's "Open the check") land on one stored run */
  VC.openRunReport = (n, testId) => { R.view = 'run'; R.run = X.runDetail[n] ? +n : 482; if (testId) R.test = testId; R.dtab = 'checks'; VC.go('runs'); };
  A['rn-report'] = (el) => VC.toast(`Opened the report for run #${nOf(el)}`, `.vibranium/reports/${nOf(el)}/index.html · in your browser`);
  A['rn-rerun'] = (el) => {
    const r = runOf(nOf(el)), failed = r.tot ? (r.tot.fail || 0) + (r.tot.blocked || 0) : 0;
    if (r.kind === 'test' && VC.startRun) return VC.startRun(failed ? { failedOnly: true } : { selector: r.sel });
    VC.toast(`Re-running #${r.n}`, `${cmdOf(r)} · results stream into a new run`);
  };
  A['rn-rerun-one'] = () => VC.toast(`Running ${R.test}`, 'vc run ' + R.test + ' · its beforeAll runs first');
  A['rn-quarantine'] = () => SH.quarantine(R.test);
  /* Quarantine = allowFailure with a reason and an until date, written as a diff (ui.md §5) */
  SH.quarantine = (id, after) => {
    const [file, test] = id.split('#'), [svc, area] = file.split('.');
    const until = '2026-12-07';
    VC.diffDialog({
      title: `Quarantine ${test}`, sub: 'It still runs and is reported, but a failure no longer fails the run, until the date below.',
      fields: `<div class="form-grid"><div class="field"><label for="q-reason">Reason</label><input class="input" id="q-reason" value="flaky on staging, see #352"><span class="hint">Shown in every report next to ⚠.</span></div>
        <div class="field"><label for="q-until">Until</label><input class="input mono" id="q-until" value="${until}"><span class="hint" title="V4008">After this day it fails the run again.</span></div></div>`,
      file: `services/${svc}/${area}.test.yaml`,
      lines: [['@', '@@ -36,4 +36,7 @@'], [' ', `  - id: ${test}`], [' ', '    call: payments#charge'], ['+', '    allowFailure:'], ['+', '      reason: "flaky on staging, see #352"'], ['+', `      until: ${until}`], [' ', '    expect:']],
      confirm: 'Write and quarantine', callout: 'If the file changed on disk since you opened it, you get Reload or Keep mine.',
      after: () => { if (after) after(); VC.render(); }
    });
  };

  /* ---- Runs space sidebar: Overview · All runs · Schedules, then the recent runs -------------- */
  const isLive = () => VC.run && (VC.run.phase === 'running' || VC.run.phase === 'stopping');
  function recentRow(n, dot, sel, meta, cur, label) {
    return `<button class="sb-row rn-sbr" data-action="rn-recent" data-n="${n}" data-fk="sbr-${n}"${cur ? ' aria-current="page"' : ''} aria-label="${esc(label)}"><span class="dot ${dot}" aria-hidden="true"></span><span class="title"><span class="mono rn-sbn">#${n}</span>${esc(sel)}</span><span class="meta">${esc(meta)}</span></button>`;
  }
  function recentHtml() {
    const rt = St.route, L = VC.run, lv = VC.rl;
    const curOf = (r) => (r.kind === 'load' ? rt === 'runs-load' && lv && lv.n === r.n : L && r.n === L.n ? rt === 'runs-live' : rt === 'runs' && R.view === 'run' && R.run === r.n);
    const live = isLive() ? recentRow(L.n, 'is-live', L.opts.selector, 'live', rt === 'runs-live', `Run ${L.n}, running, ${L.opts.selector}`) : '';
    const rows = X.runs.slice(0, isLive() ? 7 : 8).map((r) => recentRow(r.n, `st-${r.st === 'pass' ? 'pass' : r.st === 'blocked' ? 'blocked' : r.st === 'skipped' ? 'skipped' : 'fail'}`, r.sel, r.ago.replace(' ago', ''), curOf(r), `Run ${r.n}, ${r.kind === 'load' ? 'load, ' : ''}${STAT[r.st] || r.st}, ${r.sel}, ${r.ago}`));
    return `<section class="sb-section" aria-labelledby="rn-sb-recent"><h2 class="sb-label" id="rn-sb-recent">Recent</h2>${live}${rows.join('')}</section>`;
  }
  function runsSidebar() {
    const rt = St.route, on = (X.schedules || []).filter((x) => x.on).length;
    const nav = VC.sbNav([
      { label: 'Overview', icon: 'chart', current: rt === 'insights', action: 'rn-nav', attrs: 'data-to="insights"', fk: 'sbn-insights' },
      { label: 'All runs', icon: 'runs', current: rt === 'runs' && R.view !== 'run', action: 'rn-nav', attrs: 'data-to="runs"', fk: 'sbn-runs' },
      { label: 'Schedules', icon: 'clock', current: rt === 'schedules', action: 'rn-nav', attrs: 'data-to="schedules"', meta: String(on), fk: 'sbn-schedules' }
    ]);
    return nav + recentHtml();
  }
  if (VC.spaces && VC.spaces.runs) VC.spaces.runs.sidebar = runsSidebar;
  A['rn-nav'] = (el) => { const to = el.dataset.to; if (to === 'runs') R.view = 'list'; VC.go(to); };
  A['rn-recent'] = (el) => {
    const n = +el.dataset.n, r = runOf(n);
    if (VC.run && n === VC.run.n) return VC.go('runs-live');
    if (r && r.kind === 'load') return VC.openLoadRun ? VC.openLoadRun(n) : VC.go('runs-load');
    if (r && r.kind === 'adhoc') return A['rn-open'](el);
    VC.openRunReport(n);
  };

  document.addEventListener('input', (e) => {
    if (e.target.id !== 'rn-q') return;
    R.q = e.target.value;
    const b = $('#rn-list');
    if (b) { b.innerHTML = listBody(); SH.sizes(b); }
  });
  document.addEventListener('change', (e) => {
    const id = e.target.id;
    if (id === 'rn-base' || id === 'rn-head') { R[id.slice(3)] = +e.target.value; rerender(); }
    const k = e.target.dataset && e.target.dataset.rnf;
    if (k) { R[k] = e.target.value; rerender(); openFilters(k); }
    if (e.target.classList.contains('rn-pick')) {
      const n = +e.target.dataset.n;
      R.pick = e.target.checked ? R.pick.filter((x) => x !== n).concat(n).slice(-2) : R.pick.filter((x) => x !== n);
      rerender();
      const cb = $(`.rn-pick[data-n="${n}"]`); if (cb) cb.focus();
    }
  });
})();
