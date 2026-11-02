// ===================== renderVals: everything the template shows =====================
function sevWorst(D) { var w = null; (D || []).forEach(function (d) { if (d.sev === 'error') w = 'error'; else if (!w && d.sev === 'warn') w = 'warn'; }); return w; }
function iconFlags(p) { var k = fileKind(p); return { iCfg: k === 'config', iYaml: k === 'api' || k === 'test' || k === 'load', iSnap: k === 'openapi', iJson: k === 'schema', iCsv: k === 'dataset', iTs: k === 'hook' }; }
function buildTree(c, pr) {
  var S = c.state, paths = Object.keys(S.files), f = S.filter.trim().toLowerCase(), rows = [];
  function rank(seg, depth, parent) {
    if (depth === 0) { var i = TOP_ORDER.indexOf(seg); return i < 0 ? 50 : i; }
    if (parent === 'services') { var j = SVC_ORDER.indexOf(seg); return j < 0 ? 50 : j; }
    return 0;
  }
  var root = { dirs: {}, files: [] };
  paths.forEach(function (p) {
    if (f && p.toLowerCase().indexOf(f) < 0) return;
    var segs = p.split('/'), node = root;
    for (var i = 0; i < segs.length - 1; i++) { node.dirs[segs[i]] = node.dirs[segs[i]] || { dirs: {}, files: [] }; node = node.dirs[segs[i]]; }
    node.files.push(p);
  });
  function fileRow(p, depth) {
    var F = S.files[p], w = sevWorst(pr.diags[p]), dirty = F.text !== F.original;
    return Object.assign({ isDir: false, isFile: true, name: baseOf(p), path: p, cls: 'trow d' + depth + (S.active === p ? ' on' : ''), on: function () { c.openFile(p); },
      hasDot: !!w, dotCls: 'dot ' + (w === 'error' ? 'fail' : 'block'), dirty: dirty, ro: !!F.ro,
      aria: p + (dirty ? ', unsaved' : '') + (w === 'error' ? ', has errors' : w === 'warn' ? ', has warnings' : '') + (F.ro ? ', read-only' : '') }, iconFlags(p));
  }
  function walk(node, depth, prefix, parent) {
    var entries = Object.keys(node.dirs).map(function (d) { return { d: d }; }).concat(node.files.map(function (p) { return { p: p }; }));
    entries.sort(function (a, b) {
      var an = a.d || baseOf(a.p), bn = b.d || baseOf(b.p);
      var ra = rank(an, depth, parent), rb = rank(bn, depth, parent);
      if (ra !== rb) return ra - rb;
      if (depth === 0 && !!a.d !== !!b.d) return a.d ? 1 : -1;
      var ka = a.p && /\.api\./.test(a.p) ? 0 : 1, kb = b.p && /\.api\./.test(b.p) ? 0 : 1;
      if (ka !== kb) return ka - kb;
      return an < bn ? -1 : an > bn ? 1 : 0;
    });
    entries.forEach(function (e) {
      if (e.p) { rows.push(fileRow(e.p, depth)); return; }
      var full = prefix + e.d, open = f ? true : !S.collapsed[full];
      var cnt = 0; (function count(n) { cnt += n.files.length; Object.keys(n.dirs).forEach(function (k) { count(n.dirs[k]); }); })(node.dirs[e.d]);
      rows.push({ isDir: true, isFile: false, name: e.d + '/', cls: 'trow dir d' + depth, open: open, exp: open ? 'true' : 'false', cnt: cnt, toggle: function () { var cl = Object.assign({}, S.collapsed); cl[full] = !cl[full]; c.setState({ collapsed: cl }); }, chev: open ? 'chev open' : 'chev', aria: (open ? 'Collapse ' : 'Expand ') + full });
      if (open) walk(node.dirs[e.d], depth + 1, full + '/', e.d);
    });
  }
  walk(root, 0, '', '');
  return rows;
}
function srcLines(c, kind, text, D, caretLine) {
  var lines = text.split('\n'), byLine = {};
  D.forEach(function (d) { if (d.line === null || d.line === undefined) return; var l = byLine[d.line]; if (!l || (l.sev !== 'error' && d.sev === 'error')) byLine[d.line] = d; });
  if (lines.length > 1 && lines[lines.length - 1] === '') lines.pop();
  return lines.map(function (l, i) {
    var d = byLine[i], toks = tokLine(kind, l, i + 1);
    if (d) { var seenKey = false; toks = toks.map(function (t) { var sq = t.c !== 'y-punc' && t.c !== 'y-com' && t.t.trim() !== '' && !(t.c === 'y-key' && toks.some(function (x) { return x.c !== 'y-key' && x.c !== 'y-punc' && x.c !== '' && x.c !== 'y-com'; })); return { t: t.t, c: t.c + (sq ? ' squig' + (d.sev === 'warn' ? ' sqw' : '') : '') }; }); }
    return { no: i + 1, toks: toks, cls: 'ln' + (i === caretLine ? ' cur' : '') + (d ? (d.sev === 'error' ? ' err' : ' wrn') : ''), hasLens: !!d, lens: d ? d.msg : '', lensCls: 'lens ' + (d && d.sev === 'error' ? 'le' : 'lw'), gut: d ? (d.sev === 'error' ? 'gd ge' : 'gd gw') : 'gd' };
  });
}
function buildView(c) {
  var S = c.state, pr = c.project(), ctx = pr.ctx, p = S.active, F = p ? S.files[p] : null, kind = p ? fileKind(p) : null;
  c.fset = {};
  var V = {};
  // ----- shell -----
  V.profiles = ctx.profiles.map(function (x) { return { id: x.id, sel: x.id === S.profile }; });
  V.profile = S.profile;
  V.onProfile = function (e) { c.setState({ profile: e.target.value }); };
  var collapsed = !!S.sideCollapsed;
  V.appCls = 'app ed' + (collapsed ? ' side-collapsed' : '');
  V.sideToggleLbl = collapsed ? 'Expand sidebar' : 'Collapse sidebar';
  V.sideExp = collapsed ? 'false' : 'true';
  V.sideChev = collapsed ? 'i flip' : 'i';
  V.toggleSide = function () { c.setState({ sideCollapsed: !c.state.sideCollapsed }); };
  V.filter = S.filter;
  V.onFilter = function (e) { c.setState({ filter: e.target.value }); };
  V.clearFilter = function () { c.setState({ filter: '' }); };
  V.hasFilter = !!S.filter;
  V.tree = buildTree(c, pr);
  V.treeEmpty = V.tree.length === 0;
  V.openNew = function () { c.openNew(null); };
  var dirtyN = Object.keys(S.files).filter(function (x) { return S.files[x].text !== S.files[x].original; }).length;
  V.dirtyN = dirtyN; V.hasDirty = dirtyN > 0; V.dirtyLbl = dirtyN + ' unsaved';
  V.editorNavTitle = dirtyN ? 'Editor, ' + dirtyN + ' unsaved' : 'Editor';
  // ----- tabs -----
  V.tabs = S.open.map(function (x) {
    var Fx = S.files[x], dirty = Fx.text !== Fx.original, w = sevWorst(pr.diags[x]);
    return Object.assign({ name: baseOf(x), path: x, cls: 'etab' + (x === p ? ' on' : ''), sel: x === p ? 'true' : 'false', dirty: dirty, on: function () { c.openFile(x); }, close: function () { c.closeTab(x); }, closeAria: 'Close ' + baseOf(x) + (dirty ? ' (unsaved changes)' : ''), hasDot: !!w && !dirty, dotCls: 'dot ' + (w === 'error' ? 'fail' : 'block') }, iconFlags(x));
  });
  V.hasFile = !!F; V.noFile = !F;
  V.toast = S.toast; V.hasToast = !!S.toast; V.toastCls = 'toast ' + (S.toast && S.toast.kind === 'ok' ? 'ok' : 'info');
  V.closeToast = function () { c.setState({ toast: null }); };
  buildDialogs(c, V, pr);
  if (!F) return V;
  // ----- header -----
  var P = c.parseFor(kind, F.text), D = pr.diags[p] || [], dirty = F.text !== F.original, view = c.viewOf(p), errs = D.filter(function (d) { return d.sev === 'error'; }), warns = D.filter(function (d) { return d.sev === 'warn'; });
  V.dir = dirOf(p) ? dirOf(p) + '/' : ''; V.base = baseOf(p); V.kindLbl = KIND_LABEL[kind]; V.ro = !!F.ro;
  V.engine = isYamlKind(kind) ? (S.jsy ? 'js-yaml' : 'built-in parser') : '';
  V.hasEngine = !!V.engine;
  var formOk = kind === 'test' || kind === 'api' || kind === 'config' || kind === 'load' || kind === 'dataset';
  var lang = kind === 'dataset' ? 'CSV' : kind === 'schema' ? 'JSON' : kind === 'hook' ? 'TS' : 'YAML';
  var noForm = kind === 'schema' ? 'Schemas are edited as JSON; there is no form for them' : kind === 'hook' ? 'Hooks are TypeScript; edit the source' : 'OpenAPI snapshots are written by vc import and vc sync and are read-only';
  V.segs = [
    { l: kind === 'dataset' ? 'Grid' : 'Form', k: 'form' }, { l: lang, k: 'source' }, { l: 'Split', k: 'split' }
  ].map(function (x) {
    var dis = !formOk && x.k !== 'source';
    return { l: x.l, cls: view === x.k ? 'on' : '', pressed: view === x.k ? 'true' : 'false', dis: dis, title: dis ? noForm : x.k === 'split' ? 'Form and ' + lang + ' side by side' : 'Edit as ' + x.l, on: function () { if (!dis) c.setView(x.k); } };
  });
  var ops = lineDiff(F.original, F.text), st = diffStats(ops), diffOn = !!S.diffOn[p];
  V.diffPressed = diffOn ? 'true' : 'false'; V.diffCls = 'btn sm' + (diffOn ? ' on' : '');
  V.diffAdd = '+' + st.add; V.diffDel = '−' + st.del; V.hasDiffN = dirty;
  V.diffTitle = dirty ? 'Show changes since the last save: ' + st.add + ' added, ' + st.del + ' removed lines' : 'No changes since the last save';
  V.toggleDiff = function () { c.toggleDiff(); };
  V.canSave = dirty && !F.ro; V.saveDis = !V.canSave;
  V.saveTitle = F.ro ? 'Read-only snapshot' : !dirty ? 'No unsaved changes' : errs.length ? 'Blocked: fix ' + errs.length + ' error' + (errs.length > 1 ? 's' : '') + ' first (vc check must pass)' : 'Save (Ctrl+S or Cmd+S)';
  V.saveCls = 'btn sm primary' + (dirty && errs.length ? ' blockedbtn' : '');
  V.onSave = function () { c.save(); };
  V.revertDis = !dirty; V.revertTitle = dirty ? 'Discard changes since the last save' : 'Nothing to revert';
  V.onRevert = function () { c.revert(); };
  V.isTestFile = kind === 'test';
  var sel = S.sel[p], tsel = kind === 'test' && /^test:\d+$/.test(sel || 'test:0') && F.model && Array.isArray(F.model.tests) && F.model.tests[+(sel || 'test:0').split(':')[1]];
  V.runLbl = tsel ? 'Run ' + (tsel.id || 'test') : 'Run file';
  V.runTitle = 'Runs on profile ' + S.profile + ' with your unsaved edits; nothing is written';
  V.onRun = function () { c.runTest(); };
  V.status = errs.length ? errs.length + ' error' + (errs.length > 1 ? 's' : '') : warns.length ? warns.length + ' warning' + (warns.length > 1 ? 's' : '') : 'vc check passes';
  V.statusCls = 'pill ' + (errs.length ? 'fail' : warns.length ? 'block' : 'pass');
  // ----- banners -----
  V.blocked = S.blocked && S.blocked.path === p; V.blockedMsg = V.blocked ? 'Not saved: ' + S.blocked.n + ' error' + (S.blocked.n > 1 ? 's' : '') + ' must be fixed first, because vc check has to pass before a file is written. First: ' + S.blocked.first : '';
  V.closeBlocked = function () { c.setState({ blocked: null }); };
  var R = S.run && S.run.path === p ? S.run : null;
  V.hasRun = !!R; V.running = !!(R && R.running); V.runDone = !!(R && !R.running);
  V.runText = R ? (R.running ? 'Running ' + R.label + ' on ' + S.profile + '…' : R.title) : '';
  V.runCls = 'runres ' + (R && !R.running ? R.status : 'running');
  V.runPill = R && !R.running ? R.status : 'running'; V.runPillCls = 'pill ' + (R && !R.running ? ({ passed: 'pass', failed: 'fail', blocked: 'block', skipped: 'skip' }[R.status] || '') : 'accent');
  V.runLines = R && R.lines ? R.lines.map(function (l) { return { txt: l.txt, cls: l.skip ? 'rl skip' : l.ok ? 'rl ok' : 'rl bad', ok: l.ok && !l.skip, bad: !l.ok && !l.skip, sk: !!l.skip }; }) : [];
  V.closeRun = function () { c.setState({ run: null }); };
  // ----- body -----
  V.showDiff = diffOn; V.showEditor = !diffOn;
  V.showForm = !diffOn && (view === 'form' || view === 'split');
  V.showSrc = !diffOn && (view === 'source' || view === 'split');
  V.panesCls = 'epanes ' + (view === 'split' ? 'esplit' : 'single');
  V.diffEmpty = !dirty;
  V.diffModeSbs = S.diffMode === 'sbs'; V.diffModeUni = S.diffMode !== 'sbs';
  V.setUni = function () { c.setState({ diffMode: 'uni' }); }; V.setSbs = function () { c.setState({ diffMode: 'sbs' }); };
  V.uniCls = S.diffMode !== 'sbs' ? 'on' : ''; V.sbsCls = S.diffMode === 'sbs' ? 'on' : '';
  V.uniPressed = S.diffMode !== 'sbs' ? 'true' : 'false'; V.sbsPressed = S.diffMode === 'sbs' ? 'true' : 'false';
  V.diffHead = dirty ? st.add + ' added, ' + st.del + ' removed since the last save' : 'No changes since the last save';
  if (diffOn) {
    V.uni = diffHunks(ops, 3).map(function (o) {
      if (o.gap) return { isGap: true, isLn: false, txt: o.gap + ' unchanged line' + (o.gap > 1 ? 's' : '') };
      return { isGap: false, isLn: true, a: o.a || '', b: o.b || '', sign: o.t === '=' ? ' ' : o.t === '+' ? '+' : '−', text: o.text, cls: 'dl ' + (o.t === '+' ? 'add' : o.t === '-' ? 'del' : '') };
    });
    var sbsRows = diffSideBySide(ops), keep = sbsRows.map(function () { return false; });
    sbsRows.forEach(function (r, i) { if (!(r.l && r.r && r.l.t === '=')) for (var k = Math.max(0, i - 3); k <= Math.min(sbsRows.length - 1, i + 3); k++) keep[k] = true; });
    V.sbs = []; var gap = 0;
    sbsRows.forEach(function (r, i) {
      if (!keep[i]) { gap++; return; }
      if (gap) { V.sbs.push({ isGap: true, isLn: false, txt: gap + ' unchanged line' + (gap > 1 ? 's' : '') }); gap = 0; }
      var same = r.l && r.r && r.l.t === '=';
      V.sbs.push({ isGap: false, isLn: true, la: r.l ? r.l.a : '', lt: r.l ? r.l.text : '', lcls: 'dc ' + (!r.l ? 'none' : same ? '' : 'del'), rb: r.r ? r.r.b : '', rt: r.r ? r.r.text : '', rcls: 'dc ' + (!r.r ? 'none' : same ? '' : 'add') });
    });
    if (gap) V.sbs.push({ isGap: true, isLn: false, txt: gap + ' unchanged line' + (gap > 1 ? 's' : '') });
  } else { V.uni = []; V.sbs = []; }
  // ----- form -----
  V.parseErr = !P.ok; V.parseErrLine = P.ok ? 0 : P.error.line; V.parseErrMsg = P.ok ? '' : P.error.msg;
  V.goParseErr = function () { var v = c.viewOf(p); if (v === 'form') c.setView('split'); c.jump({ line: P.ok ? 0 : P.error.line - 1 }); };
  V.formOff = !formOk;
  V.fnav = []; V.fsecs = []; V.hasNav = false; V.navAdd = null; V.hasNavAdd = false;
  if (V.showForm && formOk && P.ok) {
    var dmap = {};
    D.forEach(function (d) { if (d.path) { var k = pk(d.path); if (!dmap[k] || (dmap[k].sev !== 'error' && d.sev === 'error')) dmap[k] = d; } });
    var B = { c: c, m: kind === 'dataset' ? P.value : (isObj(P.value) ? P.value : {}), kind: kind, fpath: p, ctx: ctx, dmap: dmap, dlist: D, fc: S.fc, kd: S.kd, flash: S.flash, bodyMode: S.bodyMode, sel: S.sel[p] };
    var form = kind === 'test' ? buildTestForm(B) : kind === 'api' ? buildApiForm(B) : kind === 'config' ? buildConfigForm(B) : kind === 'load' ? buildLoadForm(B) : buildDatasetForm(B, P, usedByOf(p, pr.texts));
    V.fnav = form.nav; V.hasNav = form.nav.length > 0; V.fsecs = form.secs;
    V.navAdd = form.add; V.hasNavAdd = !!form.add;
    V.navAddLbl = form.add ? form.add.l : ''; V.navAddOn = form.add ? form.add.on : null;
    V.navLbl = kind === 'test' ? 'Tests' : kind === 'api' ? 'Apis' : 'Sections';
  }
  V.formMainCls = 'fmain' + (V.hasNav ? '' : ' nonav');
  // ----- source -----
  var cl = S.caret[p] ? S.caret[p].line : -1;
  V.src = F.text;
  V.srcEditable = !F.ro; V.srcHide = F.ro ? 'false' : 'true';
  V.lines = V.showSrc ? srcLines(c, kind, F.text, D, cl) : [];
  var maxLen = 0; F.text.split('\n').forEach(function (l) { if (l.length > maxLen) maxLen = l.length; });
  var hgt = (Math.max(1, V.lines.length) + 2) * LH + 24;
  V.taStyle = 'height: ' + hgt + 'px; width: max(100%, calc(' + (maxLen + 6) + 'ch + 64px))';
  V.innerStyle = 'min-height: ' + hgt + 'px; width: max(100%, calc(' + (maxLen + 6) + 'ch + 64px))';
  V.onSrcInput = function (e) { c.srcInput(e); };
  V.onSrcKey = function (e) { c.srcKey(e); };
  V.onSrcCaret = function (e) { c.srcCaret(e); };
  V.onSrcBlur = function () { setTimeout(function () { if (c.state.comp) c.setState({ comp: null }); }, 160); };
  V.srcLabel = (F.ro ? 'Read-only ' : '') + V.base + ' source';
  var comp = S.comp && S.comp.path === p ? S.comp : null;
  V.srcComp = !!comp && V.showSrc;
  V.srcCompStyle = comp ? 'top: ' + (12 + (comp.line + 1) * LH) + 'px; left: calc(' + comp.col + 'ch + 64px)' : '';
  V.srcItems = comp ? comp.items.map(function (it, i) { return { l: it.label, d: it.detail || '', cls: 'opt' + (i === comp.sel ? ' on' : ''), hasM: !!it.method, m: it.method || '', mcls: 'method m-' + String(it.method || '').toLowerCase(), pick: function (e) { if (e && e.preventDefault) e.preventDefault(); c.acceptSrc(i, true); } }; }) : [];
  V.srcHint = isYamlKind(kind) && !F.ro ? 'Tab indents · Ctrl+Space suggests · Enter or Tab accepts · Esc closes' : F.ro ? 'Snapshot written by vc import or vc sync; read-only' : 'Tab indents';
  // aside for schema / hook / dataset
  V.hasAside = false; V.asideTitle = ''; V.aside = [];
  if (kind === 'schema') { V.hasAside = true; V.asideTitle = 'Used by'; V.aside = usedByOf(p, pr.texts).map(function (u) { return { t: u, s: '' }; }); if (!V.aside.length) V.aside = [{ t: 'Nothing references this schema yet', s: '' }]; }
  if (kind === 'hook') {
    V.hasAside = true; V.asideTitle = 'Exports';
    V.aside = hookExports(F.text).map(function (e) {
      var refs = []; Object.keys(pr.texts).forEach(function (q) { if (!isYamlKind(fileKind(q))) return; pr.texts[q].split('\n').forEach(function (l, li) { if (l.indexOf('#' + e.name) >= 0 && l.indexOf(baseOf(p)) >= 0) refs.push(q + ':' + (li + 1)); }); });
      return { t: e.name + ' (line ' + e.line + ')', s: refs.length ? 'used by ' + refs.join(', ') : 'not referenced yet' };
    });
  }
  if (kind === 'openapi') { V.hasAside = true; V.asideTitle = 'Snapshot'; var sn = ctx.snapshots.filter(function (s) { return s.path === p; })[0]; V.aside = [{ t: 'Schemas: ' + (sn ? sn.schemas.join(', ') : 'none'), s: 'Pick them in an expect builder as body schema' }, { t: 'Refresh with vc sync', s: 'This file is written by the CLI, not edited by hand' }]; }
  // ----- problems -----
  V.errN = errs.length; V.warnN = warns.length; V.hasErr = errs.length > 0; V.hasWarn = warns.length > 0; V.noProblems = !D.length;
  V.probOpen = S.probOpen; V.probExp = S.probOpen ? 'true' : 'false';
  V.toggleProb = function () { c.setState({ probOpen: !S.probOpen }); };
  V.probChev = S.probOpen ? 'chev open' : 'chev';
  V.errLbl = errs.length + (errs.length === 1 ? ' error' : ' errors'); V.warnLbl = warns.length + (warns.length === 1 ? ' warning' : ' warnings');
  V.errBadge = 'badge ' + (errs.length ? 'e' : 'z'); V.warnBadge = 'badge ' + (warns.length ? 'w' : 'z');
  V.problems = D.slice().sort(function (a, b) { return (a.sev === 'error' ? 0 : 1) - (b.sev === 'error' ? 0 : 1) || (a.line === null ? 1e9 : a.line) - (b.line === null ? 1e9 : b.line); }).map(function (d) {
    return { msg: d.msg, cls: 'prob ' + (d.sev === 'error' ? 'pe' : 'pw'), isErr: d.sev === 'error', isWarn: d.sev !== 'error', loc: (d.line !== null && d.line !== undefined ? 'line ' + (d.line + 1) : '') + (d.path && d.path.length ? (d.line !== null && d.line !== undefined ? ' · ' : '') + d.path.join('.') : ''),
      go: function () { c.jump(d); }, goAria: 'Go to: ' + d.msg,
      fixes: (d.fixes || []).map(function (fx) { return { l: fx.label, on: function () { c.applyFixObj(fx, d); } }; }), hasFix: !!(d.fixes && d.fixes.length) };
  });
  return V;
}
function buildDialogs(c, V, pr) {
  var S = c.state, nw = S.dlg;
  V.dlgOpen = !!nw;
  V.closeDlg = function () { c.setState({ dlg: null }); };
  V.confirmOpen = !!S.confirm;
  if (S.confirm) {
    var cp = S.confirm.path, cerr = (pr.diags[cp] || []).filter(function (d) { return d.sev === 'error'; }).length;
    V.confirmName = baseOf(cp); V.confirmSaveDis = cerr > 0; V.confirmSaveTitle = cerr ? 'Fix ' + cerr + ' error' + (cerr > 1 ? 's' : '') + ' first' : 'Save, then close';
    V.confirmSave = function () { if (c.save(cp)) setTimeout(function () { c.closeTab(cp, true); }, 0); };
    V.confirmDiscard = function () { var files = Object.assign({}, c.state.files); files[cp] = Object.assign({}, files[cp], { text: files[cp].original, model: c.parseFor(fileKind(cp), files[cp].original).value }); c.setState({ files: files }); setTimeout(function () { c.closeTab(cp, true); }, 0); };
    V.confirmCancel = function () { c.setState({ confirm: null }); };
  }
  if (!nw) { V.dlg = {}; return; }
  var plan = planNew(nw, S.files, pr.ctx), ctx = pr.ctx;
  var kinds = [{ k: 'test', l: 'Test' }, { k: 'api', l: 'Api' }, { k: 'testFile', l: 'Test file' }, { k: 'load', l: 'Load file' }, { k: 'schema', l: 'Schema' }, { k: 'dataset', l: 'Dataset' }, { k: 'hook', l: 'Hook' }];
  var svcs = servicesOf(S.files);
  var d = {
    kinds: kinds.map(function (x) { return { l: x.l, cls: nw.kind === x.k ? 'on' : '', pressed: nw.kind === x.k ? 'true' : 'false', on: function () { c.dlgSet({ kind: x.k, idEdited: false, id: '' }); } }; }),
    file: nw.file, showSvc: ['api', 'testFile', 'schema'].indexOf(nw.kind) >= 0 || (nw.kind === 'test' && nw.file === '+new'),
    showArea: nw.kind === 'testFile' || nw.kind === 'load' || (nw.kind === 'test' && nw.file === '+new'), areaPh: nw.kind === 'load' ? 'checkout' : 'history',
    isTest: nw.kind === 'test', isApi: nw.kind === 'api', isTestFile: nw.kind === 'testFile', isLoad: nw.kind === 'load', isSchema: nw.kind === 'schema', isDataset: nw.kind === 'dataset', isHook: nw.kind === 'hook',
    name: nw.name, onName: function (e) { c.dlgSet({ name: e.target.value }); },
    nameLbl: { test: 'What does it check? (a sentence)', api: 'What does it do?', testFile: 'Display name', load: 'Display name', schema: 'What does it describe?', dataset: 'Dataset name', hook: 'What does the function check?' }[nw.kind],
    namePh: { test: 'Rejects a declined card', api: 'Start a user export', testFile: 'Order history', load: 'Black Friday checkout mix', schema: 'line item', dataset: 'world cities', hook: 'Totals add up' }[nw.kind],
    showId: nw.kind === 'test' || nw.kind === 'api' || nw.kind === 'hook',
    idVal: nw.idEdited ? nw.id : plan.derived, onId: function (e) { c.dlgSet({ id: e.target.value, idEdited: true }); },
    idCls: 'input mono' + (plan.sugg !== undefined && plan.sugg !== null || (plan.id && !isCamel(plan.id)) ? ' err' : ''),
    idHelp: nw.idEdited ? 'Edited by hand' : 'Derived from the name: camelCase, articles dropped',
    hasSugg: !!plan.sugg, sugg: plan.sugg || '', useSugg: function () { c.dlgSet({ id: plan.sugg, idEdited: true }); },
    resetId: function () { c.dlgSet({ idEdited: false, id: '' }); }, canReset: !!nw.idEdited,
    fileOpts: Object.keys(S.files).filter(function (x) { return fileKind(x) === 'test'; }).map(function (x) { return { v: x, l: x, sel: nw.file === x }; }).concat([{ v: '+new', l: 'New test file…', sel: nw.file === '+new' }]),
    onFile: function (e) { c.dlgSet({ file: e.target.value }); }, newTestFile: nw.kind === 'test' && nw.file === '+new',
    svcOpts: svcs.map(function (x) { return { v: x, l: x, sel: nw.service === x }; }).concat([{ v: '+new', l: 'New service…', sel: nw.service === '+new' }]),
    onSvc: function (e) { c.dlgSet({ service: e.target.value }); }, newSvc: nw.service === '+new',
    newService: nw.newService, onNewSvc: function (e) { c.dlgSet({ newService: e.target.value }); },
    area: nw.area, onArea: function (e) { c.dlgSet({ area: e.target.value }); },
    scenario: nw.scenario, onScenario: function (e) { c.dlgSet({ scenario: e.target.value }); },
    methodOpts: METHODS.slice(0, 5).map(function (x) { return { v: x, l: x, sel: nw.method === x }; }), onMethod: function (e) { c.dlgSet({ method: e.target.value }); },
    apiPath: nw.apiPath, onApiPath: function (e) { c.dlgSet({ apiPath: e.target.value }); },
    apiOpts: [{ v: '', l: 'No, start empty', sel: !nw.fromApi }].concat(ctx.apis.map(function (a) { return { v: a.full, l: a.full + '  ' + a.method + ' ' + a.path, sel: nw.fromApi === a.full }; })),
    onFromApi: function (e) { c.dlgSet({ fromApi: e.target.value }); },
    hookOpts: ctx.hooks.map(function (h) { return { v: h.path, l: h.path, sel: nw.hookFile === h.path }; }).concat([{ v: '+new', l: 'New hook file…', sel: nw.hookFile === '+new' }]),
    onHookFile: function (e) { c.dlgSet({ hookFile: e.target.value }); }, newHook: nw.hookFile === '+new',
    purpose: nw.purpose, onPurpose: function (e) { c.dlgSet({ purpose: e.target.value }); },
    columns: nw.columns, onColumns: function (e) { c.dlgSet({ columns: e.target.value }); },
    target: plan.path || '…', targetNote: plan.newFile ? 'new file' : plan.path ? 'existing file' : '',
    fullId: plan.kind === 'test' || plan.kind === 'api' ? plan.full : plan.kind === 'hook' ? plan.full : plan.id,
    hasFull: !!(plan.full || plan.id),
    errs: plan.errors.map(function (t) { return { t: t }; }), hasErrs: plan.errors.length > 0 && !!(nw.name || nw.area || nw.idEdited),
    notes: plan.notes.map(function (t) { return { t: t }; }), hasNotes: plan.notes.length > 0,
    canCreate: plan.ok, createDis: !plan.ok, create: function () { c.createNew(); },
    createLbl: plan.newFile ? 'Create file' : 'Add to file',
    createTitle: plan.ok ? (plan.newFile ? 'Create ' + plan.path : 'Add to ' + plan.path) : 'Fix the issues listed first',
    preview: plan.kind === 'test' && plan.ok && nw.fromApi ? flowOf(happyPathTest(plan.id, String(nw.name).trim(), nw.fromApi, ctx)) : '', hasPreview: plan.kind === 'test' && plan.ok && !!nw.fromApi,
    onKey: function (e) { if (e.key === 'Escape') c.setState({ dlg: null }); if (e.key === 'Enter' && plan.ok && e.target.tagName === 'INPUT') { e.preventDefault(); c.createNew(); } }
  };
  V.dlg = d;
}
