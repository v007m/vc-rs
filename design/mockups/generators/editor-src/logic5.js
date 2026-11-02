// ===================== form builders (rows the template renders) =====================
var NESTED_M = ['every', 'some', 'none', 'not', 'length'];
function diagAt(B, path) { return B.dmap[pk(path)] || null; }
function subDiag(B, path) {
  var k = pk(path);
  for (var i = 0; i < B.dlist.length; i++) { var d = B.dlist[i]; if (d.path && (pk(d.path) === k || pk(d.path).indexOf(k + PK) === 0)) return d; }
  return null;
}
function worstIn(B, prefixes) {
  var w = null;
  B.dlist.forEach(function (d) {
    if (!d.path) return;
    if (prefixes.some(function (p) { return isPrefix(p, d.path); })) { if (d.sev === 'error') w = 'error'; else if (!w) w = d.sev; }
  });
  return w;
}
function shownVal(B, fid, v, fmt) { if (B.kd[fid] !== undefined) return B.kd[fid]; if (v === undefined || v === null) return ''; return fmt ? fmt(v) : typeof v === 'object' ? flowOf(v) : String(v); }
function compFor(B, fid) {
  if (!B.fc || B.fc.fid !== fid) return { compOpen: false, compItems: [] };
  return { compOpen: true, compItems: B.fc.items.map(function (it, i) {
    return { l: it.label, d: it.detail || '', cls: 'opt' + (i === B.fc.sel ? ' on' : ''), hasM: !!it.method, m: it.method || '', mcls: 'method m-' + String(it.method || '').toLowerCase(),
      pick: function (e) { if (e && e.preventDefault) e.preventDefault(); B.c.acceptField(i, true); } };
  }) };
}
function hcls(d) { return 'help' + (d ? (d.sev === 'error' ? ' err' : ' warnt') : ''); }
function icls(base, d, extra) { return base + (extra ? ' ' + extra : '') + (d ? (d.sev === 'error' ? ' err' : ' warn') : ''); }
function rText(B, lbl, path, o) {
  o = o || {};
  var v = 'value' in o ? o.value : getIn(B.m, path), fid = o.fid || fidOf(path), d = o.noDiag ? null : (o.diag || diagAt(B, path));
  var commit = o.set || function (raw) { var val = o.parse ? o.parse(raw) : raw; B.c.editModel(function (m) { setIn(m, path, (val === '' || val === undefined) && !o.keepEmpty ? undefined : val); }); };
  B.c.fset[fid] = commit;
  return Object.assign({ isText: true, lbl: lbl, val: shownVal(B, fid, v, o.fmt), fid: fid, ph: o.ph || '', type: o.type || 'text',
    cls: 'fld' + (o.span ? ' ' + o.span : '') + (B.flash === fid ? ' flash' : ''), icls: icls('input', d, o.mono === false ? '' : 'mono'),
    help: d ? d.msg : (o.help || ''), hcls: hcls(d), hasHelp: !!(d || o.help), ro: !!o.ro,
    on: function (e) { B.c.fieldInput(fid, e.target.value, e.target.selectionStart, o.comp, path, commit); },
    kd: function (e) { B.c.fieldKey(e, fid); }, bl: function () { B.c.fieldBlur(fid); },
    hasBtn: !!o.btn, btnL: o.btn ? o.btn.l : '', btnOn: o.btn ? o.btn.on : null, btnT: o.btn ? (o.btn.title || o.btn.l) : ''
  }, compFor(B, fid));
}
function optList(opts, sv) {
  var list = opts.map(function (x) { return typeof x === 'string' ? { v: x, l: x } : x; });
  if (sv !== '' && !list.some(function (x) { return x.v === sv; })) list.push({ v: sv, l: sv + ' (not found)' });
  return list.map(function (x) { return { v: x.v, l: x.l, sel: x.v === sv }; });
}
function rSelect(B, lbl, path, opts, o) {
  o = o || {};
  var v = 'value' in o ? o.value : getIn(B.m, path), fid = o.fid || fidOf(path), d = o.noDiag ? null : diagAt(B, path);
  var sv = v === undefined || v === null ? '' : String(v);
  return { isSelect: true, lbl: lbl, fid: fid, val: sv, opts: optList(opts, sv), cls: 'fld' + (o.span ? ' ' + o.span : '') + (B.flash === fid ? ' flash' : ''), icls: icls('select', d),
    help: d ? d.msg : (o.help || ''), hcls: hcls(d), hasHelp: !!(d || o.help),
    on: function (e) { var raw = e.target.value; if (o.set) { o.set(raw); return; } var val = o.parse ? o.parse(raw) : raw; B.c.editModel(function (m) { setIn(m, path, raw === '' ? undefined : val); }); } };
}
function rCheck(B, lbl, chk, on, o) {
  o = o || {};
  return { isCheck: true, lbl: lbl, chk: !!chk, fid: o.fid || ('chk-' + lbl.replace(/\W+/g, '-')), cls: 'fld chk' + (o.span ? ' ' + o.span : ''), help: o.help || '', hasHelp: !!o.help, on: function (e) { on(!!e.target.checked); } };
}
function inCell(B, fid, val, commit, o) {
  o = o || {}; B.c.fset[fid] = commit;
  var d = o.diag || null;
  return Object.assign({ isIn: true, isSel: false, isStatic: false, isRange: false, fid: fid, val: shownVal(B, fid, val), ph: o.ph || '', aria: o.aria || '',
    icls: icls('input mono', d) + (o.err ? ' err' : ''), title: d ? d.msg : (o.title || ''), wcls: 'cell' + (B.flash === fid ? ' flash' : ''),
    on: function (e) { B.c.fieldInput(fid, e.target.value, e.target.selectionStart, o.comp, o.path, commit, o.noDraft); },
    kd: function (e) { B.c.fieldKey(e, fid); }, bl: function () { B.c.fieldBlur(fid); } }, compFor(B, fid));
}
function selCell(B, fid, val, opts, on, o) {
  o = o || {};
  var sv = val === undefined || val === null ? '' : String(val);
  return { isIn: false, isSel: true, isStatic: false, isRange: false, fid: fid, val: sv, opts: optList(opts, sv), aria: o.aria || '', icls: icls('select', o.diag), title: o.diag ? o.diag.msg : '', wcls: 'cell' + (B.flash === fid ? ' flash' : ''), on: function (e) { on(e.target.value); }, compOpen: false, compItems: [] };
}
function stCell(txt, cls) { return { isIn: false, isSel: false, isStatic: true, isRange: false, txt: txt, wcls: 'cell st ' + (cls || ''), compOpen: false, compItems: [] }; }
function rangeCell(fid, val, min, max, on, aria, out) { return { isIn: false, isSel: false, isStatic: false, isRange: true, fid: fid, val: val, min: min, max: max, aria: aria, out: out, wcls: 'cell rng', on: function (e) { on(+e.target.value); }, compOpen: false, compItems: [] }; }
function rTable(lbl, o) {
  // Each cell carries its column heading (shown above the field when a narrow form stacks the table)
  // and the first cell of an item is marked so stacked items get a divider.
  var heads = o.heads || [], ncols = 0;
  o.items.forEach(function (it) { ncols = Math.max(ncols, it.cells.length); it.cells.forEach(function (cl, i) { cl.h = heads[i] && heads[i].t ? heads[i].t : ''; cl.wcls = (cl.wcls || 'cell') + (i === 0 ? ' c0' : '') + (cl.h ? ' hh' : ''); }); });
  return { isTable: true, lbl: lbl, sub: o.sub || '', gcols: o.gcols, heads: o.heads || [], hasHeads: !!(o.heads && o.heads.length), items: o.items, errs: o.errs || [], hasErrs: !!(o.errs && o.errs.length),
    add: o.add, addLbl: o.addLbl || 'Add', hasAdd: !!o.add, empty: !o.items.length, emptyTxt: o.emptyTxt || 'None yet.', cls: 'fld full tbl-f tcols' + Math.min(ncols, 4) + (o.fid && o.flash === o.fid ? ' flash' : ''), fid: o.fid || '' };
}
function uniqueKey(obj, base) { if (!obj || !Object.prototype.hasOwnProperty.call(obj, base)) return base; var n = 2; while (Object.prototype.hasOwnProperty.call(obj, base + n)) n++; return base + n; }
function rKv(B, lbl, path, o) {
  o = o || {};
  var obj = getIn(B.m, path), items = [], errs = [];
  if (!isObj(obj)) obj = {};
  Object.keys(obj).forEach(function (k) {
    var kp = path.concat(k), fidK = fidOf(kp) + '-k', fidV = fidOf(kp), dv = diagAt(B, kp) || subDiag(B, kp);
    var setV = function (raw) { B.c.editModel(function (m) { var cur = getIn(m, path); if (!isObj(cur)) return; cur[k] = o.raw ? raw : parseCell(raw); }); };
    var bad = B.kd[fidK] !== undefined;
    var kc = inCell(B, fidK, k, function (raw) { B.c.renameKeyAt(path, k, raw, fidK); }, { ph: o.kph || 'name', aria: (o.kAria || 'Name') + ' ' + k, err: bad, noDraft: true });
    var vc = inCell(B, fidV, cellText(obj[k]), setV, { ph: o.vph || 'value', aria: 'Value of ' + k, comp: o.comp === undefined ? 'tpl' : o.comp, path: kp, diag: dv });
    items.push({ cells: [kc, vc], del: function () { B.c.editModel(function (m) { setIn(m, kp, undefined); }); }, delAria: 'Remove ' + k });
    if (dv) errs.push({ txt: k + ': ' + dv.msg, cls: hcls(dv) });
    if (bad) errs.push({ txt: '"' + B.kd[fidK] + '" is empty or already used; the name stays ' + k, cls: 'help err' });
  });
  return rTable(lbl, { sub: o.sub, gcols: 'minmax(110px,0.9fr) minmax(0,2fr) 32px', heads: o.heads, items: items, errs: errs, emptyTxt: o.empty || 'None.', addLbl: o.addLbl || 'Add',
    add: o.noAdd ? null : function () { B.c.addKey(path, o.newKey || 'newVar', o.newVal !== undefined ? o.newVal : ''); } });
}
function matchRowsOf(obj) {
  var rows = [];
  Object.keys(obj || {}).forEach(function (p) {
    var v = obj[p];
    if (isObj(v) && Object.keys(v).length && Object.keys(v).every(function (k) { return MATCHERS.indexOf(k) >= 0; })) {
      Object.keys(v).forEach(function (mk) {
        var mv = v[mk];
        if (NESTED_M.indexOf(mk) >= 0 && isObj(mv) && Object.keys(mv).length === 1 && MATCHERS.indexOf(Object.keys(mv)[0]) >= 0) rows.push({ p: p, m: mk, inner: Object.keys(mv)[0], v: mv[Object.keys(mv)[0]] });
        else rows.push({ p: p, m: mk, inner: '', v: mv });
      });
    } else rows.push({ p: p, m: '=', inner: '', v: v });
  });
  return rows;
}
function buildMatchObj(rows) {
  var o = {};
  rows.forEach(function (r) {
    if (r.m === '=') { o[r.p] = r.v; return; }
    if (!isObj(o[r.p]) || Object.keys(o[r.p]).some(function (k) { return MATCHERS.indexOf(k) < 0; })) o[r.p] = {};
    var inner = {}; if (r.inner) inner[r.inner] = r.v;
    o[r.p][r.m] = r.inner ? inner : r.v;
  });
  return o;
}
function matcherValueFor(m, v) {
  if (m === 'exists' || m === 'empty') return typeof v === 'boolean' ? v : true;
  if (m === 'type') return ['string', 'number', 'integer', 'boolean', 'object', 'array', 'null'].indexOf(v) >= 0 ? v : 'string';
  if (m === 'format') return ['email', 'uuid', 'uri', 'date', 'dateTime', 'ipv4'].indexOf(v) >= 0 ? v : 'email';
  if (m === 'between') return Array.isArray(v) && v.length === 2 ? v : [0, typeof v === 'number' ? v : 1];
  if (m === 'in' || m === 'notIn' || m === 'allOf' || m === 'anyOf') return Array.isArray(v) ? v : v === '' || v === undefined ? [] : [v];
  if (typeof v === 'boolean' || Array.isArray(v)) return '';
  return v;
}
function enumFor(m) {
  if (m === 'exists' || m === 'empty') return ['true', 'false'];
  if (m === 'type') return ['string', 'number', 'integer', 'boolean', 'object', 'array', 'null'];
  if (m === 'format') return ['email', 'uuid', 'uri', 'date', 'dateTime', 'ipv4'];
  return null;
}
function parseListCell(raw) { var s = String(raw).trim(); if (s[0] === '[') return parseCell(s); return s === '' ? [] : s.split(',').map(function (x) { return parseCell(x.trim()); }); }
var MATCH_OPTS = [{ v: '=', l: 'equals' }].concat(MATCHERS.map(function (x) { return { v: x, l: x }; }));
function rMatch(B, lbl, base, o) {
  o = o || {};
  var obj;
  if (o.single) { var sv = getIn(B.m, base.concat(o.single)); obj = {}; if (sv !== undefined) obj[o.single] = sv; }
  else { obj = getIn(B.m, base); if (!isObj(obj)) obj = {}; }
  var rows = matchRowsOf(obj), items = [], errs = [], nested = rows.some(function (r) { return NESTED_M.indexOf(r.m) >= 0; });
  function write(nr) {
    var built = buildMatchObj(nr);
    B.c.editModel(function (m) {
      if (o.single) setIn(m, base.concat(o.single), built[o.single]);
      else setIn(m, base, Object.keys(built).length ? built : undefined);
    });
  }
  function upd(i, patch) { write(rows.map(function (r, j) { return j === i ? Object.assign({}, r, patch) : r; })); }
  rows.forEach(function (r, i) {
    var vp = o.single ? base.concat(o.single) : base.concat(r.p), fid = fidOf(vp) + (r.m === '=' ? '' : '-' + r.m), dv = diagAt(B, vp) || subDiag(B, vp);
    var cells = [];
    if (o.single) cells.push(stCell(o.single, 'mono'));
    else if (o.pathOpts) cells.push(selCell(B, fid + '-p', r.p, o.pathOpts, function (val) { upd(i, { p: val }); }, { aria: 'Phase' }));
    else cells.push(inCell(B, fid + '-p', r.p, function (raw) {
      var t = String(raw).trim();
      var clash = rows.some(function (x, j) { return j !== i && x.p === t && (x.m === '=' || r.m === '='); });
      if (!t || clash) { B.c.setDraft(fid + '-p', raw); return; }
      B.c.setDraft(fid + '-p', undefined); upd(i, { p: t });
    }, { ph: o.pph || 'path', aria: 'Path', err: B.kd[fid + '-p'] !== undefined && (!String(B.kd[fid + '-p']).trim() || rows.some(function (x, j) { return j !== i && x.p === String(B.kd[fid + '-p']).trim(); })), noDraft: true }));
    cells.push(selCell(B, fid + '-m', r.m, o.mopts || MATCH_OPTS, function (val) { upd(i, { m: val, inner: NESTED_M.indexOf(val) >= 0 ? r.inner : '', v: val === '=' ? (isObj(r.v) || Array.isArray(r.v) ? '' : r.v) : matcherValueFor(val, r.v) }); }, { aria: 'Matcher for ' + r.p }));
    if (nested) {
      if (NESTED_M.indexOf(r.m) >= 0) cells.push(selCell(B, fid + '-i', r.inner, [{ v: '', l: 'value' }].concat(MATCHERS.map(function (x) { return { v: x, l: x }; })), function (val) { upd(i, { inner: val, v: val ? matcherValueFor(val, r.v) : r.v }); }, { aria: 'Inner matcher' }));
      else cells.push(stCell('', ''));
    }
    var em = enumFor(r.inner || r.m), listy = ['between', 'in', 'notIn', 'allOf', 'anyOf'].indexOf(r.inner || r.m) >= 0;
    if (em) cells.push(selCell(B, fid, String(r.v), em, function (val) { upd(i, { v: val === 'true' ? true : val === 'false' ? false : val }); }, { aria: 'Value for ' + r.p, diag: dv }));
    else cells.push(inCell(B, fid, cellText(r.v), function (raw) { upd(i, { v: listy ? parseListCell(raw) : parseCell(raw) }); }, { ph: listy ? 'a, b' : 'value', aria: 'Value for ' + r.p, comp: 'tpl', path: vp, diag: dv }));
    items.push({ cells: cells, del: function () { write(rows.filter(function (x, j) { return j !== i; })); }, delAria: 'Remove check on ' + r.p });
    if (dv) errs.push({ txt: r.p + ': ' + dv.msg, cls: hcls(dv) });
  });
  var heads = o.single ? [] : [{ t: o.pathOpts ? 'Phase' : (o.pathHead || 'Path') }, { t: 'Matcher' }].concat(nested ? [{ t: 'Each' }] : []).concat([{ t: 'Value' }, { t: '' }]);
  var gcols = (o.single ? '70px ' : 'minmax(110px,1.2fr) ') + '120px ' + (nested ? '104px ' : '') + 'minmax(0,1.4fr) 32px';
  return rTable(lbl, { sub: o.sub, gcols: gcols, heads: heads.length && items.length ? heads : [], items: items, errs: errs, emptyTxt: o.empty || 'No checks.', addLbl: o.addLbl || 'Add check',
    add: o.single && rows.length ? null : function () {
      var np = o.single || (o.pathOpts ? (o.pathOpts.filter(function (x) { return !Object.prototype.hasOwnProperty.call(obj, x); })[0] || o.pathOpts[0]) : uniqueKey(obj, o.newPath || 'field'));
      var nr = rows.concat([{ p: np, m: o.newM || 'exists', inner: '', v: o.newV !== undefined ? o.newV : true }]);
      write(nr); B.c.focusLater(fidOf(o.single ? base.concat(o.single) : base.concat(np)) + '-' + (o.newM || 'exists') + (o.single || o.pathOpts ? '' : '-p'));
    } });
}
function rSeg(lbl, opts) { return { isSeg: true, lbl: lbl, cls: 'fld full', opts: opts.map(function (x) { return { l: x.l, cls: x.sel ? 'on' : '', pressed: x.sel ? 'true' : 'false', on: x.on, title: x.title || x.l, dis: !!x.dis }; }) }; }
function rCode(B, lbl, path, o) {
  o = o || {};
  var v = getIn(B.m, path), fid = o.fid || fidOf(path) + '-code', shown = B.kd[fid] !== undefined ? B.kd[fid] : (o.fmt ? o.fmt(v) : (v === undefined ? '' : String(v)));
  var err = '';
  if (o.json && B.kd[fid] !== undefined) { var jr = jsonParse(B.kd[fid]); if (!jr.ok) err = 'Not valid JSON yet (line ' + jr.error.line + '): ' + jr.error.msg + '. The YAML keeps the last valid body.'; }
  var commit = function (raw) {
    if (o.json) { var r = jsonParse(raw); if (!r.ok) return; B.c.editModel(function (m) { setIn(m, path, r.value); }); }
    else B.c.editModel(function (m) { setIn(m, path, raw === '' ? undefined : raw); });
  };
  B.c.fset[fid] = commit;
  return { isCode: true, lbl: lbl, fid: fid, val: shown, rows: o.rows || 6, cls: 'fld full', icls: 'textarea mono' + (err ? ' err' : ''), help: err || o.help || '', hcls: err ? 'help err' : 'help', hasHelp: !!(err || o.help),
    on: function (e) { B.c.fieldInput(fid, e.target.value, e.target.selectionStart, null, path, commit); }, bl: function () { B.c.fieldBlur(fid); } };
}
function rAdds(btns) { var b = btns.filter(Boolean); return b.length ? { isAdds: true, cls: 'fld full', btns: b.map(function (x) { return { l: x.l, on: x.on, title: x.title || x.l }; }) } : null; }
function rNote(txt, cls) { return { isNote: true, txt: txt, cls: 'fld full note ' + (cls || '') }; }
function rChips(lbl, chips) { return { isChips: true, lbl: lbl, cls: 'fld full', chips: chips }; }
function btn(kind, on, aria, dis) { return { isUp: kind === 'up', isDown: kind === 'down', isDup: kind === 'dup', isDel: kind === 'del', isTxt: kind === 'txt', l: aria, on: on, aria: aria, dis: !!dis }; }
var ROW_FLAGS = ['isText', 'isSelect', 'isCheck', 'isTable', 'isGrid', 'isSeg', 'isCode', 'isAdds', 'isChips', 'isNote', 'isRamp', 'compOpen', 'hasBtn', 'hasHelp'];
function sec(title, o, rows) {
  o = o || {};
  var r = rows.filter(Boolean);
  r.forEach(function (x) { ROW_FLAGS.forEach(function (f) { if (x[f] === undefined) x[f] = false; }); if (x.compItems === undefined) x.compItems = []; });
  return { title: title || '', hasHead: !!(title || (o.btns && o.btns.length)), sub: o.sub || '', hasSub: !!o.sub, pill: o.pill || '', pillCls: 'pill ' + (o.pillCls || ''), hasPill: !!o.pill,
    cls: 'fsec ' + (o.lvl ? 'l1' : 'l0') + (o.flash ? ' flash' : ''), fid: o.fid || '', btns: o.btns || [], hasBtns: !!(o.btns && o.btns.length), rows: r, mono: o.mono ? 'h3 mono' : 'h3' };
}
function moveBtns(B, arrPath, i, n, label, onDup) {
  return [
    btn('up', function () { B.c.moveItem(arrPath, i, -1); }, 'Move ' + label + ' up', i === 0),
    btn('down', function () { B.c.moveItem(arrPath, i, 1); }, 'Move ' + label + ' down', i === n - 1),
    btn('dup', onDup || function () { B.c.dupItem(arrPath, i); }, 'Duplicate ' + label),
    btn('del', function () { B.c.delItem(arrPath, i); }, 'Delete ' + label)
  ];
}
function apiHelp(B, id) {
  var a = B.ctx.apiById[id];
  return a ? a.method + ' ' + a.path + ' · ' + a.file + (a.saves.length ? ' · saves ' + a.saves.join(', ') : '') : '';
}
function hookOpts(B) { return hookItems(B.ctx, B.fpath).map(function (h) { return { v: h.insert, l: h.label + ' · ' + h.detail }; }); }
function schemaOpts(B) {
  var o = [{ v: '', l: '(no schema check)' }];
  B.ctx.schemas.forEach(function (s) { o.push({ v: relPath(B.fpath, s), l: s }); });
  B.ctx.snapshots.forEach(function (s) { s.schemas.forEach(function (n) { o.push({ v: relPath(B.fpath, s.path) + '#/components/schemas/' + n, l: s.path + ' › ' + n + ' (snapshot)' }); }); });
  return o;
}
function expectRows(B, E, o) {
  o = o || {};
  var e = getIn(B.m, E); if (!isObj(e)) e = {};
  var rows = [], body = isObj(e.body) ? e.body : {};
  rows.push(rText(B, 'Status', E.concat('status'), { fmt: fmtStatus, parse: parseStatusInput, ph: o.statusPh || 'from the api', help: o.statusHelp || '200, a list like 200, 204, or a class like 2xx' }));
  var sc = isObj(body.schema) ? body.schema.$file : undefined;
  rows.push(rSelect(B, 'Body schema', E.concat('body', 'schema'), schemaOpts(B), { value: sc === undefined ? '' : sc, span: 'w2', set: function (v) { B.c.editModel(function (m) { setIn(m, E.concat('body', 'schema'), v ? { $file: v } : undefined); }); }, help: o.compact ? '' : 'JSON Schema files under schemas/, or a schema from an OpenAPI snapshot' }));
  if (!o.compact || body.match !== undefined) rows.push(rMatch(B, 'Body match', E.concat('body', 'match'), { sub: 'paths like items[*].price; a literal means equals' }));
  if (e.headers !== undefined) rows.push(rMatch(B, 'Headers', E.concat('headers'), { pathHead: 'Header', newPath: 'content-type', newM: 'contains', newV: 'json', pph: 'header name' }));
  if (e.timing !== undefined) rows.push(rMatch(B, 'Timing', E.concat('timing'), { pathOpts: TIMING, newM: 'lt', newV: '500ms', mopts: ['lt', 'lte', 'gt', 'gte', 'between'].map(function (x) { return { v: x, l: x }; }) }));
  if (e.size !== undefined) rows.push(rMatch(B, 'Body size', E, { single: 'size', newM: 'lt', newV: '100kb' }));
  if (e.cel !== undefined) rows.push(rKv(B, 'CEL checks', E.concat('cel'), { raw: true, comp: null, kph: 'label', vph: 'expression that must be true', newKey: 'check', newVal: 'size(body.items) > 0', addLbl: 'Add CEL check', kAria: 'Label' }));
  if (e.custom !== undefined) rows.push(customTable(B, E.concat('custom')));
  if (body.equals !== undefined) rows.push(rCode(B, 'Body equals (whole body, JSON)', E.concat('body', 'equals'), { json: true, fmt: function (v) { return JSON.stringify(v, null, 2); }, rows: 5 }));
  var adds = [];
  if (o.compact && body.match === undefined) adds.push({ l: 'Body match', on: function () { B.c.editModel(function (m) { setIn(m, E.concat('body', 'match'), { field: { exists: true } }); }); } });
  if (e.headers === undefined) adds.push({ l: 'Header check', on: function () { B.c.editModel(function (m) { setIn(m, E.concat('headers'), { 'content-type': { contains: 'json' } }); }); } });
  if (e.timing === undefined) adds.push({ l: 'Timing', on: function () { B.c.editModel(function (m) { setIn(m, E.concat('timing'), { total: { lt: '500ms' } }); }); } });
  if (e.cel === undefined) adds.push({ l: 'CEL check', on: function () { B.c.editModel(function (m) { setIn(m, E.concat('cel'), { check: 'size(body.items) > 0' }); }); B.c.focusLater(fidOf(E.concat('cel', 'check')) + '-k'); } });
  if (e.custom === undefined && B.ctx.hooks.length) adds.push({ l: 'Custom function', on: function () { B.c.editModel(function (m) { setIn(m, E.concat('custom'), [{ 'function': hookOpts(B)[0].v }]); }); } });
  if (e.size === undefined && !o.compact) adds.push({ l: 'Body size', on: function () { B.c.editModel(function (m) { setIn(m, E.concat('size'), { lt: '100kb' }); }); } });
  if (body.equals === undefined && !o.compact) adds.push({ l: 'Whole body equals', on: function () { B.c.editModel(function (m) { setIn(m, E.concat('body', 'equals'), { ok: true }); }); } });
  rows.push(rAdds(adds));
  return rows;
}
function customTable(B, path) {
  var arr = getIn(B.m, path), items = [];
  (Array.isArray(arr) ? arr : []).forEach(function (c, i) {
    var cp = path.concat(i, 'function'), d = diagAt(B, cp);
    items.push({ cells: [selCell(B, fidOf(cp), isObj(c) ? c['function'] : '', hookOpts(B), function (v) { B.c.editModel(function (m) { setIn(m, cp, v); }); }, { aria: 'Function', diag: d })],
      del: function () { B.c.editModel(function (m) { setIn(m, path.concat(i), undefined); }); }, delAria: 'Remove function' });
  });
  return rTable('Custom functions', { sub: 'exports from hooks/', gcols: 'minmax(0,1fr) 32px', items: items, addLbl: 'Add function',
    add: function () { B.c.editModel(function (m) { var a = getIn(m, path); if (!Array.isArray(a)) { a = []; setIn(m, path, a); } a.push({ 'function': hookOpts(B)[0].v }); }); } });
}
function authRows(B, path, o) {
  o = o || {};
  var v = getIn(B.m, path), id = isObj(v) ? v.id : v, opts = [{ v: '', l: o.inheritLbl || 'inherit from the api' }, { v: 'none', l: 'none: send no credentials' }].concat(B.ctx.authDefs.map(function (a) { return { v: a.id, l: a.id + ' · ' + a.type }; }));
  var rows = [rSelect(B, 'Auth', path, opts, { value: id === undefined ? '' : id, fid: fidOf(isObj(v) ? path.concat('id') : path), set: function (nv) {
    B.c.editModel(function (m) { var cur = getIn(m, path); if (!nv) setIn(m, path, undefined); else if (isObj(cur)) cur.id = nv; else setIn(m, path, nv); });
  }, help: o.help || '' })];
  if (isObj(v)) rows.push(rKv(B, 'Credentials for ' + (v.id || 'auth'), path.concat('vars'), { addLbl: 'Add value', newKey: 'username' }));
  else if (id && id !== 'none') rows.push(rAdds([{ l: 'Use different credentials', title: 'Turns auth into an id with vars, for example another user', on: function () { B.c.editModel(function (m) { setIn(m, path, { id: id, vars: { username: '${secret.USER}' } }); }); } }]));
  return rows;
}
function bodyRows(B, path, inherit) {
  var v = getIn(B.m, path), fid = fidOf(path), mode = B.bodyMode[fid] || (v === undefined ? 'inherit' : typeof v === 'string' ? 'text' : isObj(v) && v.$file !== undefined ? 'file' : 'json');
  if (v === undefined && mode !== 'inherit') mode = 'inherit';
  function to(md) {
    return function () {
      B.c.setBodyMode(fid, md);
      B.c.editModel(function (m) {
        var cur = getIn(m, path);
        if (md === 'inherit') setIn(m, path, undefined);
        else if (md === 'text') setIn(m, path, typeof cur === 'string' ? cur : isObj(cur) && cur.$file === undefined ? JSON.stringify(cur) : '');
        else if (md === 'file') setIn(m, path, { $file: isObj(cur) && cur.$file ? cur.$file : '../../datasets/' });
        else setIn(m, path, isObj(cur) && cur.$file === undefined ? cur : { key: 'value' });
      });
    };
  }
  var modes = (inherit ? [{ l: 'Inherit', k: 'inherit' }] : [{ l: 'None', k: 'inherit' }]).concat([{ l: 'JSON', k: 'json' }, { l: 'Form fields', k: 'form' }, { l: 'Text', k: 'text' }, { l: 'File', k: 'file' }]);
  var rows = [rSeg('Body', modes.map(function (x) { return { l: x.l, sel: mode === x.k, on: to(x.k), title: x.k === 'inherit' && inherit ? 'Use the api\'s body' : 'Body as ' + x.l }; }))];
  if (mode === 'json') rows.push(rCode(B, 'JSON body', path, { json: true, fmt: function (x) { return JSON.stringify(x, null, 2); }, rows: Math.min(12, Math.max(4, JSON.stringify(v || {}, null, 2).split('\n').length + 1)), help: 'Templates like ${var.cartId} work inside strings' }));
  if (mode === 'form') rows.push(rKv(B, 'Fields', path, { addLbl: 'Add field', newKey: 'field' }));
  if (mode === 'text') rows.push(rCode(B, 'Text body', path, { rows: 4 }));
  if (mode === 'file') rows.push(rText(B, 'File', path.concat('$file'), { span: 'w2', help: 'Path relative to this file' }));
  if (mode === 'inherit' && inherit) rows.push(rNote(inherit, 'muted'));
  return rows;
}
function actionSec(B, arrPath, i, n, label, o) {
  o = o || {};
  var path = arrPath.concat(i), a = getIn(B.m, path), kind = actionKind(a), secs = [];
  if (!isObj(a)) a = {};
  var title = label + ' ' + (i + 1), sub = kind + ' ' + (actionTarget(a) || '…') + (a.id ? ' · id ' + a.id : '');
  var rows = [];
  rows.push(rSelect(B, 'Action', path.concat('__kind'), [{ v: 'call', l: 'call an api' }, { v: 'test', l: 'reuse a test' }, { v: 'function', l: 'run a function' }, { v: 'command', l: 'run a command' }], { value: kind, noDiag: true, set: function (nk) {
    B.c.editModel(function (m) { var x = getIn(m, path), val = actionTarget(x); ['call', 'test', 'function', 'command'].forEach(function (k) { delete x[k]; }); var y = {}; if (x.id !== undefined) y.id = x.id; y[nk] = nk === kind ? val : (nk === 'call' ? '' : nk === 'test' ? '' : nk === 'function' ? hookOpts(B)[0].v : 'echo ready'); Object.keys(x).forEach(function (k) { if (k !== 'id') y[k] = x[k]; }); setIn(m, path, y); });
  } }));
  if (kind === 'call') rows.push(rText(B, 'Calls api', path.concat('call'), { comp: 'call', span: 'w2', ph: 'service.apiId', help: apiHelp(B, a.call) || 'Type to search apis by id, method or path', keepEmpty: true }));
  if (kind === 'test') rows.push(rText(B, 'Reuses test', path.concat('test'), { comp: 'test', span: 'w2', ph: 'file.testId', keepEmpty: true, help: 'Runs that test\'s calls in order; its own expect is not checked' }));
  if (kind === 'function') rows.push(rText(B, 'Function', path.concat('function'), { comp: 'fn', span: 'w2', ph: 'hooks/file.ts#export', keepEmpty: true }));
  if (kind === 'command') rows.push(rText(B, 'Command', path.concat('command'), { span: 'w2', ph: 'docker compose up -d --wait', keepEmpty: true, help: 'A non-zero exit fails the action' }));
  rows.push(rText(B, 'Id', path.concat('id'), { ph: o.shared ? 'needed to share values' : 'optional', help: o.shared ? (a.id ? 'Shared as ${var.' + a.id + '.*}' : 'Give it an id so tests can read what it saves') : (o.isStep ? 'Read its response as ${' + (a.id || 'id') + '.body…}' : '') }));
  if (a.forEach !== undefined) rows.push(rText(B, 'forEach', path.concat('forEach'), { comp: 'tpl', span: 'w2', fmt: function (v) { return isObj(v) || Array.isArray(v) ? flowOf(v) : String(v); }, parse: parseCell, help: 'A list, a list-valued template or {$file: …}; ${item} is the element' }));
  if (a.repeat !== undefined) rows.push(rText(B, 'repeat', path.concat('repeat'), { type: 'number', parse: function (r) { return r === '' ? undefined : +r; }, help: '${iteration.index} counts from 1' }));
  if (a.parallel !== undefined) rows.push(rText(B, 'parallel', path.concat('parallel'), { type: 'number', parse: function (r) { return r === '' ? undefined : +r; }, help: 'Iterations at a time' }));
  if (kind === 'call' || kind === 'test' || a.vars !== undefined) rows.push(rKv(B, 'Vars', path.concat('vars'), { empty: kind === 'call' ? 'No vars; the api\'s defaults apply.' : 'No vars.', addLbl: 'Add var' }));
  if (isObj(a.cache)) {
    rows.push(rText(B, 'Cache for', path.concat('cache', 'for'), { ph: '12h', help: 'Reuse the result across runs, per profile' }));
    var ck = a.cache.check;
    if (isObj(ck)) { rows.push(rText(B, 'Cache check calls', path.concat('cache', 'check', 'call'), { comp: 'call', span: 'w2', help: apiHelp(B, ck.call) || 'A quick call that confirms the cached data still exists' })); rows.push(rKv(B, 'Check vars', path.concat('cache', 'check', 'vars'), { addLbl: 'Add var' })); }
    else rows.push(rAdds([{ l: 'Cache check', on: function () { B.c.editModel(function (m) { setIn(m, path.concat('cache', 'check'), { call: '' }); }); } }]));
  }
  if (a.auth !== undefined) authRows(B, path.concat('auth')).forEach(function (r) { rows.push(r); });
  if (a.body !== undefined) bodyRows(B, path.concat('body'), 'Uses the api\'s body').forEach(function (r) { rows.push(r); });
  if (a.expect !== undefined) { rows.push(rNote('Expect', 'sub')); expectRows(B, path.concat('expect'), { compact: true }).forEach(function (r) { rows.push(r); }); }
  if (a.save !== undefined) rows.push(rKv(B, 'Save', path.concat('save'), { addLbl: 'Add value', newKey: 'value', newVal: '${response.body.id}', vph: '${response.body.id}' }));
  if (a.poll !== undefined) pollRows(B, path.concat('poll')).forEach(function (r) { rows.push(r); });
  if (a.retry !== undefined) { rows.push(rText(B, 'Retry attempts', path.concat('retry', 'attempts'), { type: 'number', parse: function (r) { return r === '' ? undefined : +r; } })); rows.push(rText(B, 'Retry delay', path.concat('retry', 'delay'), { ph: '1s' })); }
  if (a.timeout !== undefined) rows.push(rText(B, 'Timeout', path.concat('timeout'), { ph: '30s' }));
  var adds = [];
  function addKey(k, v, l) { if (a[k] === undefined) adds.push({ l: l || k, on: function () { B.c.editModel(function (m) { setIn(m, path.concat(k), clone(v)); }); } }); }
  if (kind === 'call' || kind === 'test') {
    if (o.isStep || o.perTest) addKey('expect', { status: 200 }, 'expect');
    addKey('save', { value: '${response.body.id}' }, 'save');
    if (o.isStep) addKey('poll', { until: { body: { match: { status: 'done' } } }, every: '1s', timeout: '30s' }, 'poll');
    addKey('forEach', '${var.items}', 'forEach');
    addKey('repeat', 2, 'repeat');
    if (a.forEach !== undefined || a.repeat !== undefined) addKey('parallel', 2, 'parallel');
    if (o.shared) addKey('cache', { 'for': '12h' }, 'cache');
    addKey('retry', { attempts: 2, delay: '1s' }, 'retry');
    addKey('auth', 'none', 'auth');
    if (kind === 'call') addKey('body', { key: 'value' }, 'body');
  } else { if (a.vars === undefined) addKey('vars', { name: 'value' }, 'vars'); addKey('timeout', '30s', 'timeout'); }
  if (adds.length) rows.push(rAdds(adds.map(function (x) { x.l = '+ ' + x.l; return x; })));
  secs.push(sec(title, { lvl: 1, sub: sub, fid: fidOf(path), btns: moveBtns(B, arrPath, i, n, label.toLowerCase() + ' ' + (i + 1)), flash: B.flash === fidOf(path) }, rows));
  return secs;
}
function pollRows(B, P) {
  var rows = [rNote('Poll: re-send until the checks hold', 'sub')];
  rows.push(rMatch(B, 'Until body matches', P.concat('until', 'body', 'match'), { sub: 'checked on each attempt' }));
  rows.push(rText(B, 'Every', P.concat('every'), { ph: '1s' }));
  rows.push(rText(B, 'Timeout', P.concat('timeout'), { ph: '30s' }));
  rows.push(rAdds([{ l: 'Remove poll', on: function () { B.c.editModel(function (m) { setIn(m, P, undefined); }); } }]));
  return rows;
}
function lifecycleSecs(B, blocks, o) {
  o = o || {};
  var secs = [];
  var desc = { beforeAll: 'Once for the file, before its first test. Give actions an id to share what they save.', beforeEach: 'Around every test instance; each test gets its own data.', afterEach: 'After every test instance, even when it failed.', afterAll: 'Once, after the file\'s last test.', beforeRun: 'Once per run; value-producing actions run when a test first needs them.', afterRun: 'Once, at the end of the run.', before: 'Before this test, inside beforeEach.', after: 'After this test, inside afterEach.' };
  blocks.forEach(function (b) {
    var arrPath = (o.base || []).concat(b), arr = getIn(B.m, arrPath), n = Array.isArray(arr) ? arr.length : 0;
    var shared = b === 'beforeAll' || b === 'beforeRun';
    secs.push(sec(b, { sub: desc[b], pill: n + (n === 1 ? ' action' : ' actions'), pillCls: n ? 'accent' : '', fid: fidOf(arrPath) }, [
      rAdds([{ l: '+ Add ' + b + ' action', on: function () { B.c.editModel(function (m) { var a = getIn(m, arrPath); if (!Array.isArray(a)) { a = []; setIn(m, arrPath, a); } a.push(shared ? { id: uniqueActionId(a), call: '' } : { call: '' }); }); B.c.focusLater(fidOf(arrPath.concat(n, 'call'))); } }])
    ]));
    for (var i = 0; i < n; i++) actionSec(B, arrPath, i, n, b, { shared: shared, perTest: !shared }).forEach(function (s) { secs.push(s); });
  });
  return secs;
}
function uniqueActionId(arr) { var ids = arr.filter(isObj).map(function (a) { return a.id; }); return uniqueId('data', ids).replace(/^data2$/, ids.indexOf('data') < 0 ? 'data' : 'data2'); }
function profileOverrideSecs(B, base) {
  var prs = getIn(B.m, base), secs = [], ids = B.ctx.profiles.map(function (p) { return p.id; });
  (Array.isArray(prs) ? prs : []).forEach(function (p, i) {
    var pp = base.concat(i);
    secs.push(sec('Profile ' + (isObj(p) ? p.id : ''), { lvl: 1, sub: 'overrides vars when this profile is active', fid: fidOf(pp), btns: [btn('del', function () { B.c.delItem(base, i); }, 'Remove profile override')] }, [
      rSelect(B, 'Profile', pp.concat('id'), ids),
      rKv(B, 'Vars', pp.concat('vars'), { addLbl: 'Add override' })
    ]));
  });
  var used = (Array.isArray(prs) ? prs : []).map(function (p) { return isObj(p) && p.id; }), free = ids.filter(function (x) { return used.indexOf(x) < 0; });
  secs.push(sec('', {}, [rAdds(free.map(function (id) { return { l: '+ Override for ' + id, on: function () { B.c.editModel(function (m) { var a = getIn(m, base); if (!Array.isArray(a)) { a = []; setIn(m, base, a); } a.push({ id: id, vars: { name: 'value' } }); }); } }; }))]));
  return secs;
}
function skipRows(B, path, what) {
  var s = getIn(B.m, path), rows = [];
  rows.push(rCheck(B, 'Skip this ' + what, !!s, function (on) { B.c.editModel(function (m) { setIn(m, path, on ? { reason: '' } : undefined); }); B.c.focusLater(fidOf(path.concat('reason'))); }, { fid: fidOf(path) + '-chk', help: s ? 'Skipped tests are reported with the reason' : '' }));
  if (typeof s === 'string') rows.push(rText(B, 'Skip reason', path, { mono: false, span: 'w2', keepEmpty: true }));
  else if (isObj(s)) {
    rows.push(rText(B, 'Skip reason', path.concat('reason'), { mono: false, span: 'w2', keepEmpty: true, ph: 'why, with an issue link', diag: diagAt(B, path.concat('reason')) }));
    rows.push(rText(B, 'Skip until', path.concat('until'), { type: 'date', help: 'Optional; vc check warns after this date' }));
  }
  return rows;
}
function navItem(B, key, label, sub, prefixes, o) {
  o = o || {};
  var w = worstIn(B, prefixes), sel = B.sel === key;
  return { l: label, sub: sub, cls: 'fnav-i' + (sel ? ' on' : ''), on: function () { B.c.setSel(key); }, hasDot: !!w, dotCls: 'dot ' + (w === 'error' ? 'fail' : 'block'), dotLbl: w === 'error' ? 'has errors' : 'has warnings',
    hasPill: !!o.pill, pill: o.pill || '', pillCls: 'pill ' + (o.pillCls || ''), hasM: !!o.method, m: o.method || '', mcls: 'method m-' + String(o.method || '').toLowerCase(), subCls: o.monoSub ? 'fnav-s mono' : 'fnav-s', btns: sel && o.btns ? o.btns : [], hasBtns: !!(sel && o.btns) };
}
function buildTestForm(B) {
  var m = B.m, tests = Array.isArray(m.tests) ? m.tests : [], nav = [], secs = [];
  if (!B.sel || (/^test:/.test(B.sel) && !tests[+B.sel.split(':')[1]])) B.sel = tests.length ? 'test:0' : 'file';
  var lc = ['beforeAll', 'beforeEach', 'afterEach', 'afterAll'].map(function (b) { return Array.isArray(m[b]) ? m[b].length : 0; });
  nav.push(navItem(B, 'file', 'File settings', (m.id || 'no id') + ' · vars, profiles', [['id'], ['name'], ['tags'], ['skip'], ['vars'], ['profiles']]));
  nav.push(navItem(B, 'life', 'Lifecycle', 'beforeAll ' + lc[0] + ' · beforeEach ' + lc[1] + ' · afterEach ' + lc[2] + ' · afterAll ' + lc[3], [['beforeAll'], ['beforeEach'], ['afterEach'], ['afterAll']]));
  tests.forEach(function (t, i) {
    if (!isObj(t)) t = {};
    var skip = t.skip ? 'skipped' : '';
    nav.push(navItem(B, 'test:' + i, t.name || t.id || 'Untitled test', t.id || 'no id', [['tests', i]], { monoSub: true, pill: skip, pillCls: 'skip', btns: moveBtns(B, ['tests'], i, tests.length, 'test') }));
  });
  var fileId = m.id || '';
  if (B.sel === 'file') {
    secs.push(sec('File settings', { sub: 'Applies to every test in ' + B.fpath }, [
      rText(B, 'File id', ['id'], { help: 'services/<service>/<area>.test.yaml has the id <service>.<area>' }),
      rText(B, 'Name', ['name'], { mono: false, span: 'w2' }),
      rText(B, 'Tags', ['tags'], { mono: false, span: 'w2', fmt: function (v) { return Array.isArray(v) ? v.join(', ') : String(v); }, parse: tagsParse, help: 'Added to every test; lowercase, comma separated' })
    ].concat(skipRows(B, ['skip'], 'file'))));
    secs.push(sec('Vars', { sub: 'File-level values; tests, steps and actions can override them' }, [rKv(B, 'File vars', ['vars'], { addLbl: 'Add var' })]));
    secs.push(sec('Profile overrides', { sub: 'Per-profile values for this file\'s vars; the profile must exist in vibranium.config.yaml' }, []));
    profileOverrideSecs(B, ['profiles']).forEach(function (s) { secs.push(s); });
  } else if (B.sel === 'life') {
    lifecycleSecs(B, ['beforeAll', 'beforeEach', 'afterEach', 'afterAll']).forEach(function (s) { secs.push(s); });
  } else {
    var i = +B.sel.split(':')[1], tp = ['tests', i], t = isObj(tests[i]) ? tests[i] : {}, derived = camelOf(t.name || '');
    secs.push(sec(t.name || t.id || 'Test', { sub: fileId + '.' + (t.id || '?'), fid: fidOf(tp), mono: false }, [
      rText(B, 'Name', tp.concat('name'), { mono: false, span: 'w2', ph: 'A sentence: what the system does', help: 'The id follows the name until you change the id', set: function (raw) {
        B.c.editModel(function (mm) { var tt = getIn(mm, tp), follow = !tt.id || tt.id === camelOf(tt.name || ''); if (raw === '') delete tt.name; else tt.name = raw; if (follow && camelOf(raw)) tt.id = camelOf(raw); });
      } }),
      rText(B, 'Id', tp.concat('id'), { help: 'camelCase behaviour, third person: rejectsDeclinedCard', btn: derived && derived !== t.id ? { l: 'Use ' + derived, title: 'Rename the id to ' + derived + ', derived from the name', on: function () { B.c.applyFixObj({ type: 'rename', path: tp.concat('id'), from: t.id, to: derived }); } } : null }),
      rText(B, 'Tags', tp.concat('tags'), { mono: false, fmt: function (v) { return Array.isArray(v) ? v.join(', ') : String(v); }, parse: tagsParse, help: 'Added to the file\'s tags' }),
      rCheck(B, 'Runs on its own', t.standalone !== false, function (on) { B.c.editModel(function (mm) { setIn(mm, tp.concat('standalone'), on ? undefined : false); }); }, { help: 'Off: runs only when reused with test:', fid: fidOf(tp.concat('standalone')) })
    ].concat(skipRows(B, tp.concat('skip'), 'test'))));
    // before / after
    var nb = Array.isArray(t.before) ? t.before.length : 0, na = Array.isArray(t.after) ? t.after.length : 0;
    secs.push(sec('Before and after', { sub: 'Run around this test only, inside beforeEach and afterEach', pill: (nb + na) ? nb + ' before · ' + na + ' after' : '', pillCls: 'accent' }, [rAdds([
      { l: '+ Before action', on: function () { B.c.editModel(function (mm) { var a = getIn(mm, tp.concat('before')); if (!Array.isArray(a)) { a = []; setIn(mm, tp.concat('before'), a); } a.push({ call: '' }); }); B.c.focusLater(fidOf(tp.concat('before', nb, 'call'))); } },
      { l: '+ After action', on: function () { B.c.editModel(function (mm) { var a = getIn(mm, tp.concat('after')); if (!Array.isArray(a)) { a = []; setIn(mm, tp.concat('after'), a); } a.push({ call: '' }); }); B.c.focusLater(fidOf(tp.concat('after', na, 'call'))); } }
    ])]));
    for (var b = 0; b < nb; b++) actionSec(B, tp.concat('before'), b, nb, 'Before', { perTest: true }).forEach(function (s) { secs.push(s); });
    for (var a2 = 0; a2 < na; a2++) actionSec(B, tp.concat('after'), a2, na, 'After', { perTest: true }).forEach(function (s) { secs.push(s); });
    // request
    var isSteps = t.steps !== undefined, req = [];
    req.push(rSeg('Request', [
      { l: 'One call', sel: !isSteps, on: function () { B.c.editModel(function (mm) { var tt = getIn(mm, tp); if (tt.steps === undefined) return; var first = Array.isArray(tt.steps) && tt.steps.filter(function (s) { return isObj(s) && s.call; })[0]; delete tt.steps; tt.call = first ? first.call : ''; }); } },
      { l: 'Steps', sel: isSteps, on: function () { B.c.editModel(function (mm) { var tt = getIn(mm, tp); if (tt.steps !== undefined) return; var c = tt.call; delete tt.call; tt.steps = [{ id: 'first', call: c || '' }]; }); } }
    ]));
    if (!isSteps) {
      req.push(rText(B, 'Calls api', tp.concat('call'), { comp: 'call', span: 'w2', ph: 'service.apiId', keepEmpty: true, help: apiHelp(B, t.call) || 'Type to search apis by id, method or path' }));
      var api = B.ctx.apiById[t.call];
      if (api) req.push(rChips('Path vars', varRoots([api.path]).map(function (nm) {
        var sc = namesOf(scopeForPath('test', m, B.ctx, tp.concat('vars')));
        var ok = sc.indexOf(nm) >= 0 || Object.keys(api.vars).indexOf(nm) >= 0;
        return { t: '${var.' + nm + '}', cls: 'pill ' + (ok ? 'pass' : 'fail'), title: ok ? 'set before this call' : 'nothing sets it yet' };
      })));
    }
    req.push(rKv(B, 'Vars', tp.concat('vars'), { addLbl: 'Add var', empty: 'No test vars; the api\'s defaults apply.' }));
    authRows(B, tp.concat('auth')).forEach(function (r) { req.push(r); });
    if (!isSteps) bodyRows(B, tp.concat('body'), 'Uses the api\'s body' + (api && api.model.body !== undefined ? ': ' + flowOf(api.model.body) : '')).forEach(function (r) { req.push(r); });
    secs.push(sec('Request', { sub: isSteps ? 'Several actions in order; each can check and save' : 'The api already knows its URL, auth, checks and cleanup', fid: fidOf(tp.concat('call')) }, req));
    if (isSteps) {
      var st = Array.isArray(t.steps) ? t.steps : [];
      st.forEach(function (s, si) { actionSec(B, tp.concat('steps'), si, st.length, 'Step', { isStep: true }).forEach(function (x) { secs.push(x); }); });
      secs.push(sec('', {}, [rAdds([{ l: '+ Add step', on: function () { B.c.editModel(function (mm) { var a = getIn(mm, tp.concat('steps')); if (!Array.isArray(a)) { a = []; setIn(mm, tp.concat('steps'), a); } a.push({ id: uniqueId('step', a.filter(isObj).map(function (x) { return x.id; })).replace(/^step2$/, 'step' + (a.length + 1)), call: '' }); }); B.c.focusLater(fidOf(tp.concat('steps', st.length, 'call'))); } }])]));
    }
    secs.push(sec('Expect', { sub: isSteps ? 'Checked on the last step, added to its api\'s expect' : 'Added to the api\'s expect; matching keys replace it', fid: fidOf(tp.concat('expect')) },
      expectRows(B, tp.concat('expect'), { statusPh: api && api.status !== undefined ? 'api expects ' + fmtStatus(api.status) : 'from the api' })));
    var sp = [rKv(B, 'Save', tp.concat('save'), { addLbl: 'Add value', newKey: 'value', newVal: '${response.body.id}', vph: '${response.body.id}', empty: 'Nothing saved; values are for after and afterEach.' })];
    if (t.poll !== undefined) pollRows(B, tp.concat('poll')).forEach(function (r) { sp.push(r); });
    else sp.push(rAdds([{ l: '+ Poll until', on: function () { B.c.editModel(function (mm) { setIn(mm, tp.concat('poll'), { until: { body: { match: { status: 'done' } } }, every: '1s', timeout: '30s' }); }); } }]));
    secs.push(sec('Save and poll', { fid: fidOf(tp.concat('save')) }, sp));
    secs.push(sec('Examples', { sub: 'One test instance per row; read values as ${example.column}', fid: fidOf(tp.concat('examples')) }, examplesRows(B, tp)));
    secs.push(sec('Run control', { fid: fidOf(tp.concat('repeat')) }, [
      rText(B, 'Repeat', tp.concat('repeat'), { type: 'number', ph: '1', parse: function (r) { return r === '' ? undefined : +r; }, help: 'Each run is its own instance' }),
      rText(B, 'Retry attempts', tp.concat('retry', 'attempts'), { type: 'number', ph: 'none', parse: function (r) { return r === '' ? undefined : +r; }, help: 'A pass after retry is flaky' }),
      rText(B, 'Retry delay', tp.concat('retry', 'delay'), { ph: '1s' }),
      rText(B, 'Timeout', tp.concat('timeout'), { ph: 'per request × calls', help: 'Whole-test limit, like 20s' })
    ]));
  }
  return { nav: nav, secs: secs, add: { l: 'Add test', on: function () { B.c.addTest(); } } };
}
function tagsParse(raw) { var a = String(raw).split(',').map(function (s) { return s.trim(); }).filter(Boolean); return a.length ? a : undefined; }
function examplesRows(B, tp) {
  var ex = getIn(B.m, tp.concat('examples')), P = tp.concat('examples'), rows = [];
  var dsOpts = [{ v: '', l: 'Rows written here' }].concat(B.ctx.datasets.map(function (d) { return { v: '${dataset.' + d.id + '}', l: 'dataset ' + d.id + ' (' + d.path + ', ' + d.rows + ' rows)' }; }));
  var dsSel = rSelect(B, 'Rows come from', P, dsOpts, { value: typeof ex === 'string' ? ex : '', fid: fidOf(P) + '-src', span: 'w2', noDiag: true, set: function (v) {
    B.c.editModel(function (m) { if (v) setIn(m, P, v); else setIn(m, P, [{ value: '' }]); });
  } });
  if (ex === undefined) {
    rows.push(rNote('No examples: the test runs once.', 'muted'));
    rows.push(rAdds([{ l: '+ Examples table', on: function () { B.c.editModel(function (m) { setIn(m, P, [{ value: 'first' }, { value: 'second' }]); }); } },
      B.ctx.datasets.length ? { l: '+ Use dataset ' + B.ctx.datasets[0].id, on: function () { B.c.editModel(function (m) { setIn(m, P, '${dataset.' + B.ctx.datasets[0].id + '}'); }); } } : null]));
    return rows;
  }
  rows.push(dsSel);
  if (typeof ex === 'string' || !Array.isArray(ex)) {
    var mm = /\$\{dataset\.(\w+)\}/.exec(String(ex)), ds = mm ? B.ctx.datasets.filter(function (d) { return d.id === mm[1]; })[0] : null;
    rows.push(rNote(ds ? 'Each of the ' + ds.rows + ' rows of ' + ds.path + ' becomes a test instance; columns: ' + ds.cols.join(', ') : 'Rows come from ' + cellText(ex), 'muted'));
    if (ds) rows.push(rAdds([{ l: 'Copy rows here to edit them', on: function () {
      var r = csvParse(B.ctx.texts[ds.path]); if (!r.ok) return;
      B.c.editModel(function (m) { setIn(m, P, r.value.rows.filter(function (x) { return x.length === r.value.cols.length; }).map(function (row) { var o = {}; r.value.cols.forEach(function (c, ci) { o[c] = parseCell(row[ci]); }); return o; })); });
    } }]));
    return rows;
  }
  var cols = [];
  ex.forEach(function (r) { if (isObj(r)) Object.keys(r).forEach(function (k) { if (cols.indexOf(k) < 0) cols.push(k); }); });
  rows.push(gridRow(B, 'Examples', P, cols, ex.map(function (r) { return cols.map(function (c) { return isObj(r) ? r[c] : undefined; }); }), {
    setCell: function (ri, ci, raw) { B.c.editModel(function (m) { var a = getIn(m, P); if (!isObj(a[ri])) a[ri] = {}; a[ri][cols[ci]] = parseCell(raw); }); },
    renameCol: function (ci, raw, fid) { var to = String(raw).trim(); if (!to || (cols.indexOf(to) >= 0 && to !== cols[ci])) { B.c.setDraft(fid, raw); return; } B.c.setDraft(fid, undefined); B.c.editModel(function (m) { var a = getIn(m, P); a.forEach(function (r, ri) { if (isObj(r)) a[ri] = renameKey(r, cols[ci], to); }); }); },
    delCol: function (ci) { B.c.editModel(function (m) { var a = getIn(m, P); a.forEach(function (r) { if (isObj(r)) delete r[cols[ci]]; }); if (a.every(function (r) { return !isObj(r) || !Object.keys(r).length; })) setIn(m, P, undefined); }); },
    addCol: function () { B.c.editModel(function (m) { var a = getIn(m, P), nm = uniqueKey(a[0] || {}, 'column'); a.forEach(function (r) { if (isObj(r)) r[nm] = ''; }); }); },
    addRow: function () { B.c.editModel(function (m) { var a = getIn(m, P), o = {}; cols.forEach(function (c) { o[c] = ''; }); a.push(o); }); },
    delRow: function (ri) { B.c.editModel(function (m) { setIn(m, P.concat(ri), undefined); }); },
    colsEditable: true
  }));
  return rows;
}
function gridRow(B, lbl, P, cols, data, o) {
  var gcols = cols.map(function (c, ci) {
    var fid = fidOf(P) + '-c' + ci;
    var ed = !!o.colsEditable && (!o.colEditable || o.colEditable(ci)), rad = !!o.radio && (!o.radio.on || o.radio.on(ci));
    return { val: shownVal(B, fid, c), fid: fid, isEdit: ed, isStaticHead: !ed, name: c, aria: 'Column name ' + c, icls: 'input mono th-in' + (B.kd[fid] !== undefined ? ' err' : ''),
      on: function (e) { B.c.fieldInput(fid, e.target.value, null, null, null, null, true); o.renameCol(ci, e.target.value, fid); }, bl: function () { B.c.fieldBlur(fid); }, del: function () { o.delCol(ci); }, delAria: 'Delete column ' + c, hasDel: !!o.delCol && ed,
      isRadio: rad, rChk: rad ? o.radio.checked(ci) : false, rOn: rad ? function () { o.radio.set(ci); } : null, rName: 'r-' + fidOf(P), rLbl: o.radio ? o.radio.label : '' };
  });
  var rows = data.map(function (row, ri) {
    return { n: ri + 1, cells: row.map(function (v, ci) {
      var fid = fidOf(P) + '-r' + ri + 'c' + ci, d = o.diagOf ? o.diagOf(ri, ci) : null;
      B.c.fset[fid] = function (raw) { o.setCell(ri, ci, raw, fid); };
      return { val: shownVal(B, fid, v === undefined ? '' : cellText(v)), fid: fid, ph: o.phOf ? o.phOf(ri, ci) : '', aria: (o.rowName ? o.rowName(ri) : 'Row ' + (ri + 1)) + ', ' + cols[ci], icls: 'input mono' + (d ? ' err' : ''), title: d || '',
        on: function (e) { B.c.fieldInput(fid, e.target.value, e.target.selectionStart, null, null, B.c.fset[fid], o.noDraft); }, bl: function () { B.c.fieldBlur(fid); } };
    }), del: function () { o.delRow(ri); }, delAria: 'Delete row ' + (ri + 1), hasDel: !!o.delRow };
  });
  return { isGrid: true, lbl: lbl, cls: 'fld full', cols: gcols, rows: rows, addRow: o.addRow, addCol: o.addCol, hasAddCol: !!o.addCol, addRowLbl: o.addRowLbl || 'Add row', addColLbl: o.addColLbl || 'Add column', note: o.note || '', hasNote: !!o.note, firstHead: o.firstHead || '#', hasFirst: o.firstHead !== null };
}
