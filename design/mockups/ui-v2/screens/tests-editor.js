/* Tests space · Editor (#tests-editor): Form | YAML over one test file. It opens from
   Edit on a read card and is in no nav; Done (or Escape) goes back to the read view.
   YAML is a mock CodeMirror (gutter, squiggle, hover card, autocomplete, quick fix,
   Explain panel). Save goes through a diff; errors ask "Save anyway"; a digest
   conflict offers Reload or Keep mine. File ops: New, Rename, Format, Delete. */
(function () {
  'use strict';
  const VC = window.VC, D = VC.data, S = VC.state, A = VC.actions, TD = VC.testsData;
  const { $, $$, esc, ic, badge, method, chipSrc, hlYaml } = VC.h;
  const FILE = 'orders.checkout', PATH = TD.files[FILE].path;
  const MATCHERS = ['eq', 'ne', 'gt', 'gte', 'lt', 'lte', 'between', 'in', 'notIn', 'type', 'exists', 'empty', 'length', 'contains', 'startsWith', 'endsWith', 'regex', 'format', 'every', 'some', 'none', 'not', 'allOf', 'anyOf'];

  /* the form model of the open file (mirrors TD.yaml) */
  const FM = {
    placesOrder: { line: 19, tags: ['smoke'], call: 'orders#create', vars: [['cardToken', '${var.savedCard}', 'test']], expect: [['status', 'eq', '201', 'test'], ['headers.content-type', 'contains', 'json', 'api'], ['body.state', 'in', '[placed, paid]', 'test'], ['body.total', 'gt', '0', 'test']], save: [['orderId', '${response.body.id}']] },
    rejectsDeclinedCard: { line: 34, tags: [], call: 'payments#charge', vars: [['cardToken', '${var.declinedCard}', 'test']], expect: [['status', 'eq', '402', 'test'], ['headers.content-type', 'contains', 'json', 'api'], ['body.error.code', 'eq', 'card_declined', 'test'], ['timing.total', 'lt', '800ms', 'test']], save: [] },
    refundsPartially: { line: 47, tags: [], steps: [['orders#create', 'save orderId ← ${response.body.id}'], ['payments#refund', 'vars amount: 1250 · expect status 201'], ['orders#get', 'expect body.refundedTotal eq 1250']], vars: [], expect: [], save: [] },
    appliesCoupon: { line: 64, tags: [], call: 'orders#create', allow: true, vars: [['coupon', '${example.coupon}', 'test']], examples: { cols: ['coupon', 'percent'], rows: [['TEN', '10'], ['HALF', '50'], ['GONE', '0']] }, expect: [['status', 'eq', '201', 'test'], ['body.discountPercent', 'eq', '${example.percent}', 'test']], save: [] }
  };
  const E = (VC.ed = { view: 'form', test: 'appliesCoupon', fixed: false, conflict: false, conflictOnce: true, deleted: false, explain: false, complete: false, ac: 0, sel: 'test',
    changes: { 'appliesCoupon.name': ['Applies a percentage coupon', 'Applies a percentage coupon to the order total'] } });
  /* Edit on a read card: the mock models one file, so another file's test shows a note */
  VC.edSel = (id) => {
    const t = (id || '').split('#')[1];
    E.from = id;
    E.foreign = id && !id.startsWith(FILE + '#') ? id : null;
    if (!E.foreign && FM[t]) { E.test = t; E.sel = 'test'; }
  };
  const errs = () => (E.fixed ? 0 : 1);
  const nChanges = () => Object.keys(E.changes).length + (E.fixedBy ? 1 : 0);
  const val = (key, dflt) => (E.changes[key] ? E.changes[key][1] : dflt);

  /* ---- source with edits applied ------------------------------------------ */
  function sourceLines() {
    const lines = TD.yaml.split('\n');
    if (E.fixedBy) lines[TD.diag.line - 1] = lines[TD.diag.line - 1].replace('${var.savedCard}', E.fixedBy);
    Object.entries(E.changes).forEach(([k, [o, n]]) => {
      const [t, field] = k.split('.');
      const key = field.split('/').pop();
      for (let i = FM[t].line - 1; i < lines.length && (i === FM[t].line - 1 || !/^ {2}- id:/.test(lines[i])); i++) {
        if (lines[i].includes(`${key}: ${o}`)) { lines[i] = lines[i].replace(`${key}: ${o}`, `${key}: ${n}`); break; }
      }
    });
    return lines;
  }

  /* ---- form view ---------------------------------------------------------- */
  const fld = (id, label, key, v, opts) => `<div class="field${opts && opts.wide ? ' ed-wide' : ''}"><label for="${id}">${label}</label><input class="input${opts && opts.mono ? ' mono' : ''}${E.changes[key] ? ' is-dirty' : ''}" id="${id}" data-ed="${key}" data-orig="${esc(v)}" value="${esc(val(key, v))}" data-fk="${id}">${opts && opts.hint ? `<span class="hint">${opts.hint}</span>` : ''}</div>`;
  function expectHtml(id, m) {
    return `<section class="ed-sec" aria-labelledby="ed-exp"><div class="row"><h3 class="section-label" id="ed-exp">Expect</h3><span class="grow"></span><button class="btn btn-sm btn-ghost" data-action="ed-add-check" aria-haspopup="menu" data-fk="ed-addchk">${ic('plus', 'ico-sm')}Add check</button></div>
      <div class="table-wrap glass-dense"><table class="table ed-exp" aria-label="Checks"><thead><tr><th>Path</th><th>Matcher</th><th>Value</th><th class="hide-narrow">From</th><th><span class="sr-only">Remove</span></th></tr></thead><tbody>
      ${m.expect.map(([p, mt, v, src], i) => {
        const ro = src === 'api';
        return `<tr${ro ? ' class="is-inherited"' : ''}><td><input class="input input-sm mono" value="${esc(p)}" aria-label="Path" ${ro ? 'readonly' : ''} data-fk="ede-p${i}"></td>
          <td><select class="select input-sm mono" aria-label="Matcher" ${ro ? 'disabled' : ''} data-fk="ede-m${i}">${MATCHERS.map((x) => `<option${x === mt ? ' selected' : ''}>${x}</option>`).join('')}</select></td>
          <td><input class="input input-sm mono${E.changes[`${id}.expect/${p.split('.').pop()}`] ? ' is-dirty' : ''}" value="${esc(val(`${id}.expect/${p.split('.').pop()}`, v))}" data-ed="${id}.expect/${p.split('.').pop()}" data-orig="${esc(v)}" aria-label="Value for ${esc(p)}" ${ro ? 'readonly' : ''} data-fk="ede-v${i}"></td>
          <td class="hide-narrow">${chipSrc(src === 'test' ? 'file' : src)}</td>
          <td>${ro ? `<span class="subtle" title="Inherited from the api; override it here to replace">${ic('lock', 'ico-sm')}</span>` : `<button class="btn btn-ghost btn-icon btn-sm" data-action="ed-rm" aria-label="Remove ${esc(p)} check" data-fk="ede-x${i}">${ic('x')}</button>`}</td></tr>`;
      }).join('')}</tbody></table></div>
      <p class="hint subtle">Checks inherited from the api merge with these; a test check on the same path replaces the api's. <a href="#tests-editor" data-action="ed-plan">See the merged list</a></p></section>`;
  }
  function examplesHtml(m) {
    if (!m.examples) return `<section class="ed-sec" aria-labelledby="ed-ex"><h3 class="section-label" id="ed-ex">Examples</h3><p class="subtle">No examples. Add a table to run this test once per row; each row is its own instance (<code>:1</code>, <code>:2</code>…).</p><div><button class="btn btn-sm" data-action="ed-add-examples" data-fk="ed-addex">${ic('plus', 'ico-sm')}Add examples</button></div></section>`;
    return `<section class="ed-sec" aria-labelledby="ed-ex"><div class="row"><h3 class="section-label" id="ed-ex">Examples · ${m.examples.rows.length} rows</h3><span class="grow"></span><button class="btn btn-sm btn-ghost" data-action="ed-add-row" data-fk="ed-addrow">${ic('plus', 'ico-sm')}Row</button><button class="btn btn-sm btn-ghost" data-action="ed-add-col" data-fk="ed-addcol">${ic('plus', 'ico-sm')}Column</button></div>
      <div class="table-wrap glass-dense"><table class="table ed-grid" aria-label="Examples"><thead><tr><th class="r">#</th>${m.examples.cols.map((c) => `<th class="mono">${esc(c)}</th>`).join('')}</tr></thead><tbody>
      ${m.examples.rows.map((r, i) => `<tr><td class="r mono muted">:${i + 1}</td>${r.map((v, j) => `<td><input class="input input-sm mono ed-cell" value="${esc(v)}" aria-label="Row ${i + 1} ${esc(m.examples.cols[j])}" data-fk="edx-${i}-${j}"></td>`).join('')}</tr>`).join('')}</tbody></table></div>
      <p class="hint subtle">Use a column as <code>\${example.coupon}</code>. Select one row with <code>${FILE}#${E.test}:2</code>.</p></section>`;
  }
  function testForm() {
    const id = E.test, m = FM[id], t = TD.tests[`${FILE}#${id}`];
    const a = m.call && D.apis[m.call];
    return `<div class="ed-form"><div class="row"${m.allow ? ' title="allowFailure until 2026-12-07"' : ''}>${badge(t.st)}</div>
      ${id === 'placesOrder' && !E.fixed ? `<p class="callout" data-tone="danger">${ic('fail')}<span><b>V2101</b> savedCard is never set (vars.cardToken). <button class="btn btn-sm" data-action="ed-fix" data-fk="ed-fixform">${ic('wand', 'ico-sm')}Use \${var.declinedCard}</button></span></p>` : ''}
      <section class="ed-sec ed-cols" aria-label="Basics">${fld('edf-id', 'id', `${id}.id`, id, { mono: true, hint: 'A third-person verb' })}${fld('edf-name', 'name', `${id}.name`, t.name, { wide: true })}
        <div class="field"><label for="edf-call">call</label>${m.steps ? '<span class="subtle">Uses steps below</span>' : `<button class="btn ed-apibtn" id="edf-call" data-action="ed-api-pick" aria-haspopup="menu" data-fk="edf-call">${a ? method(a.m) : ''}<span class="mono">${esc(m.call)}</span>${ic('chev-down', 'ico-sm')}</button>`}</div>
        <div class="field"><span class="ed-lbl">tags</span><div class="row row-wrap">${m.tags.map((g) => `<span class="chip">${esc(g)}</span>`).join('')}<button class="btn btn-ghost btn-sm" data-action="ed-add-tag" data-fk="edf-tag">${ic('plus', 'ico-sm')}tag</button></div></div></section>
      ${m.steps ? `<section class="ed-sec" aria-labelledby="ed-steps"><h3 class="section-label" id="ed-steps">Steps · run in order</h3><ol class="ed-steps">${m.steps.map(([api, d], i) => `<li><span class="n">${i + 1}</span>${method(D.apis[api] ? D.apis[api].m : 'GET')}<span class="mono">${esc(api)}</span><span class="subtle ellipsis">${esc(d)}</span><button class="btn btn-ghost btn-icon btn-sm" data-action="noop" aria-label="Step ${i + 1} options">${ic('dots')}</button></li>`).join('')}</ol><div><button class="btn btn-sm" data-action="noop">${ic('plus', 'ico-sm')}Add step</button></div></section>` : ''}
      ${m.vars.length ? `<section class="ed-sec" aria-labelledby="ed-vars"><h3 class="section-label" id="ed-vars">Vars</h3><div class="kv glass-dense">${m.vars.map(([k, v]) => `<div class="kv-row"><span class="k">${esc(k)}</span><span class="v"><input class="input mono${E.changes[`${id}.vars/${k}`] ? ' is-dirty' : ''}" value="${esc(val(`${id}.vars/${k}`, id === 'placesOrder' && E.fixedBy ? E.fixedBy : v))}" data-ed="${id}.vars/${k}" data-orig="${esc(v)}" aria-label="${esc(k)}" data-fk="edv-${k}"></span><span class="s">${v.includes('savedCard') && !E.fixed ? '<span class="badge badge-sm st-fail">V2101</span>' : chipSrc(v.includes('example') ? 'set' : 'profile', 'where the value comes from')}</span></div>`).join('')}</div></section>` : ''}
      ${m.steps ? '' : examplesHtml(m)}${m.steps ? '' : expectHtml(id, m)}
      ${m.save.length ? `<section class="ed-sec" aria-labelledby="ed-save"><h3 class="section-label" id="ed-save">Save · for later steps and tests</h3><div class="kv glass-dense">${m.save.map(([k, v]) => `<div class="kv-row"><span class="k">${esc(k)}</span><span class="v"><span>${esc(v)}</span></span><span class="s">${chipSrc('set')}</span></div>`).join('')}</div></section>` : ''}
      <details class="disc ed-disc" data-fk="ed-tl"><summary>${ic('chev-down', 'chev')}Lifecycle for this test</summary><div class="disc-body"><p class="subtle">Inherits the file's beforeAll, beforeEach and afterAll.</p>
        <div class="row"><button class="btn btn-sm" data-action="ed-pick" data-v="lifecycle">${ic('flow', 'ico-sm')}Edit file lifecycle</button><button class="btn btn-sm btn-ghost" data-action="noop">${ic('plus', 'ico-sm')}before / after</button></div></div></details></div>`;
  }
  function lifeForm() {
    const lc = TD.files[FILE].lifecycle;
    const blk = (name, items, hint) => `<section class="ed-sec" aria-labelledby="edl-${name}"><div class="row"><h3 class="section-label" id="edl-${name}">${name}</h3><span class="subtle">${hint}</span><span class="grow"></span><button class="btn btn-sm btn-ghost" data-action="ed-add-action" data-v="${name}" aria-haspopup="menu" data-fk="edl-add-${name}">${ic('plus', 'ico-sm')}Action</button></div>
      <ol class="ed-steps">${items.length ? items.map((x, i) => `<li><span class="n">${i + 1}</span><span class="chip mono">${esc(x.split(':')[0])}</span><span class="mono ellipsis">${esc(x.split(': ').slice(1).join(': '))}</span><button class="btn btn-ghost btn-icon btn-sm" data-action="noop" aria-label="Action options">${ic('dots')}</button></li>`).join('') : '<li class="subtle">None</li>'}</ol></section>`;
    return `<div class="ed-form">${blk('beforeAll', lc.beforeAll, 'once per file, may be cached')}${blk('beforeEach', lc.beforeEach, 'before every test, in order')}${blk('afterAll', lc.afterAll, 'once, after the last test')}
      <section class="ed-sec"><h3 class="section-label">Cleanups</h3><p class="subtle">Registered by the apis these actions call (<code>users#create</code> → <code>users#delete</code>). They run newest first, also after a failure or Stop.</p></section></div>`;
  }
  function formCard() {
    return `<section class="glass ed-card" aria-labelledby="ed-form-t"><div class="card-head"><h2 class="card-title" id="ed-form-t"${E.sel === 'test' ? ` title="${FILE}#${E.test} · ${PATH}:${FM[E.test].line}"` : ` title="${PATH}"`}>${E.sel === 'test' ? `<span class="mono">${E.test}</span>` : 'File lifecycle'}</h2></div>
      <div class="card-body ed-formwrap" data-sk="ed-form">${E.sel === 'test' ? testForm() : lifeForm()}</div></section>`;
  }

  /* ---- YAML view ---------------------------------------------------------- */
  const swapVar = (html) => html.replace(/<span class="var-token(?: is-secret)?" data-var="([^"]+)">/g, (m, n) => `<span class="ed-var" tabindex="0" data-v="${n}">`);
  function lineHtml(l, n) {
    const isDiag = n === TD.diag.line && !E.fixed;
    let h = swapVar(hlYaml(l));
    if (isDiag) h = h.replace('<span class="ed-var" tabindex="0" data-v="var.savedCard">', '<span class="ed-var ed-squig" tabindex="0" data-v="var.savedCard" aria-describedby="ed-diag-msg">');
    if (n === TD.diag.line && E.complete) h = `${esc(l.replace(/\$\{var\.savedCard\}|\$\{var\.[a-zA-Z]+\}/, ''))}<span class="ed-caretwrap"><span class="ed-typed">\${var.</span><span class="ed-caret" aria-hidden="true"></span>${acHtml()}</span>`;
    const gut = isDiag ? `<button class="ed-bulb" data-action="ed-bulb" aria-label="Quick fixes for V2101" title="Quick fix (⌘.)" data-fk="ed-bulb">${ic('wand', 'ico-sm')}</button>` : '<span class="ed-gap"></span>';
    return `<div class="ed-line${isDiag ? ' is-diag' : ''}${n === TD.diag.line && E.complete ? ' is-cur' : ''}"><span class="ed-gut">${gut}${isDiag ? `<span class="ed-mark" title="V2101 savedCard is never set">${ic('fail', 'ico-sm')}</span>` : '<span class="ed-gap"></span>'}<span class="ln">${n}</span></span><span class="ed-txt">${h || ' '}</span></div>`;
  }
  function acHtml() {
    return `<span class="ed-ac l3" role="listbox" id="ed-ac" aria-label="Suggestions" tabindex="-1">${TD.complete.map(([k, v, d], i) => `<span class="ed-ac-i" role="option" id="ed-ac-${i}" aria-selected="${E.ac === i}" data-action="ed-ac-pick" data-i="${i}"><span class="chip ed-kind">${k}</span><span class="mono">${esc(v)}</span><span class="subtle">${esc(d)}</span></span>`).join('')}<span class="ed-ac-foot subtle">↑↓ to move · ↵ to insert · esc</span></span>`;
  }
  function problems() {
    if (E.fixed) return `<div class="ed-problems is-ok">${ic('pass', 'ico-sm')}<span>vc check · no problems in this file</span></div>`;
    return `<div class="ed-problems" role="status"><span class="badge badge-sm st-fail">${ic('fail')}1 error</span><span class="mono">V2101</span><span id="ed-diag-msg" class="ellipsis">${esc(TD.diag.msg)} · line ${TD.diag.line}</span><span class="grow"></span>
      <button class="btn btn-sm" data-action="ed-fix" data-fk="ed-fixall">${ic('wand', 'ico-sm')}Fix all</button><button class="btn btn-sm btn-ghost" data-action="ed-explain" aria-pressed="${E.explain}" data-fk="ed-explain">${ic('info', 'ico-sm')}Explain V2101</button></div>`;
  }
  function explainHtml() {
    if (!E.explain) return '';
    const x = TD.explain;
    return `<aside class="ed-explain glass-dense" aria-labelledby="ed-x-t"><div class="row"><span class="badge badge-sm st-fail">${x.code}</span><h3 class="ed-x-title" id="ed-x-t">${esc(x.title)}</h3><span class="grow"></span><button class="btn btn-ghost btn-icon btn-sm" data-action="ed-explain" aria-label="Close explanation" data-fk="ed-x-close">${ic('x')}</button></div>
      ${x.body.map((p) => `<p>${esc(p)}</p>`).join('')}<h4 class="section-label">Here</h4><p class="mono subtle">${esc(TD.diag.help)}</p><h4 class="section-label">Fix</h4><p>${esc(x.fix)}</p>
      <pre class="code glass-dense">${hlYaml('# vibranium.config.yaml\nprofiles:\n  local:\n    vars:\n      savedCard: tok_visa')}</pre><p class="subtle">Same text as <code>vc explain V2101</code>.</p></aside>`;
  }
  function yamlCard() {
    const lines = sourceLines();
    return `<section class="glass ed-card" aria-labelledby="ed-yaml-t"><div class="card-head"><h2 class="card-title" id="ed-yaml-t" title="${PATH}">YAML</h2><span class="grow"></span>
        <button class="btn btn-sm btn-ghost" data-action="ed-suggest" data-fk="ed-suggest">${ic('sparkle', 'ico-sm')}Suggest<span class="kbd hide-narrow">⌃Space</span></button></div>
      <div class="card-tabs">${problems()}</div>
      <div class="card-body ed-yamlbody${E.explain ? ' has-explain' : ''}"><div class="ed-code glass-dense" role="region" aria-label="Source of ${PATH}" tabindex="0" data-sk="ed-code" data-fk="ed-code">${lines.map((l, i) => lineHtml(l, i + 1)).join('')}</div>${explainHtml()}</div></section>`;
  }

  /* ---- header, banners, screen ------------------------------------------- */
  function headHtml() {
    const n = errs(), c = nChanges();
    return `<div class="ed-head on-l0"><div class="grow row ed-title"><h1 class="card-title mono ellipsis" title="${PATH}">${FILE}</h1>${c ? `<span class="chip ed-dirty" title="Unsaved changes">${c} unsaved</span>` : ''}</div>
      <div class="seg seg-sm" role="group" aria-label="View">${[['form', 'Form'], ['yaml', 'YAML']].map(([k, l]) => `<button aria-pressed="${E.view === k}" data-action="ed-view" data-v="${k}" data-fk="edv-${k}">${l}</button>`).join('')}</div>
      <button class="btn btn-sm" data-action="ed-done" data-fk="ed-done" title="Back to the read view (Esc)">Done</button>
      <button class="btn btn-sm btn-primary" data-action="ed-save" data-fk="ed-save"${n ? ` title="${n} error; Save asks before writing"` : ''}>${ic('check', 'ico-sm')}Save<span class="kbd hide-narrow">⌘S</span></button>
      ${VC.moreBtn('ed-file', 'Plan and file actions', 'ed-file')}</div>`;
  }
  function foreignHtml() {
    if (!E.foreign) return '';
    return `<p class="callout ed-banner" data-tone="info">${ic('info')}<span>The mockup edits <b class="mono">${FILE}</b> only. <b class="mono">${esc(E.foreign)}</b> opens the same way in its own file.</span></p>`;
  }
  function conflictHtml() {
    if (!E.conflict) return '';
    return `<div class="callout ed-banner" data-tone="danger" role="alert">${ic('warn')}<span><b>Not saved: the file changed on disk</b> <span title="digest 9f3c1a → 41ab07">since you opened it</span> (git pull 2 min ago: +2 lines in rejectsDeclinedCard).</span><span class="grow"></span>
      <button class="btn btn-sm" data-action="ed-reload" data-fk="ed-reload">${ic('refresh', 'ico-sm')}Reload</button><button class="btn btn-sm btn-danger" data-action="ed-keep" data-fk="ed-keep">Keep mine…</button></div>`;
  }
  VC.screens['tests-editor'] = {
    space: 'tests',
    enter: () => { if (E.view !== 'yaml') E.view = 'form'; },
    crumbs: () => [D.project.name, 'Tests', E.sel === 'test' ? `${FILE}#${E.test}` : FILE],
    status: () => (E.deleted ? 'file in Trash' : errs() ? `${ic('fail', 'ico-sm')} 1 error` : ''),
    render: () => {
      if (E.deleted) return `<div class="stage-one"><section class="glass stage-one"><div class="empty empty-quiet"><span><b class="mono" title="${PATH}">${FILE}</b> is in the Trash. Tests that referenced it fail vc check until you restore it.</span><button class="btn" data-action="ed-restore">${ic('arrow-left')}Restore</button></div></section></div>`;
      const body = `<div class="stage-one">${E.view === 'yaml' ? yamlCard() : formCard()}</div>`;
      return `${headHtml()}${foreignHtml()}${conflictHtml()}${body}`;
    },
    mount: (view) => { const ac = $('#ed-ac', view); if (ac) ac.focus({ preventScroll: true }); }
  };

  /* ---- actions: view, form, YAML helpers ---------------------------------- */
  A['ed-view'] = (el) => { E.view = el.dataset.v; VC.render(); };
  A['ed-done'] = () => {
    const back = E.foreign || (E.sel === 'test' ? `${FILE}#${E.test}` : null);
    Object.assign(VC.tx, back && TD.tests[back] ? { sel: back, view: 'test', check: null } : { file: FILE, view: 'file', check: null });
    VC.go('tests');
  };
  A['ed-pick'] = (el) => { if (el.dataset.v === 'lifecycle') E.sel = 'lifecycle'; else { E.sel = 'test'; E.test = el.dataset.v; } VC.render(); };
  const applyFix = (to) => { E.fixed = true; E.fixedBy = to; E.complete = false; E.explain = false; VC.hidePopovers(); VC.render(); VC.toast('Fixed V2101', `vars.cardToken now uses ${to} · vc check · 0 errors`); };
  A['ed-fix'] = () => applyFix('${var.declinedCard}');
  A['ed-explain'] = () => { E.explain = !E.explain; if (E.view === 'form') E.view = 'yaml'; VC.render(); };
  A['ed-bulb'] = (el) => VC.menu(el, `<div class="pop-label">Quick fixes · V2101</div>${VC.menuItem('Use <code>${var.declinedCard}</code>', 'ed-fixto', 'data-v="${var.declinedCard}"', { ico: 'wand', meta: 'closest name' })}${VC.menuItem('Use <code>${var.cardToken}</code>', 'ed-fixto', 'data-v="${var.cardToken}"', { ico: 'wand' })}
    ${VC.menuItem('Set savedCard in vibranium.config.yaml…', 'ed-setvar', '', { ico: 'plus' })}<div class="pop-sep"></div>${VC.menuItem('Explain V2101', 'ed-explain', '', { ico: 'info' })}${VC.menuItem('Fix all in file', 'ed-fix', '', { ico: 'check', meta: '1' })}`);
  A['ed-fixto'] = (el) => applyFix(el.dataset.v);
  A['ed-setvar'] = () => VC.diffDialog({ title: 'Set savedCard', sub: 'Adds the var to profile local', file: 'vibranium.config.yaml', lines: [['@', '@@ -29,3 +29,4 @@ profiles:'], [' ', '  local:'], [' ', '    vars:'], ['+', '      savedCard: tok_visa'], [' ', '      declinedCard: tok_chargeDeclined']], after: () => { E.fixed = true; VC.render(); } });
  A['ed-suggest'] = () => { E.complete = true; E.ac = 0; if (E.view === 'form') E.view = 'yaml'; VC.render(); };
  A['ed-ac-pick'] = (el) => { const v = TD.complete[+el.dataset.i][1]; if (v.startsWith('${var.')) return applyFix(v); E.complete = false; VC.render(); VC.toast(`Inserted ${v}`, 'Suggestions come from the compiled program: ids, vars in scope and matchers'); };
  A['ed-api-pick'] = (el) => VC.menu(el, `<div class="pop-label">call</div>${Object.keys(D.apis).map((id) => VC.menuItem(`${method(D.apis[id].m)}<span class="mono">${esc(id)}</span>`, 'noop', '', { radio: true, checked: id === FM[E.test].call })).join('')}`);
  A['ed-add-check'] = (el) => VC.menu(el, `<div class="pop-label">Add a check</div>${['status', 'headers', 'body.match', 'body.schema', 'body.equals', 'size', 'timing', 'cel', 'custom'].map((k) => VC.menuItem(`<code>${k}</code>`, 'ed-check-add', `data-v="${k}"`)).join('')}`);
  A['ed-check-add'] = (el) => { FM[E.test].expect.push([el.dataset.v === 'timing' ? 'timing.total' : el.dataset.v === 'body.match' ? 'body.' : el.dataset.v, el.dataset.v === 'timing' ? 'lt' : 'eq', '', 'test']); VC.render(); };
  A['ed-rm'] = (el) => { const i = +el.dataset.fk.replace('ede-x', ''); FM[E.test].expect.splice(i, 1); VC.render(); };
  A['ed-add-row'] = () => { const ex = FM[E.test].examples; ex.rows.push(ex.cols.map(() => '')); VC.render(); };
  A['ed-add-col'] = () => { const ex = FM[E.test].examples; ex.cols.push('col' + (ex.cols.length + 1)); ex.rows.forEach((r) => r.push('')); VC.render(); };
  A['ed-add-examples'] = () => { FM[E.test].examples = { cols: ['value'], rows: [[''], ['']] }; VC.render(); };
  A['ed-add-tag'] = () => { FM[E.test].tags.push('needs-triage'); VC.render(); };
  A['ed-add-action'] = (el) => VC.menu(el, `<div class="pop-label">Add to ${esc(el.dataset.v)}</div>${[['call', 'an api'], ['test', 'run another test'], ['function', 'a JS function'], ['command', 'a shell command']].map(([k, d]) => VC.menuItem(`<code>${k}</code>`, 'noop', '', { meta: d })).join('')}<div class="pop-sep"></div>${VC.menuItem('forEach / repeat / parallel', 'noop', '', { meta: 'modifiers' })}`);
  A['ed-new-test'] = () => VC.toast('New test', 'Same as vc new test orders.checkout#… --from orders#create');
  A['ed-plan'] = () => {
    const P = TD.plan;
    VC.dialog(`<div class="dlg dlg-wide"><div class="dlg-head"><div class="grow"><h2 id="dlg-title">Plan preview</h2><div class="sub">Same as <code>vc plan ${FILE} --explain</code> · profile ${esc(S.profile)} · nothing is sent</div></div><button class="btn btn-ghost btn-icon" data-action="close-dialog" aria-label="Close">${ic('x')}</button></div>
      <div class="dlg-body"><pre class="code glass-dense wrap">plan  ${esc(P.line)}</pre>
        <section><h3 class="section-label">Order</h3><ol class="tx-life">${P.tree.map(([ph, x]) => `<li class="${ph === 'test' ? 'is-this' : ''}"><span class="ph mono">${ph}</span><span class="mono ellipsis">${esc(x)}</span></li>`).join('')}</ol></section>
        <section><h3 class="section-label">Merged expects · rejectsDeclinedCard</h3><div class="table-wrap glass-dense"><table class="table"><thead><tr><th>Check</th><th>Value</th><th>From</th><th>At</th></tr></thead><tbody>${P.merged.map(([k, v, src, at]) => `<tr${v.startsWith('replaced') ? ' class="is-overridden"' : ''}><td class="mono">${esc(k)}</td><td class="mono">${esc(v)}</td><td>${chipSrc(src === 'test' ? 'file' : src)}</td><td class="mono muted">${esc(at)}</td></tr>`).join('')}</tbody></table></div></section></div>
      <div class="dlg-foot"><span class="note">4 instances · ~1.4 s at concurrency 4</span><button class="btn" data-action="close-dialog">Close</button><button class="btn btn-primary" data-action="ed-run-file">${ic('play')}Run file</button></div></div>`, 'is-wide');
  };
  A['ed-run-file'] = () => VC.startRun && VC.startRun({ selector: FILE, only: Object.keys(FM).map((t) => `${FILE}#${t}`) });

  /* ---- save: errors → confirm, diff, digest conflict ---------------------- */
  function saveDiff() {
    const before = TD.yaml.split('\n'), after = sourceLines(), out = [];
    before.forEach((l, i) => { if (l !== after[i]) out.push(['@', `@@ -${i + 1},1 +${i + 1},1 @@`], ['-', l], ['+', after[i]]); });
    return out;
  }
  A['ed-save'] = () => {
    if (errs() && !E.saveAnyway) {
      return VC.dialog(`<div class="dlg dlg-narrow"><div class="dlg-head"><div class="grow"><h2 id="dlg-title">Save with 1 error?</h2><div class="sub">${PATH}</div></div><button class="btn btn-ghost btn-icon" data-action="close-dialog" aria-label="Close">${ic('x')}</button></div>
        <div class="dlg-body"><p class="callout" data-tone="danger">${ic('fail')}<span><b>V2101</b> ${esc(TD.diag.msg)} · line ${TD.diag.line}. A run of this file stops at vc check until it is fixed.</span></p></div>
        <div class="dlg-foot"><button class="btn" data-action="close-dialog">Cancel</button><button class="btn" data-action="ed-fix-save">${ic('wand')}Fix all and save</button><button class="btn btn-danger" data-action="ed-save-anyway" autofocus>Save anyway</button></div></div>`);
    }
    const lines = saveDiff();
    E.saveAnyway = false;
    if (!lines.length) return VC.toast('Nothing to save', 'The file matches what is on disk');
    VC.dialog(`<div class="dlg"><div class="dlg-head"><div class="grow"><h2 id="dlg-title">Save ${FILE}</h2><div class="sub">${nChanges()} change${nChanges() === 1 ? '' : 's'} · vc fmt runs on write, then vc check</div></div><button class="btn btn-ghost btn-icon" data-action="close-dialog" aria-label="Close">${ic('x')}</button></div>
      <div class="dlg-body"><div class="row"><span class="mono">${PATH}</span></div>${VC.diffHtml(lines, PATH)}</div>
      <div class="dlg-foot"><span class="note">${errs() ? `${ic('fail', 'ico-sm')} saving with 1 error` : 'vc check · 0 errors'}</span><button class="btn" data-action="close-dialog">Cancel</button><button class="btn btn-primary" data-action="ed-write" autofocus>${ic('check')}Write</button></div></div>`);
  };
  A['ed-save-anyway'] = () => { E.saveAnyway = true; A['ed-save'](); };
  A['ed-fix-save'] = () => { E.fixed = true; E.fixedBy = '${var.declinedCard}'; A['ed-save'](); };
  A['ed-write'] = () => {
    VC.closeDialog();
    if (E.conflictOnce) { E.conflictOnce = false; E.conflict = true; return VC.render(); }
    commit('');
  };
  function commit(extra) {
    const prev = { changes: E.changes, fixedBy: E.fixedBy, yaml: TD.yaml };
    TD.yaml = sourceLines().join('\n');
    E.changes = {}; E.fixedBy = null; E.conflict = false;
    VC.render();
    VC.undoToast(`Wrote ${PATH}`, `${extra}vc check · ${errs()} error${errs() === 1 ? '' : 's'}`, () => { TD.yaml = prev.yaml; E.changes = prev.changes; E.fixedBy = prev.fixedBy; });
  }
  A['ed-reload'] = () => { E.changes = {}; E.fixedBy = null; E.fixed = false; E.conflict = false; VC.render(); VC.toast('Reloaded from disk', 'Your edits were dropped; the change from git pull is in'); };
  A['ed-keep'] = () => VC.diffDialog({ title: 'Keep mine', sub: 'Overwrites the change on disk with your version', file: PATH,
    lines: [['@', '@@ -36,4 +36,2 @@ rejectsDeclinedCard (theirs, from git pull)'], ['-', '    retry:'], ['-', '      attempts: 2'], [' ', '    vars:']].concat(saveDiff()), confirm: 'Overwrite', after: () => { const t = $('#toasts').lastElementChild; if (t) t.remove(); commit('overwrote the change on disk · '); } });

  /* ---- file ops ----------------------------------------------------------- */
  A['ed-file'] = (el) => VC.menu(el, `${VC.menuItem('Plan', 'ed-plan', '', { ico: 'flow', meta: 'vc plan' })}${VC.menuItem('Run this file…', 'tx-run-dialog', '', { ico: 'play', meta: '⌘R' })}
    <div class="pop-sep"></div>${E.sel === 'test' ? VC.menuItem('Edit file lifecycle', 'ed-pick', 'data-v="lifecycle"', { ico: 'flow', meta: 'beforeAll · afterAll' }) : VC.menuItem(`Back to ${E.test}`, 'ed-pick', `data-v="${E.test}"`, { ico: 'arrow-left' })}${VC.menuItem('Add test', 'ed-new-test', '', { ico: 'plus', meta: 'vc new test' })}<div class="pop-sep"></div><div class="pop-label">New from a template</div>${TD.templates.map(([k, l, c]) => VC.menuItem(esc(l), 'ed-new', `data-k="${k}"`, { ico: 'plus', meta: `vc new ${k}` })).join('')}
    <div class="pop-sep"></div>${VC.menuItem('Rename…', 'ed-rename', '', { ico: 'redirect', meta: 'vc rename' })}${VC.menuItem('Format', 'ed-format', '', { ico: 'braces', meta: 'vc fmt' })}${VC.menuItem('Delete…', 'ed-delete', '', { ico: 'x' })}
    <div class="pop-sep"></div><div class="pop-label">Mockup</div>${VC.menuItem('Simulate a change on disk', 'ed-sim', '', { ico: 'refresh' })}`);
  A['ed-sim'] = () => { E.conflict = true; VC.render(); };
  A['ed-new'] = (el) => {
    const [k, l, c] = TD.templates.find((x) => x[0] === el.dataset.k);
    const file = { test: 'services/orders/returns.test.yaml', api: 'services/orders/orders.api.yaml', service: 'services/shipping/shipping.api.yaml', load: 'load/checkout/weekend.load.yaml' }[k];
    const lines = { test: ['id: orders.returns', 'name: Returns', 'tests:', '  - id: acceptsReturn', '    name: Accepts a return', '    call: orders#create', '    expect:', '      status: 201'], api: ['  cancel:', '    name: Cancel order', '    method: POST', '    path: /orders/${orderId}/cancel'], service: ['name: shipping', 'baseUrl: https://shipping.shop.example', 'apis: {}'], load: ['id: checkout.weekend', 'mix:', '  - test: orders.checkout#placesOrder', 'stages:', '  - { duration: 1m, users: 50 }'] }[k];
    VC.diffDialog({ title: `New ${l.toLowerCase()}`, sub: c, file, isNew: k !== 'api', lines: [['@', `@@ -0,0 +1,${lines.length} @@`]].concat(lines.map((x) => ['+', x])), confirm: 'Create' });
  };
  A['ed-rename'] = () => VC.diffDialog({ title: 'Rename orders.checkout → orders.purchase', sub: 'Same as vc rename orders.checkout orders.purchase · moves the file and updates 3 references', confirm: 'Rename and write 3 files',
    files: [{ path: 'services/orders/checkout.test.yaml → purchase.test.yaml', lines: [['@', '@@ -1,1 +1,1 @@'], ['-', 'id: orders.checkout'], ['+', 'id: orders.purchase']] },
      { path: 'load/checkout/peak.load.yaml', lines: [['@', '@@ -9,2 +9,2 @@ mix:'], ['-', '  - test: orders.checkout#placesOrder'], ['+', '  - test: orders.purchase#placesOrder'], [' ', '    weight: 1']] },
      { path: 'services/payments/refunds.test.yaml', lines: [['@', '@@ -6,1 +6,1 @@ beforeEach:'], ['-', '  - test: orders.checkout#placesOrder'], ['+', '  - test: orders.purchase#placesOrder']] }] });
  A['ed-format'] = () => VC.diffDialog({ title: 'Format', sub: 'vc fmt: key order, two-space indent, flow maps kept short', file: PATH, lines: [['@', '@@ -29,2 +29,2 @@'], ['-', '          state: { in: [placed,paid] }'], ['+', '          state: { in: [placed, paid] }'], [' ', '          total: { gt: 0 }']], confirm: 'Write' });
  A['ed-delete'] = () => VC.dialog(`<div class="dlg dlg-narrow"><div class="dlg-head"><div class="grow"><h2 id="dlg-title">Delete ${FILE}?</h2><div class="sub mono">${PATH}</div></div><button class="btn btn-ghost btn-icon" data-action="close-dialog" aria-label="Close">${ic('x')}</button></div>
    <div class="dlg-body"><p class="callout" data-tone="warn">${ic('warn')}<span><b>2 places use tests from this file.</b> They fail vc check (V2001) until you restore it or point them elsewhere.</span></p>
      <ul class="tx-life"><li><span class="ph mono">mix</span><span class="mono ellipsis">load/checkout/peak.load.yaml:9 → placesOrder</span></li><li><span class="ph mono">beforeEach</span><span class="mono ellipsis">services/payments/refunds.test.yaml:6 → placesOrder</span></li></ul>
      <p class="subtle">The file moves to the system Trash; git still has it.</p></div>
    <div class="dlg-foot"><button class="btn" data-action="close-dialog">Cancel</button><button class="btn btn-danger" data-action="ed-delete-go" autofocus>Move to Trash</button></div></div>`);
  A['ed-delete-go'] = () => { VC.closeDialog(); E.deleted = true; VC.render(); VC.undoToast('Moved to Trash', PATH, () => { E.deleted = false; }); };
  A['ed-restore'] = () => { E.deleted = false; VC.render(); VC.toast('Restored', PATH); };

  /* ---- input, hover card, keys -------------------------------------------- */
  document.addEventListener('input', (e) => {
    const el = e.target;
    if (!el.dataset || !el.dataset.ed) return;
    if (el.value === el.dataset.orig) delete E.changes[el.dataset.ed]; else E.changes[el.dataset.ed] = [el.dataset.orig, el.value];
    el.classList.toggle('is-dirty', !!E.changes[el.dataset.ed]);
    const d = $('.ed-head .ed-dirty'), c = nChanges();
    if (d) d.textContent = `${c} unsaved`; else if (c) { const r = $('.ed-head .row'); if (r) r.insertAdjacentHTML('beforeend', `<span class="chip ed-dirty">${c} unsaved</span>`); }
  });
  const HOVER = {
    'var.declinedCard': () => [TD.hover.type, TD.hover.value, 'profile', TD.hover.at, TD.hover.doc],
    'var.savedCard': () => ['unknown', 'not set', 'config', 'no layer sets it under ' + S.profile, 'V2101: ' + TD.diag.help],
    'var.catalog.productIds': () => ['list<string>', '[p_1, p_7, p_9]', 'set', 'beforeAll catalog · saved', 'Saved by catalog#seedProducts; cached for 3h.'],
    'item': () => ['string', 'one of productIds', 'set', 'forEach item', 'The current element while forEach runs.'],
    'response.body.id': () => ['string', 'known after the call', 'request', 'runtime', 'The response of this step.']
  };
  function showCard(el) {
    const name = el.dataset.v;
    const f = HOVER[name] || (name.startsWith('example.') ? () => ['column', 'per row: ' + FM.appliesCoupon.examples.rows.map((r) => r[name === 'example.coupon' ? 0 : 1]).join(', '), 'set', 'examples', 'One instance per examples row.'] : null);
    if (!f) return;
    const [type, v, src, at, doc] = f();
    VC.tip(el, `<div class="row"><span class="mono">\${${esc(name)}}</span><span class="grow"></span><span class="chip mono">${esc(type)}</span></div><div class="tip-val">${src === 'secret' ? '<span class="redacted">••••</span>' : esc(v)}</div><div class="tip-src">${chipSrc(src)}<span>${esc(at)}</span></div><p class="ed-tipdoc">${esc(doc)}</p><div class="subtle">profile ${esc(S.profile)}</div>`, 'ed-tipcard');
  }
  const hideCard = () => VC.untip();
  document.addEventListener('mouseover', (e) => { const v = e.target.closest && e.target.closest('.ed-var'); if (v) showCard(v); });
  document.addEventListener('mouseout', (e) => { if (e.target.closest && e.target.closest('.ed-var')) hideCard(); });
  document.addEventListener('focusin', (e) => { if (e.target.matches && e.target.matches('.ed-var')) showCard(e.target); });
  document.addEventListener('focusout', (e) => { if (e.target.matches && e.target.matches('.ed-var')) hideCard(); });
  document.addEventListener('keydown', (e) => {
    if (S.route !== 'tests-editor') return;
    const mod = e.metaKey || e.ctrlKey;
    if (mod && e.code === 'KeyS') { e.preventDefault(); return A['ed-save'](); }
    if (e.ctrlKey && e.code === 'Space') { e.preventDefault(); return A['ed-suggest'](); }
    if (mod && e.key === '.' && !E.fixed) { e.preventDefault(); const b = $('.ed-bulb'); if (b) A['ed-bulb'](b); return; }
    const overlay = $('#dlg').open || $('#menu').matches(':popover-open') || $('#pick').matches(':popover-open') || S.drawer;
    if (e.key === 'Escape' && !E.complete && !overlay && !e.defaultPrevented) { e.preventDefault(); return A['ed-done'](); }
    if (!E.complete || !e.target.closest('#ed-ac')) return;
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') { e.preventDefault(); E.ac = (E.ac + (e.key === 'ArrowDown' ? 1 : -1) + TD.complete.length) % TD.complete.length; $$('#ed-ac [role="option"]').forEach((o, i) => o.setAttribute('aria-selected', i === E.ac)); $('#ed-ac').setAttribute('aria-activedescendant', 'ed-ac-' + E.ac); }
    else if (e.key === 'Enter') { e.preventDefault(); A['ed-ac-pick']({ dataset: { i: E.ac } }); }
    else if (e.key === 'Escape') { e.preventDefault(); E.complete = false; VC.render(); }
  });
})();
