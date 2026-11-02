// ===================== other formats =====================
function csvParse(text) {
  var rows = [], row = [], f = '', i = 0, q = false, s = String(text), lines = [], line = 1, rowLine = 1;
  for (; i < s.length; i++) {
    var c = s[i];
    if (q) { if (c === '"') { if (s[i + 1] === '"') { f += '"'; i++; } else q = false; } else { if (c === '\n') line++; f += c; } continue; }
    if (c === '"') { q = true; continue; }
    if (c === ',') { row.push(f); f = ''; continue; }
    if (c === '\r') continue;
    if (c === '\n') { row.push(f); rows.push(row); lines.push(rowLine); row = []; f = ''; line++; rowLine = line; continue; }
    f += c;
  }
  if (f !== '' || row.length) { row.push(f); rows.push(row); lines.push(rowLine); }
  if (q) return { ok: false, error: { msg: 'a quoted field is never closed', line: rowLine } };
  var cols = rows.length ? rows[0] : [];
  return { ok: true, value: { cols: cols, rows: rows.slice(1) }, lines: lines };
}
function csvField(v) { v = v == null ? '' : String(v); return /[",\n]/.test(v) ? '"' + v.replace(/"/g, '""') + '"' : v; }
function csvStringify(m) { return [m.cols].concat(m.rows).map(function (r) { return r.map(csvField).join(','); }).join('\n') + '\n'; }
function jsonParse(text) {
  try { return { ok: true, value: JSON.parse(text) }; }
  catch (e) {
    var msg = String(e.message), line = 1, m;
    if ((m = /line (\d+) column (\d+)/.exec(msg))) line = +m[1];
    else if ((m = /position (\d+)/.exec(msg))) line = String(text).slice(0, +m[1]).split('\n').length;
    else if (/Unexpected end/.test(msg)) line = String(text).replace(/\s+$/, '').split('\n').length;
    msg = msg.replace(/^JSON\.parse: /, '').replace(/ in JSON at position \d+.*$/, '').replace(/\s*\(line \d+ column \d+\)/, '');
    return { ok: false, error: { msg: msg, line: line } };
  }
}
function hookExports(text) {
  var out = [], re = /export\s+(?:const|let|function|async\s+function)\s+([A-Za-z_$][\w$]*)/g, m;
  while ((m = re.exec(text))) out.push({ name: m[1], line: String(text).slice(0, m.index).split('\n').length });
  return out;
}

// ===================== line diff (LCS) =====================
function lineDiff(a, b) {
  var A = String(a).split('\n'), B = String(b).split('\n'), ops = [];
  var pre = 0; while (pre < A.length && pre < B.length && A[pre] === B[pre]) pre++;
  var suf = 0; while (suf < A.length - pre && suf < B.length - pre && A[A.length - 1 - suf] === B[B.length - 1 - suf]) suf++;
  var a2 = A.slice(pre, A.length - suf), b2 = B.slice(pre, B.length - suf), n = a2.length, m = b2.length, i, j;
  var dp = []; for (i = 0; i <= n; i++) { dp.push(new Array(m + 1).fill(0)); }
  for (i = n - 1; i >= 0; i--) for (j = m - 1; j >= 0; j--) dp[i][j] = a2[i] === b2[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
  for (i = 0; i < pre; i++) ops.push({ t: '=', a: i + 1, b: i + 1, text: A[i] });
  i = 0; j = 0;
  while (i < n || j < m) {
    if (i < n && j < m && a2[i] === b2[j]) { ops.push({ t: '=', a: pre + i + 1, b: pre + j + 1, text: a2[i] }); i++; j++; }
    else if (j < m && (i >= n || dp[i][j + 1] >= dp[i + 1][j])) { ops.push({ t: '+', b: pre + j + 1, text: b2[j] }); j++; }
    else { ops.push({ t: '-', a: pre + i + 1, text: a2[i] }); i++; }
  }
  for (i = 0; i < suf; i++) ops.push({ t: '=', a: A.length - suf + i + 1, b: B.length - suf + i + 1, text: A[A.length - suf + i] });
  return ops;
}
function diffStats(ops) {
  var s = { add: 0, del: 0 };
  ops.forEach(function (o) { if (o.t === '+') s.add++; else if (o.t === '-') s.del++; });
  return s;
}
function diffHunks(ops, ctxN) {
  var keep = ops.map(function () { return false; });
  ops.forEach(function (o, i) { if (o.t !== '=') for (var k = Math.max(0, i - ctxN); k <= Math.min(ops.length - 1, i + ctxN); k++) keep[k] = true; });
  var out = [], skipped = 0;
  ops.forEach(function (o, i) {
    if (keep[i]) { if (skipped) { out.push({ gap: skipped }); skipped = 0; } out.push(o); }
    else skipped++;
  });
  if (skipped) out.push({ gap: skipped });
  return out;
}
function diffSideBySide(ops) {
  var rows = [], i = 0;
  while (i < ops.length) {
    if (ops[i].t === '=') { rows.push({ l: ops[i], r: ops[i] }); i++; continue; }
    var dels = [], adds = [];
    while (i < ops.length && ops[i].t === '-') dels.push(ops[i++]);
    while (i < ops.length && ops[i].t === '+') adds.push(ops[i++]);
    while (i < ops.length && ops[i].t === '-') dels.push(ops[i++]);
    for (var k = 0; k < Math.max(dels.length, adds.length); k++) rows.push({ l: dels[k] || null, r: adds[k] || null });
  }
  return rows;
}

// ===================== project context =====================
var NAMESPACES = ['var', 'env', 'secret', 'response', 'example', 'item', 'iteration', 'fake', 'dataset', 'run'];
var MATCHERS = ['eq', 'ne', 'gt', 'gte', 'lt', 'lte', 'between', 'in', 'notIn', 'type', 'exists', 'empty', 'length', 'contains', 'startsWith', 'endsWith', 'regex', 'format', 'every', 'some', 'none', 'not', 'allOf', 'anyOf'];
var METHODS = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS'];
var TIMING = ['dns', 'connect', 'tls', 'firstByte', 'download', 'total'];
var FAKES = ['person.fullName', 'person.firstName', 'internet.email', 'internet.url', 'location.city', 'location.countryCode', 'lorem.words(2)', 'string.uuid', 'number.int(100)', 'commerce.price'];
var KEYS = {
  testFile: ORDER.testFile, test: ORDER.test, action: ORDER.action, expect: ORDER.expect, bodyExpect: ORDER.bodyExpect,
  apiFile: ORDER.apiFile, api: ORDER.api, load: ORDER.load, config: ORDER.config, poll: ORDER.poll, skip: ORDER.skip,
  retry: ORDER.retry, cache: ORDER.cache, mix: ORDER.mix, stage: ORDER.stage, profile: ORDER.profile, cleanup: ORDER.cleanup
};
function templateRefs(s) {
  var out = [], re = /\$\{\s*([^}]*)\}/g, m;
  if (typeof s !== 'string') return out;
  while ((m = re.exec(s))) {
    var expr = m[1].split('|')[0].trim(), mm = /^([A-Za-z_][\w]*)(?:\.([A-Za-z_][\w]*))?/.exec(expr);
    if (mm) out.push({ ns: mm[1], name: mm[2] || '', raw: m[0], expr: expr });
  }
  return out;
}
function walkStrings(v, path, fn, stop) {
  if (stop && stop(path)) return;
  if (typeof v === 'string') fn(v, path);
  else if (Array.isArray(v)) v.forEach(function (x, i) { walkStrings(x, path.concat(i), fn, stop); });
  else if (isObj(v)) Object.keys(v).forEach(function (k) { walkStrings(v[k], path.concat(k), fn, stop); });
}
function varRoots(vals) {
  var out = [];
  vals.forEach(function (v) { walkStrings(v, [], function (s) { templateRefs(s).forEach(function (r) { if (r.ns === 'var' && r.name && out.indexOf(r.name) < 0) out.push(r.name); }); }); });
  return out;
}
function buildCtx(texts, parse) {
  var ctx = { apis: [], apiById: {}, apiFiles: {}, tests: [], testById: {}, configVars: {}, profiles: [], authIds: [], authDefs: [], beforeRun: [], schemas: [], datasets: [], hooks: [], snapshots: [], files: Object.keys(texts), texts: texts };
  Object.keys(texts).forEach(function (p) {
    var k = fileKind(p), t = texts[p], r;
    if (k === 'schema') ctx.schemas.push(p);
    if (k === 'dataset') { r = csvParse(t); ctx.datasets.push({ id: camelOf(baseOf(p).replace(/\.csv$/, '')), path: p, cols: r.ok ? r.value.cols : [], rows: r.ok ? r.value.rows.length : 0 }); }
    if (k === 'hook') ctx.hooks.push({ path: p, exports: hookExports(t) });
    if (!isYamlKind(k)) return;
    r = parse(t);
    if (!r.ok || !isObj(r.value)) return;
    var f = r.value;
    if (k === 'openapi') {
      var sc = f.components && f.components.schemas ? Object.keys(f.components.schemas) : [];
      ctx.snapshots.push({ path: p, schemas: sc });
    }
    if (k === 'api') {
      var fid = typeof f.id === 'string' ? f.id : baseOf(p).replace(/\.api\.ya?ml$/, '');
      ctx.apiFiles[fid] = { path: p, vars: isObj(f.vars) ? f.vars : {}, baseUrl: f.baseUrl, ids: [] };
      (Array.isArray(f.apis) ? f.apis : []).forEach(function (a, i) {
        if (!isObj(a) || typeof a.id !== 'string') return;
        var info = { full: fid + '.' + a.id, fileId: fid, id: a.id, name: a.name || '', method: String(a.method || 'GET').toUpperCase(), path: String(a.path || ''), file: p, idx: i, vars: isObj(a.vars) ? a.vars : {}, saves: isObj(a.save) ? Object.keys(a.save) : [], status: a.expect ? a.expect.status : undefined, model: a };
        var provided = Object.keys(info.vars).concat(Object.keys(isObj(f.vars) ? f.vars : {}));
        info.needs = varRoots([a.path, a.query, a.headers, a.body, f.baseUrl, f.headers]).filter(function (x) { return provided.indexOf(x) < 0; });
        ctx.apis.push(info); ctx.apiById[info.full] = info; ctx.apiFiles[fid].ids.push(a.id);
      });
    }
    if (k === 'test') {
      var tfid = typeof f.id === 'string' ? f.id : '';
      (Array.isArray(f.tests) ? f.tests : []).forEach(function (tt, i) {
        if (!isObj(tt) || typeof tt.id !== 'string') return;
        var info = { full: tfid + '.' + tt.id, id: tt.id, name: tt.name || '', file: p, idx: i, model: tt };
        ctx.tests.push(info); ctx.testById[info.full] = info;
      });
    }
    if (k === 'config') {
      ctx.config = f;
      ctx.configVars = isObj(f.vars) ? f.vars : {};
      ctx.profiles = (Array.isArray(f.profiles) ? f.profiles : []).filter(isObj).map(function (pr) { return { id: pr.id, vars: isObj(pr.vars) ? pr.vars : {}, isDefault: pr.default === true }; });
      ctx.authDefs = (Array.isArray(f.auth) ? f.auth : []).filter(isObj);
      ctx.authIds = ctx.authDefs.map(function (a) { return a.id; }).filter(Boolean);
      ctx.beforeRun = (Array.isArray(f.beforeRun) ? f.beforeRun : []).filter(function (a) { return isObj(a) && a.id; }).map(function (a) { return { id: a.id, call: a.call || a.test || a.function || '' }; });
    }
  });
  return ctx;
}
function actionTarget(a) { return !isObj(a) ? '' : a.call ? a.call : a.test ? a.test : a['function'] ? a['function'] : a.command ? a.command : ''; }
function actionKind(a) { return !isObj(a) ? 'call' : a.call !== undefined ? 'call' : a.test !== undefined ? 'test' : a['function'] !== undefined ? 'function' : a.command !== undefined ? 'command' : 'call'; }
function testSaves(ctx, full, seen) {
  var t = ctx.testById[full]; seen = seen || {};
  if (!t || seen[full]) return [];
  seen[full] = true;
  var out = [], m = t.model;
  (Array.isArray(m.steps) ? m.steps : []).forEach(function (s) { out = out.concat(actionSaveNames(ctx, s, seen)); });
  if (m.call && ctx.apiById[m.call]) out = out.concat(ctx.apiById[m.call].saves);
  if (isObj(m.save)) out = out.concat(Object.keys(m.save));
  return out;
}
function actionSaveNames(ctx, a, seen) {
  if (!isObj(a)) return [];
  var out = [];
  if (a.call && ctx.apiById[a.call]) out = out.concat(ctx.apiById[a.call].saves);
  if (a.test) out = out.concat(testSaves(ctx, a.test, seen));
  if (isObj(a.save)) out = out.concat(Object.keys(a.save));
  return out;
}
function baseScope(ctx) {
  var out = [];
  Object.keys(ctx.configVars).forEach(function (k) { out.push({ name: k, src: 'config vars' }); });
  ctx.profiles.forEach(function (p) { Object.keys(p.vars).forEach(function (k) { if (!out.some(function (o) { return o.name === k; })) out.push({ name: k, src: 'profile ' + p.id }); }); });
  ctx.beforeRun.forEach(function (b) { out.push({ name: b.id, src: 'beforeRun · ' + b.call, shared: true }); });
  return out;
}
function namesOf(scope) { return scope.map(function (s) { return s.name; }); }
function actionSite(ctx, a, path, scope, opts) {
  opts = opts || {};
  var api = isObj(a) && a.call ? ctx.apiById[a.call] : null, own = [];
  if (api) {
    Object.keys(api.vars).forEach(function (k) { own.push({ name: k, src: 'default of ' + a.call }); });
    var af = ctx.apiFiles[api.fileId];
    if (af) Object.keys(af.vars).forEach(function (k) { own.push({ name: k, src: api.fileId + ' file vars' }); });
  }
  if (isObj(a) && isObj(a.vars)) Object.keys(a.vars).forEach(function (k) { own.push({ name: k, src: 'vars on this action' }); });
  if (opts.selfId && isObj(a) && a.id) own.push({ name: a.id, src: 'this action', shared: true });
  return { path: path, kind: opts.kind || 'action', a: a, api: api, scope: scope, own: own, forEach: isObj(a) && a.forEach !== undefined, ex: opts.ex, ids: opts.ids || [] };
}
function examplesKeys(t) {
  if (!isObj(t) || t.examples === undefined) return null;
  if (Array.isArray(t.examples)) { var ks = []; t.examples.forEach(function (r) { if (isObj(r)) Object.keys(r).forEach(function (k) { if (ks.indexOf(k) < 0) ks.push(k); }); }); return ks; }
  return 'any';
}
function sitesFor(kind, m, ctx) {
  var sites = [];
  if (!isObj(m)) return sites;
  var base = baseScope(ctx);
  if (kind === 'test') {
    var fileVars = [];
    if (isObj(m.vars)) Object.keys(m.vars).forEach(function (k) { fileVars.push({ name: k, src: 'file vars' }); });
    (Array.isArray(m.profiles) ? m.profiles : []).forEach(function (p) { if (isObj(p) && isObj(p.vars)) Object.keys(p.vars).forEach(function (k) { if (!fileVars.some(function (f) { return f.name === k; })) fileVars.push({ name: k, src: 'file profile ' + p.id }); }); });
    sites.push({ path: ['vars'], kind: 'vars', scope: base, own: [], ids: [] });
    var S0 = base.concat(fileVars), shared = [];
    (Array.isArray(m.beforeAll) ? m.beforeAll : []).forEach(function (a, i) {
      sites.push(actionSite(ctx, a, ['beforeAll', i], S0.concat(shared), { selfId: true }));
      if (isObj(a) && a.id) shared.push({ name: a.id, src: 'beforeAll · ' + actionTarget(a), shared: true });
    });
    var S1 = S0.concat(shared), each = [];
    (Array.isArray(m.beforeEach) ? m.beforeEach : []).forEach(function (a, i) {
      sites.push(actionSite(ctx, a, ['beforeEach', i], S1.concat(each)));
      actionSaveNames(ctx, a).forEach(function (n) { each.push({ name: n, src: 'saved by ' + actionTarget(a) + ' (beforeEach)' }); });
    });
    var S2 = S1.concat(each);
    (Array.isArray(m.tests) ? m.tests : []).forEach(function (t, ti) {
      if (!isObj(t)) return;
      var tp = ['tests', ti], ex = examplesKeys(t), tv = [];
      if (isObj(t.vars)) Object.keys(t.vars).forEach(function (k) { tv.push({ name: k, src: 'test vars' }); });
      var ST = S2.concat(tv), before = [], ids = [];
      (Array.isArray(t.before) ? t.before : []).forEach(function (a, i) {
        sites.push(actionSite(ctx, a, tp.concat('before', i), ST.concat(before), { ex: ex, ids: ids.slice() }));
        actionSaveNames(ctx, a).forEach(function (n) { before.push({ name: n, src: 'saved by ' + actionTarget(a) + ' (before)' }); });
        if (isObj(a) && a.id) ids.push(a.id);
      });
      var SB = ST.concat(before), steps = [];
      (Array.isArray(t.steps) ? t.steps : []).forEach(function (s, si) {
        sites.push(actionSite(ctx, s, tp.concat('steps', si), SB.concat(steps), { kind: 'step', ex: ex, ids: ids.slice() }));
        actionSaveNames(ctx, s).forEach(function (n) { steps.push({ name: n, src: 'saved by step ' + (isObj(s) && s.id ? s.id : si + 1) + ' (' + actionTarget(s) + ')' }); });
        if (isObj(s) && s.id) ids.push(s.id);
      });
      var callSaves = [];
      if (t.call && ctx.apiById[t.call]) ctx.apiById[t.call].saves.forEach(function (n) { callSaves.push({ name: n, src: 'saved by ' + t.call }); });
      var tsite = actionSite(ctx, t, tp, SB.concat(steps).concat(callSaves), { kind: 'test', ex: ex, ids: ids.slice() });
      tsite.callScope = SB;
      sites.push(tsite);
      var all = SB.concat(steps).concat(callSaves);
      if (isObj(t.save)) Object.keys(t.save).forEach(function (k) { all.push({ name: k, src: 'saved by this test' }); });
      (Array.isArray(t.after) ? t.after : []).forEach(function (a, i) { sites.push(actionSite(ctx, a, tp.concat('after', i), all, { ex: ex, ids: ids.slice() })); });
    });
    (Array.isArray(m.afterEach) ? m.afterEach : []).forEach(function (a, i) { sites.push(actionSite(ctx, a, ['afterEach', i], S2)); });
    (Array.isArray(m.afterAll) ? m.afterAll : []).forEach(function (a, i) { sites.push(actionSite(ctx, a, ['afterAll', i], S1)); });
  }
  if (kind === 'config') {
    var br = [];
    Object.keys(ctx.configVars).forEach(function (k) { br.push({ name: k, src: 'config vars' }); });
    (Array.isArray(m.beforeRun) ? m.beforeRun : []).forEach(function (a, i) { sites.push(actionSite(ctx, a, ['beforeRun', i], br.slice(), { selfId: true })); if (isObj(a) && a.id) br.push({ name: a.id, src: 'beforeRun · ' + actionTarget(a), shared: true }); });
    (Array.isArray(m.afterRun) ? m.afterRun : []).forEach(function (a, i) { sites.push(actionSite(ctx, a, ['afterRun', i], br.slice())); });
  }
  return sites;
}
function siteAt(sites, path) {
  var best = null;
  sites.forEach(function (s) { if (isPrefix(s.path, path) && (!best || s.path.length > best.path.length)) best = s; });
  return best;
}
function scopeForPath(kind, m, ctx, path) {
  var sites = sitesFor(kind, m, ctx), s = siteAt(sites, path);
  if (!s) return baseScope(ctx);
  var sc = s.scope.concat(s.own);
  if (s.kind === 'test' && path.length > 2 && ['vars', 'body', 'auth', 'call'].indexOf(path[2]) >= 0) sc = s.callScope.concat(s.own);
  var seen = {}, out = [];
  sc.slice().reverse().forEach(function (x) { if (!seen[x.name]) { seen[x.name] = 1; out.unshift(x); } });
  return out;
}
