/* Runs space · Schedules (#schedules): vibranium.config.yaml › schedules:. The vc server
   runs each one while `vc ui` or `vc serve` is up and records it in run history; Export to
   CI writes a GitHub Actions cron workflow for when this machine sleeps. Every save is a
   YAML diff (VC.diffDialog). Builder C. */
(function () {
  'use strict';
  const VC = window.VC, D = VC.data, A = VC.actions, SH = VC.sh, TD = VC.testsData;
  const { $, esc, ic } = VC.h;
  const L = D.shared.schedules;
  const C = (VC.sch = { form: null, gone: null });
  const NOW = new Date(2026, 10, 26, 14, 12);
  const DAYS = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];
  const MON = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
  const PRESETS = [['hourly', 'Every hour', '0 * * * *'], ['weekdays', 'Weekdays 02:00', '0 2 * * 1-5'], ['daily', 'Every day 09:00', '0 9 * * *'], ['weekly', 'Mondays 09:00', '0 9 * * 1'], ['custom', 'Custom', null]];
  const NOTIFY = ['none', 'slack #qa-alerts', 'pagerduty on failure', 'email on failure'];
  const isLoad = (sel) => sel === TD.load.id;
  const byId = (id) => L.find((s) => s.id === id);

  /* ---- cron: parse, words, next times ------------------------------------------ */
  const RANGE = [[0, 59], [0, 23], [1, 31], [1, 12], [0, 7]];
  function field(f, [lo, hi]) {
    const out = new Set();
    for (const part of f.split(',')) {
      const m = part.match(/^(\*|\d+(?:-\d+)?)(?:\/(\d+))?$/);
      if (!m) return null;
      const [a, b] = m[1] === '*' ? [lo, hi] : m[1].split('-').map(Number).concat(m[1].includes('-') ? [] : [+m[1]]);
      const step = m[2] ? +m[2] : 1;
      if (a < lo || b > hi || a > b || step < 1) return null;
      for (let v = a; v <= b; v += step) out.add(v);
    }
    return out;
  }
  function parse(cron) {
    const f = String(cron || '').trim().split(/\s+/);
    if (f.length !== 5) return null;
    const sets = f.map((x, i) => field(x, RANGE[i]));
    if (sets.some((x) => !x)) return null;
    if (sets[4].has(7)) sets[4].add(0);
    return { sets, raw: f };
  }
  const hm = (h, m) => `${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}`;
  function timeWords(m, h) {
    const num = (x) => /^\d+$/.test(x), step = (x) => /^\*\/\d+$/.test(x), at = num(m) && +m ? ` at :${m.padStart(2, '0')}` : '';
    if (step(m) && h === '*') return `every ${m.slice(2)} minutes`;
    if (num(m) && h === '*') return `every hour${at}`;
    if (num(m) && step(h)) return `every ${h.slice(2)} hours${at}`;
    return num(m) && num(h) ? hm(+h, +m) : null;
  }
  /* "Weekdays 02:00", "Every 6 hours", "Weekdays, every 4 hours at :15"; anything else stays cron */
  function words(cron) {
    const p = parse(cron);
    if (!p) return 'Not a valid cron';
    const [m, h, dom, mon, dow] = p.raw, t = timeWords(m, h);
    const day = dom !== '*' || mon !== '*' ? null : dow === '*' ? 'Every day' : dow === '1-5' ? 'Weekdays' : /^\d$/.test(dow) ? `${DAYS[+dow % 7]}s` : null;
    if (!day || !t) return `cron ${cron}`;
    if (/^\d/.test(t)) return `${day} ${t}`;
    return day === 'Every day' ? t[0].toUpperCase() + t.slice(1) : `${day}, ${t}`;
  }
  const cronNote = (c, tail) => { const w = words(c); return w.startsWith('cron ') ? (tail ? `   # ${tail}` : '') : `   # ${w}${tail ? ', ' + tail : ''}`; };
  const match = ({ sets }, d) => sets[0].has(d.getMinutes()) && sets[1].has(d.getHours()) && sets[2].has(d.getDate()) && sets[3].has(d.getMonth() + 1) && sets[4].has(d.getDay());
  function nextRuns(cron, n) {
    const p = parse(cron), out = [];
    if (!p) return out;
    const d = new Date(NOW.getTime());
    for (let i = 0; i < 60 * 24 * 400 && out.length < n; i++) {
      d.setMinutes(d.getMinutes() + 1);
      if (match(p, d)) out.push(new Date(d.getTime()));
    }
    return out;
  }
  function when(d) {
    const day = Math.round((new Date(d).setHours(0, 0, 0, 0) - new Date(NOW).setHours(0, 0, 0, 0)) / 864e5);
    const lab = day === 0 ? 'Today' : day === 1 ? 'Tomorrow' : `${DAYS[d.getDay()].slice(0, 3)} ${d.getDate()} ${MON[d.getMonth()]}`;
    return `${lab} ${hm(d.getHours(), d.getMinutes())}`;
  }

  /* ---- YAML: the config block and the exported workflow ------------------------------ */
  const yamlOf = (s) => [`  ${s.id}:`, `    selector: ${s.sel}`, `    profile: ${s.profile}`, `    cron: "${s.cron}"${cronNote(s.cron)}`]
    .concat(s.notify && s.notify !== 'none' ? [`    notify: ${s.notify}`] : [], s.runner === 'ci' ? ['    runner: githubActions'] : [], s.on ? [] : ['    enabled: false']);
  const cmd = (s) => `vc ${isLoad(s.sel) ? 'load' : 'run'}${s.sel === 'all' ? '' : ' ' + s.sel} --profile ${s.profile} --report junit,html`;
  const workflow = (s) => [
    `# Written by vc ui from vibranium.config.yaml › schedules › ${s.id}`, `name: vc schedule ${s.id}`, 'on:', '  schedule:', `    - cron: "${s.cron}"${cronNote(s.cron, 'UTC')}`, '  workflow_dispatch: {}',
    'permissions:', '  contents: read', 'concurrency:', `  group: vc-schedule-${s.id}`, 'jobs:', '  run:', '    runs-on: ubuntu-latest', '    timeout-minutes: 30', '    steps:',
    '      - uses: actions/checkout@v4', '      - uses: vibranium/setup-action@v2', '      - run: vc check', `      - run: ${cmd(s)}`,
    '      - if: always()', '        uses: actions/upload-artifact@v4', `        with: { name: vc-report-${s.id}, path: .vibranium/reports }`];
  const wfPath = (s) => `.github/workflows/vc-schedule-${s.id}.yml`;
  function diffFor(next, was) {
    const ctx = [[' ', 'schedules:']].concat(L.filter((s) => s !== was).slice(-1).flatMap((s) => yamlOf(s).map((t) => [' ', t])));
    const del = was ? yamlOf(was).map((t) => ['-', t]) : [], add = next ? yamlOf(next).map((t) => ['+', t]) : [];
    return [['@', `@@ -61,${ctx.length + del.length} +61,${ctx.length + add.length} @@`]].concat(ctx, del, add);
  }

  /* ---- list ------------------------------------------------------------------------ */
  const runner = (s) => (s.runner === 'ci' ? `<span class="sc-runner">${ic('branch', 'ico-sm')}GitHub Actions</span>` : `<span class="sc-runner">${ic('terminal', 'ico-sm')}vc server</span>`);
  function last(s) {
    if (!s.last) return '<span class="subtle">Not run yet</span>';
    return `<span class="sc-last"><span class="dot st-${s.last.st}" aria-hidden="true"></span><a href="#runs" data-action="sc-open-run" data-n="${s.last.n}" aria-label="Run ${s.last.n}, ${SH.WORD[s.last.st] || s.last.st}, ${esc(s.last.ago)}">#${s.last.n}</a><span class="subtle">${esc(s.last.ago)}</span></span>`;
  }
  const row = (s) => `<tr class="${s.on ? '' : 'is-off'}"><th scope="row" class="mono">${esc(s.id)}</th>
    <td><span class="sc-when"><span>${esc(words(s.cron))}</span><span class="mono subtle">${esc(s.cron)}</span></span></td>
    <td class="mono ellipsis sc-opt">${esc(s.sel)}</td><td class="mono sc-opt2">${esc(s.profile)}</td><td class="sc-opt">${runner(s)}</td>
    <td class="num">${s.on ? esc(s.runner === 'ci' ? `${s.next} (CI)` : s.next) : '<span class="subtle">Off</span>'}</td><td>${last(s)}</td>
    <td><label class="switch"><input type="checkbox" class="sc-on" data-id="${esc(s.id)}" ${s.on ? 'checked' : ''} aria-label="Run ${esc(s.id)} on schedule" data-fk="sc-on-${esc(s.id)}"><span class="track"></span></label></td>
    <td class="r"><button class="btn btn-ghost btn-icon btn-sm" data-action="sc-menu" data-id="${esc(s.id)}" aria-haspopup="menu" aria-expanded="false" aria-label="More for ${esc(s.id)}" data-fk="sc-m-${esc(s.id)}">${ic('dots')}</button></td></tr>`;
  /* the runs the schedules made, newest first: the same rows All runs shows */
  function historyHtml() {
    const rs = D.shared.runs.filter((r) => r.sched);
    if (!rs.length) return '';
    const res = (r) => (r.kind === 'load' ? (r.load.breach ? `breached ${r.load.breach}` : 'thresholds held') : SH.totLabel(r.tot));
    const rows = rs.map((r) => `<tr><td><span class="sc-last"><span class="dot st-${r.st === 'pass' ? 'pass' : 'fail'}" aria-hidden="true"></span><a href="#runs" data-action="sc-open-run" data-n="${r.n}">#${r.n}</a></span></td><td class="mono">${esc(r.sched)}</td><td class="wrap">${esc(res(r))}</td><td class="mono sc-opt">${esc(r.profile)}</td><td class="num sc-opt">${esc(r.dur)}</td><td class="subtle">${esc(r.when)}</td></tr>`);
    return `<section class="glass" aria-labelledby="sc-h-t"><div class="card-head"><div class="grow"><h2 class="card-title" id="sc-h-t">Scheduled runs</h2><span class="card-sub">Each one is in run history, like any other run</span></div>
      <button class="btn btn-ghost btn-sm" data-action="sc-all-runs" data-fk="sc-all-runs">All runs${ic('arrow-right', 'ico-sm')}</button></div>
      <div class="card-body">${SH.table(['Run', 'Schedule', 'Result', ['Profile', 'sc-opt'], ['Took', 'sc-opt'], 'When'], rows, 'sc-table')}</div></section>`;
  }
  function listHtml() {
    const on = L.filter((s) => s.on).length;
    const body = L.length ? SH.table(['Name', 'When', ['Selector', 'sc-opt'], ['Profile', 'sc-opt2'], ['Runner', 'sc-opt'], 'Next run', 'Last', ['<span class="sr-only">On</span>', ''], ['<span class="sr-only">Actions</span>', 'r']], L.map(row), 'sc-table')
      : SH.empty('clock', 'No schedules yet', 'New schedule adds one to vibranium.config.yaml.');
    return `<section class="glass sc-card" aria-labelledby="sc-title"><div class="card-head"><div class="grow"><h2 class="card-title" id="sc-title">Schedules</h2>
        <span class="card-sub">${on} of ${L.length} on · in vibranium.config.yaml › schedules</span></div></div>
      <div class="card-body sc-body">${body}<p class="sc-note subtle">${ic('info', 'ico-sm')}Schedules run while <code>vc ui</code> or <code>vc serve</code> is up. Export to CI for runs when this machine sleeps.</p></div></section>`;
  }

  /* ---- new / edit dialog -------------------------------------------------------------- */
  const TAGS = [...new Set(Object.values(TD.files).flatMap((f) => f.tags))].sort();
  const FILES = Object.keys(TD.files).concat([TD.load.id]);
  function blank(s) {
    if (!s) return { id: '', mode: 'tags', tags: ['smoke'], files: [], text: '', profile: 'staging', preset: 'weekdays', cron: '0 2 * * 1-5', notify: 'slack #qa-alerts', was: null };
    const tags = s.sel.split(' ').filter((x) => x.startsWith('tag:')).map((x) => x.slice(4)), files = s.sel.split(' ').filter((x) => FILES.includes(x));
    const mode = tags.length && tags.length === s.sel.split(' ').length ? 'tags' : files.length === s.sel.split(' ').length ? 'files' : 'text';
    const p = PRESETS.find((x) => x[2] === s.cron);
    return { id: s.id, mode, tags, files, text: s.sel, profile: s.profile, preset: p ? p[0] : 'custom', cron: s.cron, notify: s.notify || 'none', was: s };
  }
  const selOf = (F) => (F.mode === 'tags' ? F.tags.map((t) => `tag:${t}`).join(' ') : F.mode === 'files' ? F.files.join(' ') : F.text.trim());
  function errs(F) {
    const e = {};
    if (!/^[a-z][a-z0-9-]*$/.test(F.id)) e.id = 'Lowercase words joined by hyphens, such as nightly-staging.';
    else if (L.some((s) => s.id === F.id && s !== F.was)) e.id = `A schedule named ${F.id} already exists.`;
    if (!selOf(F)) e.sel = 'Pick at least one tag or file, or type a selector.';
    if (!parse(F.cron)) e.cron = 'Five fields: minute hour day-of-month month weekday, such as 0 2 * * 1-5.';
    return e;
  }
  const chips = (F, key, opts) => `<div class="sc-chips" role="group" aria-label="${key === 'tags' ? 'Tags' : 'Files'}">${opts.map((o) => `<button class="chip sc-chip" aria-pressed="${F[key].includes(o)}" data-action="sc-chip" data-k="${key}" data-v="${esc(o)}" data-fk="scc-${esc(o)}">${esc(key === 'tags' ? '#' + o : o)}</button>`).join('')}</div>`;
  const hintInner = (F) => { const s = selOf(F); return `Runs <code>${esc(s ? `vc ${isLoad(s) ? 'load' : 'run'} ${s}` : 'nothing yet')}</code>`; };
  function pickHtml(F) {
    const body = F.mode === 'tags' ? chips(F, 'tags', TAGS) : F.mode === 'files' ? chips(F, 'files', FILES)
      : `<input class="input mono" id="sc-text" value="${esc(F.text)}" placeholder="orders users tag:smoke" aria-labelledby="sc-sel-l" aria-describedby="sc-sel-h sc-sel-e" autocomplete="off" spellcheck="false">`;
    return `${body}<span class="hint" id="sc-sel-h">${hintInner(F)}</span>`;
  }
  const nextHtml = (F) => { const ns = nextRuns(F.cron, 3); return ns.length ? `<span class="hint">${esc(words(F.cron))} · next ${ns.map(when).join(', ')}</span>` : ''; };
  const errHtml = (id) => `<span class="sc-err" id="${id}-e" role="alert"></span>`;
  function formHtml(F) {
    const sel = (id, opts, cur) => `<select class="select" id="${id}">${opts.map(([v, l]) => `<option value="${esc(v)}"${v === cur ? ' selected' : ''}>${esc(l)}</option>`).join('')}</select>`;
    return `<div class="dlg"><div class="dlg-head"><div class="grow"><h2 id="dlg-title">${F.was ? `Edit ${esc(F.was.id)}` : 'New schedule'}</h2><div class="sub">Saved to vibranium.config.yaml › schedules, so it is in git.</div></div><button class="btn btn-ghost btn-icon" data-action="close-dialog" aria-label="Close">${ic('x')}</button></div>
      <div class="dlg-body sc-form">
        <div class="field"><label for="sc-id">Name</label><input class="input mono" id="sc-id" value="${esc(F.id)}" placeholder="nightly-staging" autocomplete="off" spellcheck="false" aria-describedby="sc-id-e" autofocus>${errHtml('sc-id')}</div>
        <div class="field"><label for="sc-profile">Profile</label>${sel('sc-profile', D.profiles.map((p) => [p.id, p.id]), F.profile)}</div>
        <div class="field sc-wide"><span class="label" id="sc-sel-l">What to run</span>${SH.seg('Pick by', [['tags', 'Tags'], ['files', 'Files'], ['text', 'Selector']], F.mode, 'sc-mode')}<div class="sc-pick" id="sc-pick">${pickHtml(F)}</div>${errHtml('sc-sel')}</div>
        <div class="field"><label for="sc-preset">When</label>${sel('sc-preset', PRESETS.map(([k, l]) => [k, l]), F.preset)}</div>
        <div class="field"><label for="sc-cron">Cron</label><input class="input mono" id="sc-cron" value="${esc(F.cron)}" autocomplete="off" spellcheck="false" aria-describedby="sc-next sc-cron-e"${F.preset === 'custom' ? '' : ' readonly'}>${errHtml('sc-cron')}</div>
        <div class="sc-wide" id="sc-next" aria-live="polite">${nextHtml(F)}</div>
        <div class="field"><label for="sc-notify">Notify</label>${sel('sc-notify', NOTIFY.map((n) => [n, n === 'none' ? 'Nobody' : n]), F.notify)}</div>
        ${F.profile === 'prod' ? `<p class="callout sc-wide" data-tone="warn">${ic('warn')}<span><b>Runs against live prod</b> with no one at the keyboard. Writes still follow the profile's approval rules.</span></p>` : ''}</div>
      <div class="dlg-foot"><button class="btn" data-action="close-dialog">Cancel</button><button class="btn btn-primary" data-action="sc-save">${ic('check')}Review change</button></div></div>`;
  }
  function showErrs(F, only) {
    const e = errs(F);
    [['sc-id', 'id'], ['sc-sel', 'sel'], ['sc-cron', 'cron']].filter(([, k]) => !only || only === k).forEach(([id, k]) => {
      const box = $(`#${id}-e`), inp = $(`#${id === 'sc-sel' ? 'sc-text' : id}`);
      if (box) box.innerHTML = e[k] ? ic('fail', 'ico-sm') + esc(e[k]) : '';
      if (inp) inp.setAttribute('aria-invalid', e[k] ? 'true' : 'false');
    });
    return e;
  }
  function bind(F) {
    const on = (id, ev, fn) => { const el = $(`#${id}`); if (el) el.addEventListener(ev, fn); };
    on('sc-id', 'input', (e) => { F.id = e.target.value.trim(); if (F.tried) showErrs(F, 'id'); });
    on('sc-profile', 'change', (e) => { F.profile = e.target.value; paint(F, '#sc-profile'); });
    on('sc-notify', 'change', (e) => { F.notify = e.target.value; });
    on('sc-preset', 'change', (e) => { F.preset = e.target.value; const p = PRESETS.find((x) => x[0] === F.preset); if (p[2]) F.cron = p[2]; paint(F, p[2] ? '#sc-preset' : '#sc-cron'); });
    on('sc-cron', 'input', (e) => { F.cron = e.target.value; $('#sc-next').innerHTML = nextHtml(F); if (F.tried) showErrs(F, 'cron'); });
    on('sc-text', 'input', (e) => { F.text = e.target.value; $('#sc-sel-h').innerHTML = hintInner(F); if (F.tried) showErrs(F, 'sel'); });
  }
  /* repaint the open dialog in place, keeping its scroll and the focused control */
  function paint(F, focus) {
    const b = $('#dlg .dlg-body'), top = b ? b.scrollTop : 0;
    $('#dlg').innerHTML = formHtml(F);
    bind(F);
    if (F.tried) showErrs(F);
    const nb = $('#dlg .dlg-body'); if (nb) nb.scrollTop = top;
    const f = $(focus, $('#dlg')); if (f) f.focus();
  }
  function openForm(s) { C.form = blank(s); VC.dialog(formHtml(C.form)); bind(C.form); }
  A['sc-new'] = () => openForm(null);
  A['sc-edit'] = (el) => openForm(byId(el.dataset.id));
  A['sc-mode'] = (el) => { C.form.mode = el.dataset.v; paint(C.form, `[data-fk="sc-mode-${el.dataset.v}"]`); };
  A['sc-chip'] = (el) => {
    const F = C.form, k = el.dataset.k, v = el.dataset.v;
    F[k] = F[k].includes(v) ? F[k].filter((x) => x !== v) : F[k].concat(v);
    paint(F, `[data-fk="scc-${v}"]`);
  };
  A['sc-save'] = () => {
    const F = C.form, was = F.was;
    F.tried = true;
    const e = showErrs(F), bad = ['id', 'sel', 'cron'].find((k) => e[k]);
    if (bad) { const f = $(bad === 'id' ? '#sc-id' : bad === 'cron' ? '#sc-cron' : '#sc-pick button, #sc-text'); if (f) f.focus(); return; }
    const next = { id: F.id, cron: F.cron.trim(), sel: selOf(F), profile: F.profile, runner: was ? was.runner : 'server', next: when(nextRuns(F.cron, 1)[0]), last: was ? was.last : null, on: was ? was.on : true, notify: F.notify };
    next.words = words(next.cron);
    VC.diffDialog({
      title: was ? `Save ${next.id}` : `Add schedule ${next.id}`, sub: 'The vc server picks the change up at once.', file: 'vibranium.config.yaml', lines: diffFor(next, was), confirm: 'Write',
      callout: next.runner === 'ci' ? 'This schedule is exported to CI: export it again so the workflow matches.' : 'The file is checked for changes on disk before writing.',
      after: () => { if (was) Object.assign(was, next); else L.push(next); C.form = null; VC.render(); focusFk(`sc-m-${next.id}`); }
    });
  };

  /* ---- row actions -------------------------------------------------------------------- */
  const focusFk = (k) => { const b = $(`[data-fk="${k}"]`); if (b) b.focus(); };
  A['sc-menu'] = (el) => {
    const s = byId(el.dataset.id), at = `data-id="${esc(s.id)}"`, mi = VC.menuItem;
    VC.menu(el, [mi('Run now', 'sc-run', at, { ico: 'play' }), mi('Edit…', 'sc-edit', at, { ico: 'pencil' }), mi(s.runner === 'ci' ? 'Export to CI again…' : 'Export to CI…', 'sc-export', at, { ico: 'external' }),
      '<div class="pop-sep"></div>', mi(s.on ? 'Disable' : 'Enable', 'sc-toggle', at, { ico: s.on ? 'pause' : 'play' }), mi('Delete', 'sc-del', at, { ico: 'x' })].join(''));
  };
  A['sc-run'] = (el) => {
    const s = byId(el.dataset.id);
    if (isLoad(s.sel)) A['rl-go'](); else VC.startRun(s.sel === 'all' ? undefined : { selector: s.sel });
    VC.toast(`Started ${s.id} now`, `Under ${s.profile}. Its next scheduled run stays ${s.next}.`);
  };
  function setOn(s, v) {
    s.on = v;
    if (v) s.next = when(nextRuns(s.cron, 1)[0]);
    VC.render(); focusFk(`sc-on-${s.id}`);
    VC.toast(`${s.id} ${v ? 'on' : 'off'}`, v ? `Next run ${s.next}` : 'vibranium.config.yaml › enabled: false; it stays in the list');
  }
  A['sc-toggle'] = (el) => { const s = byId(el.dataset.id); setOn(s, !s.on); };
  A['sc-del'] = (el) => {
    const s = byId(el.dataset.id), i = L.indexOf(s);
    L.splice(i, 1); VC.render(); focusFk('top-new');
    VC.toast(`Deleted ${s.id}`, 'Removed from vibranium.config.yaml', null, { label: 'Undo', ico: 'refresh', action: () => { L.splice(i, 0, s); VC.render(); focusFk(`sc-m-${s.id}`); } });
  };
  A['sc-all-runs'] = () => { if (VC.runsState) VC.runsState.view = 'list'; VC.go('runs'); };
  A['sc-open-run'] = (el) => { const n = +el.dataset.n, r = D.shared.runs.find((x) => x.n === n); if (r && r.kind === 'load' && VC.openLoadRun) VC.openLoadRun(n); else VC.openRunReport(n); };

  /* ---- Export to CI ---------------------------------------------------------------------- */
  A['sc-export'] = (el) => {
    const s = byId(el.dataset.id), at = `data-id="${esc(s.id)}"`;
    VC.dialog(`<div class="dlg"><div class="dlg-head"><div class="grow"><h2 id="dlg-title">Export ${esc(s.id)} to CI</h2><div class="sub">A GitHub Actions cron workflow, for runs when this machine sleeps.</div></div><button class="btn btn-ghost btn-icon" data-action="close-dialog" aria-label="Close">${ic('x')}</button></div>
      <div class="dlg-body"><div class="row"><span class="mono">${esc(wfPath(s))}</span>${s.runner === 'ci' ? '' : '<span class="chip">new file</span>'}</div>
        <pre class="code glass-dense sc-yaml" tabindex="0" aria-label="Workflow YAML">${esc(workflow(s).join('\n'))}</pre>
        <p class="callout" data-tone="info">${ic('info')}<span>GitHub reads cron as UTC and can start a few minutes late. Add the ${esc(s.profile)} secrets listed in <code>.env.schema</code> as repository secrets. Writing also sets <code>runner: githubActions</code>, so the vc server stops running it.</span></p></div>
      <div class="dlg-foot"><button class="btn" data-action="close-dialog">Cancel</button><button class="btn" data-action="sc-copy" ${at}>${ic('copy')}Copy</button><button class="btn btn-primary" data-action="sc-write" ${at} autofocus>${ic('file')}Write file</button></div></div>`);
  };
  A['sc-copy'] = (el) => {
    const s = byId(el.dataset.id);
    try { navigator.clipboard.writeText(workflow(s).join('\n')).catch(() => {}); } catch (e) { /* clipboard may be blocked */ }
    VC.toast('Copied the workflow', `Save it as ${wfPath(s)}`);
  };
  A['sc-write'] = (el) => {
    const s = byId(el.dataset.id), wf = workflow(s);
    VC.diffDialog({
      title: `Export ${s.id} to CI`, sub: 'One new workflow, and the schedule marked as run by GitHub Actions.', confirm: 'Write 2 files',
      files: [{ path: wfPath(s), isNew: true, lines: [['@', `@@ -0,0 +1,${wf.length} @@`]].concat(wf.map((t) => ['+', t])) }, { path: 'vibranium.config.yaml', lines: diffFor(Object.assign({}, s, { runner: 'ci' }), s) }],
      after: () => { s.runner = 'ci'; VC.render(); focusFk(`sc-m-${s.id}`); }
    });
  };

  /* ---- screen ---------------------------------------------------------------------- */
  VC.screens.schedules = {
    space: 'runs',
    crumbs: () => [D.project.name, 'Runs', 'Schedules'],
    status: () => '',
    primary: () => `<button class="btn btn-primary btn-sm" data-action="sc-new" data-fk="top-new">${ic('plus', 'ico-sm')}New schedule</button>`,
    render: () => `<div class="sc-page" data-sk="sc-body">${listHtml()}${historyHtml()}</div>`
  };
  document.addEventListener('change', (e) => {
    const t = e.target;
    if (t.classList && t.classList.contains('sc-on')) setOn(byId(t.dataset.id), t.checked);
  });
})();
