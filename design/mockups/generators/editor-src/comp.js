// ===================== the component =====================
var LH = 20;
var KIND_LABEL = { test: 'test file', api: 'api file', config: 'project config', load: 'load file', dataset: 'dataset', schema: 'JSON schema', hook: 'hook', openapi: 'OpenAPI snapshot', text: 'file' };
var TOP_ORDER = ['vibranium.config.yaml', 'services', 'load', 'schemas', 'datasets', 'hooks', 'openapi'];
var SVC_ORDER = ['users', 'orders', 'catalog', 'carts', 'payments', 'geo'];
function parseYamlText(text) {
  var mine = yParseSafe(text);
  if (typeof jsyaml !== 'undefined' && jsyaml && jsyaml.load) {
    try {
      var v = jsyaml.load(text, { schema: jsyaml.CORE_SCHEMA });
      return { ok: true, value: v === undefined ? null : v, index: mine.ok ? mine.index : {}, comments: mine.ok ? mine.comments : {}, engine: 'js-yaml' };
    } catch (e) {
      return { ok: false, error: { msg: String(e.reason || e.message || 'syntax error').split('\n')[0], line: e.mark ? e.mark.line + 1 : 1 }, index: {}, comments: {} };
    }
  }
  return mine;
}
function parseByKind(kind, text) {
  if (isYamlKind(kind)) return parseYamlText(text);
  if (kind === 'schema') return jsonParse(text);
  if (kind === 'dataset') return csvParse(text);
  return { ok: true, value: text };
}
class Component extends DCLogic {
  constructor(props) {
    super(props);
    var files = {};
    Object.keys(SEED).forEach(function (p) { var k = fileKind(p); files[p] = { text: SEED[p], original: SEED[p], ro: k === 'openapi', model: null }; });
    Object.keys(files).forEach(function (p) { var r = parseByKind(fileKind(p), files[p].text); files[p].model = r.ok ? r.value : null; });
    this.pcache = {}; this.fset = {}; this.memo = {};
    this.pendingSel = null; this.pendingScroll = null; this.pendingFocus = null;
    var start = 'services/orders/checkout.test.yaml';
    this.state = {
      files: files, open: [start, 'services/users/users.api.yaml', 'vibranium.config.yaml', 'load/checkout.load.yaml'], active: start,
      view: {}, sel: {}, diffOn: {}, diffMode: 'sbs', caret: {}, comp: null, fc: null, kd: {}, bodyMode: {},
      probOpen: true, filter: '', collapsed: {}, dlg: null, confirm: null, toast: null, blocked: null, run: null, flash: null,
      profile: 'staging', menu: null, sideCollapsed: false, jsy: typeof jsyaml !== 'undefined'
    };
    this.state.sel[start] = 'test:0';
  }
  componentDidMount() {
    var self = this, tries = 0;
    function poll() {
      if (typeof jsyaml !== 'undefined') { self.pcache = {}; self.memo = {}; self.setState({ jsy: true }); return; }
      if (++tries < 20) setTimeout(poll, 250);
    }
    if (!this.state.jsy) setTimeout(poll, 250);
  }
  componentDidUpdate() {
    if (typeof document === 'undefined') return;
    try {
      if (this.pendingSel !== null) {
        var ta = document.getElementById('src-ta');
        if (ta) { ta.focus(); ta.setSelectionRange(this.pendingSel, this.pendingSel); }
        this.pendingSel = null;
      }
      if (this.pendingScroll !== null) {
        var w = document.getElementById('src-scroll');
        if (w) { var top = this.pendingScroll * LH; if (top < w.scrollTop + 40 || top > w.scrollTop + w.clientHeight - 60) w.scrollTop = Math.max(0, top - 120); }
        this.pendingScroll = null;
      }
      if (this.pendingFocus) {
        var f = this.pendingFocus; this.pendingFocus = null;
        var el = null, ids = f.ids || [];
        for (var i = 0; i < ids.length && !el; i++) el = document.getElementById(ids[i]);
        if (el) { if (el.scrollIntoView) el.scrollIntoView({ block: 'center', behavior: 'smooth' }); if (!f.noFocus && el.focus) { el.focus({ preventScroll: true }); if (f.select && el.select) el.select(); } }
      }
    } catch (e) { }
  }
  parseFor(kind, text) {
    var key = kind + PK + text;
    if (!this.pcache[key]) { if (Object.keys(this.pcache).length > 300) this.pcache = {}; this.pcache[key] = parseByKind(kind, text); }
    return this.pcache[key];
  }
  // ---------- derived project state ----------
  project() {
    var S = this.state;
    if (this.memo.files === S.files && this.memo.jsy === S.jsy) return this.memo.val;
    var self = this, texts = {};
    Object.keys(S.files).forEach(function (p) { texts[p] = S.files[p].text; });
    var ctx = buildCtx(texts, function (t) { return self.parseFor('api', t); });
    var diags = {};
    Object.keys(S.files).forEach(function (p) { var k = fileKind(p); diags[p] = diagnose(p, k, texts[p], self.parseFor(k, texts[p]), ctx); });
    this.memo = { files: S.files, jsy: S.jsy, val: { ctx: ctx, diags: diags, texts: texts } };
    return this.memo.val;
  }
  // ---------- editing pipeline: one source of truth per file ----------
  applyText(p, t, extra) {
    var S = this.state, f = S.files[p];
    if (!f || f.ro) return;
    var r = this.parseFor(fileKind(p), t), nf = Object.assign({}, f, { text: t });
    if (r.ok) nf.model = r.value;
    var files = Object.assign({}, S.files); files[p] = nf;
    var run = S.run && S.run.path === p ? null : S.run;
    this.setState(Object.assign({ files: files, blocked: null, run: run }, extra || {}));
  }
  editModel(fn, path) {
    var S = this.state, p = path || S.active, f = S.files[p];
    if (!f || f.ro) return;
    var kind = fileKind(p), P = this.parseFor(kind, f.text);
    if (!P.ok) return;
    var m = clone(P.value);
    if (m === null || m === undefined) m = kind === 'dataset' ? { cols: [], rows: [] } : {};
    var r = fn(m); if (r !== undefined) m = r;
    var t;
    if (isYamlKind(kind)) t = yApplyModel(f.text, kind, m);
    else if (kind === 'dataset') t = csvStringify(m);
    else return;
    if (t !== f.text) this.applyText(p, t);
  }
  setDraft(fid, raw) { var kd = Object.assign({}, this.state.kd); if (raw === undefined) delete kd[fid]; else kd[fid] = raw; this.setState({ kd: kd }); }
  fieldInput(fid, raw, caret, mode, path, commit, noDraft) {
    var S = this.state, kd = Object.assign({}, S.kd), fc = null;
    if (!noDraft) kd[fid] = raw;
    if (mode) {
      var pr = this.project(), kind = fileKind(S.active), m = S.files[S.active].model;
      var site = path && m ? siteAt(sitesFor(kind, m, pr.ctx), path) : null;
      var env = { ctx: pr.ctx, scope: path && m ? scopeForPath(kind, m, pr.ctx, path) : baseScope(pr.ctx), ex: site ? site.ex : null, ids: site ? site.ids : [], fpath: S.active, local: m && Array.isArray(m.apis) ? m.apis.filter(isObj).map(function (a) { return a.id; }) : [] };
      var r = fieldCompletion(raw, caret === null || caret === undefined ? String(raw).length : caret, mode, env);
      if (r && r.items.length && !(r.whole && r.items.length === 1 && r.items[0].insert === raw)) fc = { fid: fid, items: r.items, sel: 0, from: r.from, to: r.to, whole: !!r.whole, val: raw, mode: mode, path: path };
    }
    this.setState({ kd: kd, fc: fc });
    if (commit) commit(raw);
  }
  fieldKey(e, fid) {
    var fc = this.state.fc;
    if (e.key === 's' && (e.metaKey || e.ctrlKey)) { e.preventDefault(); this.save(); return; }
    if (!fc || fc.fid !== fid) return;
    if (e.key === 'ArrowDown') { e.preventDefault(); this.setState({ fc: Object.assign({}, fc, { sel: (fc.sel + 1) % fc.items.length }) }); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); this.setState({ fc: Object.assign({}, fc, { sel: (fc.sel - 1 + fc.items.length) % fc.items.length }) }); }
    else if (e.key === 'Enter' || e.key === 'Tab') { e.preventDefault(); this.acceptField(fc.sel); }
    else if (e.key === 'Escape') { e.preventDefault(); this.setState({ fc: null }); }
  }
  fieldBlur(fid) {
    var S = this.state, self = this;
    if (S.kd[fid] !== undefined) { var kd = Object.assign({}, S.kd); delete kd[fid]; this.setState({ kd: kd }); }
    if (S.fc && S.fc.fid === fid) setTimeout(function () { if (self.state.fc && self.state.fc.fid === fid) self.setState({ fc: null }); }, 160);
  }
  acceptField(i, mouse) {
    var fc = this.state.fc, now = Date.now();
    if (!fc || (mouse && this.lastPick && now - this.lastPick < 250)) return;
    if (mouse) this.lastPick = now;
    var it = fc.items[i], v = fc.val, nv;
    if (fc.whole) nv = it.insert;
    else { var after = v.slice(fc.to); nv = v.slice(0, fc.from) + it.insert + (it.close && after[0] !== '}' ? '}' : '') + after; }
    var commit = this.fset[fc.fid];
    var kd = Object.assign({}, this.state.kd); kd[fc.fid] = nv;
    this.setState({ fc: null, kd: kd });
    if (commit) commit(nv);
    if (it.reopen) this.fieldInput(fc.fid, nv, fc.from + it.insert.length, fc.mode, fc.path, null);
  }
  renameKeyAt(path, from, raw, fidK) {
    var to = String(raw).trim(), S = this.state, f = S.files[S.active], obj = f.model ? getIn(f.model, path) : null;
    var kd = Object.assign({}, S.kd);
    if (!to || (to !== from && isObj(obj) && Object.prototype.hasOwnProperty.call(obj, to))) { kd[fidK] = raw; this.setState({ kd: kd }); return; }
    if (kd[fidK] !== undefined) { delete kd[fidK]; this.setState({ kd: kd }); }
    if (to === from) return;
    this.editModel(function (m) { setIn(m, path, renameKey(getIn(m, path), from, to)); });
  }
  addKey(path, base, val) {
    var f = this.state.files[this.state.active], cur = f.model ? getIn(f.model, path) : null, k = uniqueKey(isObj(cur) ? cur : {}, base);
    this.editModel(function (m) { var o = getIn(m, path); if (!isObj(o)) { o = {}; setIn(m, path, o); } o[k] = val; });
    this.focusLater(fidOf(path.concat(k)) + '-k', true);
  }
  focusLater(fid, select) { this.pendingFocus = { ids: [fid], select: !!select }; }
  moveItem(arrPath, i, d) {
    var j = i + d, self = this, p = this.state.active;
    this.editModel(function (m) { var a = getIn(m, arrPath); if (!Array.isArray(a) || j < 0 || j >= a.length) return; var t = a[i]; a[i] = a[j]; a[j] = t; });
    var sel = this.state.sel[p] || '';
    var mm = /^(test|api):(\d+)$/.exec(sel);
    if (mm && arrPath.length === 1 && +mm[2] === i) self.setSelFor(p, mm[1] + ':' + j);
  }
  dupItem(arrPath, i) {
    var p = this.state.active, sel = this.state.sel[p] || '';
    this.editModel(function (m) {
      var a = getIn(m, arrPath), c = clone(a[i]);
      if (isObj(c) && typeof c.id === 'string') c.id = uniqueId(c.id, a.filter(isObj).map(function (x) { return x.id; }));
      if (isObj(c) && typeof c.name === 'string' && arrPath[0] === 'tests') c.name = c.name + ' (copy)';
      a.splice(i + 1, 0, c);
    });
    var mm = /^(test|api):(\d+)$/.exec(sel);
    if (mm && arrPath.length === 1) this.setSelFor(p, mm[1] + ':' + (i + 1));
  }
  delItem(arrPath, i) {
    var p = this.state.active, sel = this.state.sel[p] || '';
    this.editModel(function (m) { setIn(m, arrPath.concat(i), undefined); });
    var mm = /^(test|api):(\d+)$/.exec(sel);
    if (mm && arrPath.length === 1 && +mm[2] >= i && +mm[2] > 0) this.setSelFor(p, mm[1] + ':' + (+mm[2] - 1));
  }
  addTest() {
    var p = this.state.active, m = this.state.files[p].model, n = isObj(m) && Array.isArray(m.tests) ? m.tests.length : 0;
    var ids = n ? m.tests.filter(isObj).map(function (t) { return t.id; }) : [];
    var id = ids.indexOf('newTest') < 0 ? 'newTest' : uniqueId('newTest', ids);
    this.editModel(function (mm) { if (!Array.isArray(mm.tests)) mm.tests = []; mm.tests.push({ id: id, name: 'New test', call: '', expect: { status: 200 } }); });
    this.setSelFor(p, 'test:' + n);
    this.focusLater(fidOf(['tests', n, 'name']), true);
  }
  setSelFor(p, key) { var sel = Object.assign({}, this.state.sel); sel[p] = key; this.setState({ sel: sel, fc: null }); }
  setSel(key) { this.setSelFor(this.state.active, key); }
  setBodyMode(fid, md) { var b = Object.assign({}, this.state.bodyMode); b[fid] = md; this.setState({ bodyMode: b }); }
  applyFixObj(fix, d) {
    var S = this.state, p = S.active, f = S.files[p];
    if (fix.type === 'jump') { this.jump({ path: fix.path, line: d ? d.line : null }); return; }
    var t = applyFix(f.text, fileKind(p), fix);
    if (t !== f.text) this.applyText(p, t, { toast: { msg: 'Applied: ' + fix.label, sub: p, kind: 'info' } });
  }
  // ---------- files and tabs ----------
  openFile(p) {
    var S = this.state, open = S.open.indexOf(p) >= 0 ? S.open : S.open.concat(p);
    this.setState({ open: open, active: p, comp: null, fc: null, menu: null });
  }
  closeTab(p, force) {
    var S = this.state, f = S.files[p];
    if (!force && f && f.text !== f.original) { this.setState({ confirm: { path: p } }); return; }
    var open = S.open.filter(function (x) { return x !== p; }), active = S.active;
    if (active === p) { var i = S.open.indexOf(p); active = open[Math.min(i, open.length - 1)] || null; }
    this.setState({ open: open, active: active, confirm: null, comp: null, fc: null });
  }
  setView(v) { var view = Object.assign({}, this.state.view); view[this.state.active] = v; var d = Object.assign({}, this.state.diffOn); d[this.state.active] = false; this.setState({ view: view, diffOn: d, comp: null, fc: null }); }
  toggleDiff() { var d = Object.assign({}, this.state.diffOn); d[this.state.active] = !d[this.state.active]; this.setState({ diffOn: d, comp: null }); }
  save(p) {
    p = p || this.state.active;
    var S = this.state, f = S.files[p];
    if (!f || f.ro || f.text === f.original) return false;
    var pr = this.project(), D = pr.diags[p] || [], errs = D.filter(function (d) { return d.sev === 'error'; }), warns = D.filter(function (d) { return d.sev === 'warn'; });
    if (errs.length) { this.setState({ blocked: { path: p, n: errs.length, first: errs[0].msg }, probOpen: true }); return false; }
    var files = Object.assign({}, S.files); files[p] = Object.assign({}, f, { original: f.text });
    this.setState({ files: files, blocked: null, confirm: null, toast: { msg: 'Saved ' + p, sub: 'vc check passed' + (warns.length ? ' with ' + warns.length + ' warning' + (warns.length > 1 ? 's' : '') : ''), kind: 'ok' } });
    var self = this; setTimeout(function () { if (self.state.toast && self.state.toast.msg === 'Saved ' + p) self.setState({ toast: null }); }, 3800);
    return true;
  }
  revert() {
    var S = this.state, p = S.active, f = S.files[p];
    if (!f || f.text === f.original) return;
    this.applyText(p, f.original, { kd: {}, fc: null, comp: null, toast: { msg: 'Reverted ' + p, sub: 'back to the last saved version', kind: 'info' } });
  }
  runTest() {
    var S = this.state, p = S.active, self = this, sel = S.sel[p] || 'test:0';
    var pr = this.project(), m = S.files[p].model;
    this.setState({ run: { path: p, running: true, label: /^test:/.test(sel) && m && m.tests && m.tests[+sel.split(':')[1]] ? m.tests[+sel.split(':')[1]].id : (m && m.id) || p } });
    setTimeout(function () {
      var cur = self.state.run; if (!cur || cur.path !== p) return;
      var res = simulateRun(p, self.state.files[p].model, sel, self.project().diags[p] || [], self.state.profile);
      self.setState({ run: Object.assign({ path: p, running: false }, res) });
    }, 650);
  }
  // ---------- source editor ----------
  srcComp(text, caret, force) {
    var S = this.state, p = S.active, kind = fileKind(p);
    if (!isYamlKind(kind) || kind === 'openapi') return null;
    var pr = this.project(), P = this.parseFor(kind, text), f = S.files[p];
    var lastGood = P.ok ? P : this.parseFor(kind, f.text).ok ? this.parseFor(kind, f.text) : null;
    var r = yamlCompletion(text, caret, { fpath: p, kind: kind, ctx: pr.ctx, model: P.ok ? P.value : f.model, index: (P.ok ? P.index : lastGood && lastGood.index) || {}, force: !!force });
    if (!r || !r.items.length) return null;
    var ls = text.lastIndexOf('\n', r.from - 1) + 1, line = text.slice(0, ls).split('\n').length - 1;
    return { path: p, items: r.items, sel: 0, from: r.from, to: r.to, line: line, col: r.from - ls };
  }
  caretInfo(text, pos) { var ls = text.lastIndexOf('\n', pos - 1) + 1; return { pos: pos, line: text.slice(0, ls).split('\n').length - 1 }; }
  setCaret(p, info, extra) { var c = Object.assign({}, this.state.caret); c[p] = info; this.setState(Object.assign({ caret: c }, extra || {})); }
  srcInput(e) {
    var S = this.state, p = S.active, f = S.files[p];
    if (!f || f.ro) return;
    var t = e.target.value, pos = e.target.selectionStart, ch = t[pos - 1];
    var comp = ch && /[\w.${:\s\-]/.test(ch) && ch !== '\n' ? this.srcComp(t, pos, false) : null;
    var c = Object.assign({}, S.caret); c[p] = this.caretInfo(t, pos);
    this.applyText(p, t, { comp: comp, caret: c });
  }
  insertSrc(t, from, to, ins, extra) {
    var p = this.state.active, nt = t.slice(0, from) + ins + t.slice(to), pos = from + ins.length;
    this.pendingSel = pos;
    var c = Object.assign({}, this.state.caret); c[p] = this.caretInfo(nt, pos);
    this.applyText(p, nt, Object.assign({ caret: c, comp: null }, extra || {}));
    return { text: nt, pos: pos };
  }
  acceptSrc(i, mouse) {
    var S = this.state, comp = S.comp, p = S.active, f = S.files[p], now = Date.now();
    if (!comp || !f || (mouse && this.lastPick && now - this.lastPick < 250)) return;
    if (mouse) this.lastPick = now;
    var it = comp.items[i], t = f.text, after = t.slice(comp.to);
    var ins = it.insert + (it.close && after[0] !== '}' ? '}' : '');
    var r = this.insertSrc(t, comp.from, comp.to, ins);
    if (it.reopen) { var nc = this.srcComp(r.text, r.pos, true); this.setState({ comp: nc }); }
  }
  srcKey(e) {
    var S = this.state, p = S.active, f = S.files[p], comp = S.comp;
    if (!f) return;
    if (e.key === 's' && (e.metaKey || e.ctrlKey)) { e.preventDefault(); this.save(); return; }
    if (f.ro) return;
    var ta = e.target, t = ta.value, s = ta.selectionStart, en = ta.selectionEnd;
    if (comp && comp.path === p) {
      if (e.key === 'ArrowDown') { e.preventDefault(); this.setState({ comp: Object.assign({}, comp, { sel: (comp.sel + 1) % comp.items.length }) }); return; }
      if (e.key === 'ArrowUp') { e.preventDefault(); this.setState({ comp: Object.assign({}, comp, { sel: (comp.sel - 1 + comp.items.length) % comp.items.length }) }); return; }
      if (e.key === 'Enter' || e.key === 'Tab') { e.preventDefault(); this.acceptSrc(comp.sel); return; }
      if (e.key === 'Escape') { e.preventDefault(); this.setState({ comp: null }); return; }
    }
    if (e.key === ' ' && e.ctrlKey) { e.preventDefault(); this.setState({ comp: this.srcComp(t, s, true) }); return; }
    if (e.key === 'Tab') {
      e.preventDefault();
      var ls = t.lastIndexOf('\n', s - 1) + 1;
      if (e.shiftKey) {
        var lead = /^ {0,2}/.exec(t.slice(ls))[0].length;
        if (lead) { this.pendingSel = Math.max(ls, s - lead); var nt = t.slice(0, ls) + t.slice(ls + lead); var c0 = Object.assign({}, S.caret); c0[p] = this.caretInfo(nt, this.pendingSel); this.applyText(p, nt, { caret: c0, comp: null }); }
        return;
      }
      this.insertSrc(t, s, en, '  ');
      return;
    }
    if (e.key === 'Enter' && !e.shiftKey && !e.metaKey && !e.ctrlKey && s === en && isYamlKind(fileKind(p))) {
      e.preventDefault();
      var ls2 = t.lastIndexOf('\n', s - 1) + 1, line = t.slice(ls2, s), ind = /^\s*/.exec(line)[0];
      if (/^\s*-\s/.test(line)) ind = ind + '  ';
      if (/:\s*$/.test(line)) ind += '  ';
      this.insertSrc(t, s, en, '\n' + ind);
    }
  }
  srcCaret(e) {
    var p = this.state.active, t = e.target.value, info = this.caretInfo(t, e.target.selectionStart), cur = this.state.caret[p];
    var extra = {};
    if (e.type === 'click' || e.type === 'mouseup') extra.comp = null;
    if (e.key && (e.key === 'ArrowLeft' || e.key === 'ArrowRight' || e.key === 'Home' || e.key === 'End')) extra.comp = null;
    if (!cur || cur.line !== info.line || cur.pos !== info.pos || 'comp' in extra) this.setCaret(p, info, extra);
  }
  jump(d) {
    var S = this.state, p = S.active, kind = fileKind(p), v = this.viewOf(p), f = S.files[p], upd = {};
    if (d.path && v !== 'source' && f.model) {
      var key = navKeyFor(kind, d.path, S.sel[p]);
      if (key) { var sel = Object.assign({}, S.sel); sel[p] = key; upd.sel = sel; }
      var ids = [];
      for (var n = d.path.length; n > 0; n--) { var base = fidOf(d.path.slice(0, n)); ids.push(base, base + '-k', base + '-code', base + '-chk', base + '-src'); }
      this.pendingFocus = { ids: ids, noFocus: v === 'split' };
      upd.flash = fidOf(d.path);
      var self = this; setTimeout(function () { if (self.state.flash === upd.flash) self.setState({ flash: null }); }, 1600);
    }
    if (d.line !== null && d.line !== undefined && v !== 'form') {
      var lines = f.text.split('\n'), off = 0;
      for (var i = 0; i < d.line && i < lines.length; i++) off += lines[i].length + 1;
      var lead = lines[d.line] ? /^\s*/.exec(lines[d.line])[0].length : 0;
      this.pendingSel = off + lead; this.pendingScroll = d.line;
      var c = Object.assign({}, S.caret); c[p] = { pos: off + lead, line: d.line }; upd.caret = c;
    }
    upd.diffOn = Object.assign({}, S.diffOn); upd.diffOn[p] = false;
    this.setState(upd);
  }
  viewOf(p) {
    var k = fileKind(p), v = this.state.view[p];
    if (k === 'openapi' || k === 'schema' || k === 'hook') return 'source';
    return v || 'form';
  }
  // ---------- new item dialog ----------
  openNew(kind) {
    var S = this.state, p = S.active, k = p ? fileKind(p) : null, svc = /^services\/([^\/]+)\//.exec(p || '');
    var testFiles = Object.keys(S.files).filter(function (x) { return fileKind(x) === 'test'; });
    this.setState({ menu: null, dlg: { kind: kind || (k === 'api' ? 'api' : 'test'), name: '', id: '', idEdited: false, file: k === 'test' ? p : testFiles[0], service: svc ? svc[1] : 'orders', newService: '', area: '', scenario: '', method: 'GET', apiPath: '/', fromApi: '', hookFile: 'hooks/checks.ts', purpose: '', columns: 'name, value' } });
  }
  dlgSet(patch) { this.setState({ dlg: Object.assign({}, this.state.dlg, patch) }); }
  createNew() {
    var S = this.state, nw = S.dlg, pr = this.project(), plan = planNew(nw, S.files, pr.ctx);
    if (!plan.ok) return;
    var files = Object.assign({}, S.files), open = S.open.slice(), sel = Object.assign({}, S.sel), target = plan.path, toast = null;
    function addFile(p, text) { files[p] = { text: text, original: text, ro: false, model: parseByKind(fileKind(p), text).value }; }
    if (plan.kind === 'test') {
      var test = happyPathTest(plan.id, String(nw.name).trim(), nw.fromApi, pr.ctx);
      if (plan.newFile) { var fidp = plan.fileId; addFile(target, yDump({ id: fidp, name: fidp.split('.').slice(1).join(' ').replace(/^./, function (c) { return c.toUpperCase(); }), tags: [fidp.split('.')[0]], tests: [test] }, 'test')); sel[target] = 'test:0'; toast = 'Created ' + target; }
      else { var P = yParseSafe(files[target].text), m = clone(P.value); if (!Array.isArray(m.tests)) m.tests = []; m.tests.push(test); files[target] = Object.assign({}, files[target], { text: yApplyModel(files[target].text, 'test', m), model: m }); sel[target] = 'test:' + (m.tests.length - 1); toast = 'Added ' + plan.full + ' (unsaved)'; }
    } else if (plan.kind === 'api') {
      var api = { id: plan.id, name: String(nw.name).trim(), method: nw.method, path: nw.apiPath };
      if (api.method === 'GET') delete api.method;
      api.expect = { status: nw.method === 'POST' ? 201 : nw.method === 'DELETE' ? 204 : 200 };
      if (plan.newFile) { var svc = target.split('/')[1]; addFile(target, yDump({ id: svc, name: svc.replace(/^./, function (c) { return c.toUpperCase(); }) + ' service', baseUrl: '${var.' + svc + 'Url}/api/v1', auth: pr.ctx.authIds[0] || 'none', apis: [api] }, 'api')); sel[target] = 'api:0'; toast = 'Created ' + target; }
      else { var P2 = yParseSafe(files[target].text), m2 = clone(P2.value); if (!Array.isArray(m2.apis)) m2.apis = []; m2.apis.push(api); files[target] = Object.assign({}, files[target], { text: yApplyModel(files[target].text, 'api', m2), model: m2 }); sel[target] = 'api:' + (m2.apis.length - 1); toast = 'Added ' + plan.full + ' (unsaved)'; }
    } else if (plan.kind === 'hook' && !plan.newFile) {
      var ht = files[target].text; files[target] = Object.assign({}, files[target], { text: ht.replace(/\n*$/, '\n\n') + hookStub(plan.id), model: null }); toast = 'Added export ' + plan.id + ' (unsaved)';
    } else {
      addFile(target, newFileText(plan, nw, pr.ctx)); toast = 'Created ' + target;
      if (plan.kind === 'load') sel[target] = 'mix';
    }
    if (open.indexOf(target) < 0) open.push(target);
    this.setState({ files: files, open: open, active: target, sel: sel, dlg: null, toast: { msg: toast, sub: plan.newFile ? 'written to disk, like vc new' : 'review it, then Save', kind: 'ok' }, comp: null, fc: null });
  }
  renderVals() { return buildView(this); }
}
function navKeyFor(kind, path, cur) {
  var h = path[0];
  if (kind === 'test') { if (h === 'tests' && typeof path[1] === 'number') return 'test:' + path[1]; if (['beforeAll', 'beforeEach', 'afterEach', 'afterAll'].indexOf(h) >= 0) return 'life'; return 'file'; }
  if (kind === 'api') { if (h === 'apis' && typeof path[1] === 'number') return 'api:' + path[1]; return 'file'; }
  if (kind === 'config') return { syntaxVersion: 'general', project: 'general', paths: 'general', vars: 'profiles', profiles: 'profiles', auth: 'auth', defaults: 'defaults', secrets: 'output', reports: 'output', history: 'output', beforeRun: 'hooks', afterRun: 'hooks' }[h] || cur;
  if (kind === 'load') return { mix: 'mix', stages: 'stages', thresholds: 'thresholds' }[h] || 'scenario';
  return cur;
}
