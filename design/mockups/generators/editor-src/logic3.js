// ===================== diagnostics =====================
var AUTH_TYPES = ['basic', 'bearer', 'apiKey', 'oauth2ClientCredentials', 'oauth2AuthorizationCode', 'oauth2Refresh', 'flow', 'plugin'];
var REPORT_FORMATS = ['ctrf', 'junit', 'html', 'json', 'markdown'];
var LOAD_METRICS = ['p50', 'p90', 'p95', 'p99', 'max', 'errorRate', 'rps'];
function lineOf(index, path) {
  if (!index || !path) return null;
  for (var n = path.length; n >= 0; n--) { var nd = index[pk(path.slice(0, n))]; if (nd) return nd.vline !== undefined && n === path.length ? nd.vline : nd.line; }
  return null;
}
function validStatus(v) {
  if (typeof v === 'number') return Math.floor(v) === v && v >= 100 && v <= 599;
  if (typeof v === 'string') return /^[1-5]xx$/.test(v);
  if (Array.isArray(v)) return v.length > 0 && v.every(function (x) { return validStatus(x); });
  if (isObj(v)) return Object.keys(v).length > 0 && Object.keys(v).every(function (k) { return MATCHERS.indexOf(k) >= 0; });
  return false;
}
function statusFix(v) {
  if (typeof v === 'number' && v > 599 && String(v).length > 3) { var s = +String(v).slice(0, 3); if (validStatus(s)) return s; }
  if (typeof v === 'string' && /^[1-5]xx$/i.test(v)) return v.toLowerCase();
  if (typeof v === 'string' && /^\s*\d{3}\s*$/.test(v) && validStatus(+v)) return +v;
  return null;
}
function parseStatusInput(raw) {
  var s = String(raw).trim();
  if (s === '') return undefined;
  if (s.indexOf(',') >= 0) return s.split(',').map(function (x) { x = x.trim(); return /^\d+$/.test(x) ? +x : x; }).filter(function (x) { return x !== ''; });
  if (/^\d+$/.test(s)) return +s;
  return s;
}
function fmtStatus(v) { return v === undefined || v === null ? '' : Array.isArray(v) ? v.join(', ') : isObj(v) ? flowOf(v) : String(v); }
function flowOf(v) {
  if (Array.isArray(v)) return '[' + v.map(flowOf).join(', ') + ']';
  if (isObj(v)) return '{' + Object.keys(v).map(function (k) { return yKey(k) + ': ' + flowOf(v[k]); }).join(', ') + '}';
  if (typeof v === 'string') return /[,\[\]{}]/.test(v) && v.indexOf('${') < 0 ? JSON.stringify(v) : yFmt(v);
  return yFmt(v);
}
function parseCell(raw) {
  var s = String(raw);
  if (s.trim() === '') return '';
  if (/^\s*[\[{]/.test(s)) {
    try { var r = flowParse(s.trim()); if (!r.incomplete && s.trim().slice(r.end).trim() === '') return r.v; } catch (e) { }
  }
  if (/^\s*["']/.test(s)) { try { var q = scanQuoted(s.trim(), 0); if (q && q.end === s.trim().length) return q.val; } catch (e2) { } }
  if (s.indexOf('${') >= 0) return s;
  var v = yScalar(s.trim());
  return typeof v === 'number' && isNaN(v) ? s : v;
}
function cellText(v) { return v === undefined || v === null ? (v === null ? 'null' : '') : typeof v === 'object' ? flowOf(v) : String(v); }
function unknownKeys(obj, sh, path, add, index) {
  if (!isObj(obj) || !KEYS[sh]) return;
  Object.keys(obj).forEach(function (k) {
    if (KEYS[sh].indexOf(k) >= 0) return;
    var s = suggest(k, KEYS[sh]), kp = path.concat(k), ln = index && index[pk(kp)] ? index[pk(kp)].line : lineOf(index, path);
    add('error', 'unknown-key', 'Unknown key "' + k + '"' + (s ? '; did you mean "' + s + '"?' : ''), kp,
      s ? [{ label: 'Rename to ' + s, type: 'renameKey', path: path, from: k, to: s }] : [], ln);
  });
}
function hookRef(ref, fpath, ctx) {
  var m = /^(.*)#([\w$]+)$/.exec(String(ref));
  if (!m) return { ok: false, msg: 'Function references look like hooks/checks.ts#exportName' };
  var target = fpath === 'vibranium.config.yaml' ? resolvePath('x', m[1]) : resolvePath(fpath, m[1]);
  var h = ctx.hooks.filter(function (x) { return x.path === target; })[0];
  if (!h) return { ok: false, msg: 'Hook file not found: ' + target };
  if (!h.exports.some(function (e) { return e.name === m[2]; })) {
    var s = suggest(m[2], h.exports.map(function (e) { return e.name; }));
    return { ok: false, msg: target + ' has no export "' + m[2] + '"' + (s ? '; did you mean "' + s + '"?' : ''), find: '#' + m[2], replace: s ? '#' + s : null };
  }
  return { ok: true };
}
function checkExpect(e, path, fpath, ctx, add, index) {
  if (!isObj(e)) return;
  unknownKeys(e, 'expect', path, add, index);
  if (e.status !== undefined && !validStatus(e.status)) {
    var fx = statusFix(e.status), ln = lineOf(index, path.concat('status'));
    add('error', 'status', 'expect.status ' + JSON.stringify(e.status) + ' is not a valid HTTP status; use a code (100 to 599), a list like [200, 204] or a class like 2xx', path.concat('status'),
      fx !== null ? [{ label: 'Change to ' + fmtStatus(fx), type: 'set', path: path.concat('status'), value: fx }] : []);
  }
  if (isObj(e.body)) {
    unknownKeys(e.body, 'bodyExpect', path.concat('body'), add, index);
    var sc = e.body.schema;
    if (isObj(sc) && typeof sc.$file === 'string') {
      var parts = sc.$file.split('#'), target = resolvePath(fpath, parts[0]), sp2 = path.concat('body', 'schema'), ln2 = lineOf(index, sp2.concat('$file'));
      if (ctx.texts[target] === undefined) {
        var byBase = ctx.schemas.filter(function (s) { return baseOf(s) === baseOf(target); })[0] || suggest(target, ctx.schemas);
        var rel = byBase ? relPath(fpath, byBase) : null;
        add('error', 'schema', 'Schema file not found: ' + target + (rel ? '; did you mean ' + byBase + '?' : ''), sp2,
          rel ? [{ label: 'Use ' + byBase, type: 'text', line: ln2, find: parts[0], replace: rel }] : [], ln2);
      } else if (parts[1]) {
        var snap = ctx.snapshots.filter(function (s) { return s.path === target; })[0], nm = parts[1].split('/').pop();
        if (snap && snap.schemas.indexOf(nm) < 0) add('error', 'schema', target + ' has no schema "' + nm + '"', sp2, [], ln2);
      }
    }
  }
  if (Array.isArray(e.custom)) e.custom.forEach(function (c, i) {
    if (!isObj(c) || typeof c['function'] !== 'string') return;
    var r = hookRef(c['function'], fpath, ctx), cp = path.concat('custom', i, 'function');
    if (!r.ok) add('error', 'hook', r.msg, cp, r.replace ? [{ label: 'Use ' + r.replace.slice(1), type: 'text', line: lineOf(index, cp), find: r.find, replace: r.replace }] : []);
  });
  if (isObj(e.timing)) Object.keys(e.timing).forEach(function (k) {
    if (TIMING.indexOf(k) < 0) { var s = suggest(k, TIMING); add('error', 'timing', 'Unknown timing phase "' + k + '"' + (s ? '; did you mean "' + s + '"?' : ''), path.concat('timing', k), s ? [{ label: 'Rename to ' + s, type: 'renameKey', path: path.concat('timing'), from: k, to: s }] : []); }
  });
}
function walkExpects(v, path, fn) {
  if (Array.isArray(v)) v.forEach(function (x, i) { walkExpects(x, path.concat(i), fn); });
  else if (isObj(v)) Object.keys(v).forEach(function (k) {
    if ((k === 'expect' || (k === 'until' && path[path.length - 1] === 'poll')) && isObj(v[k])) fn(v[k], path.concat(k));
    walkExpects(v[k], path.concat(k), fn);
  });
}
function checkSkip(s, path, add, index) {
  if (s === undefined || s === false) return;
  if (s === true || s === null) { add('error', 'skip-reason', 'skip needs a reason; it is shown in reports', path.concat('reason'), [{ label: 'Add a reason', type: 'set', path: path, value: { reason: '' } }], lineOf(index, path)); return; }
  if (typeof s === 'string') { if (!s.trim()) add('error', 'skip-reason', 'skip needs a reason; it is shown in reports', path, [], lineOf(index, path)); return; }
  if (!isObj(s)) return;
  unknownKeys(s, 'skip', path, add, index);
  if (!s.reason || !String(s.reason).trim()) add('error', 'skip-reason', 'skip needs a reason; it is shown in reports', path.concat('reason'), [{ label: 'Write a reason', type: 'jump', path: path.concat('reason') }], lineOf(index, path));
  if (s.until !== undefined) {
    var u = String(s.until);
    if (!/^\d{4}-\d{2}-\d{2}$/.test(u)) add('error', 'skip-until', 'skip.until must be a date like 2026-11-15', path.concat('until'));
    else if (u < TODAY) {
      var d = new Date(TODAY + 'T00:00:00Z'); d.setUTCDate(d.getUTCDate() + 14); var ext = d.toISOString().slice(0, 10);
      add('warn', 'skip-expired', 'skip expired on ' + u + '; the test is still skipped until you remove or extend it', path.concat('until'),
        [{ label: 'Remove skip', type: 'del', path: path }, { label: 'Extend to ' + ext, type: 'set', path: path.concat('until'), value: ext }]);
    }
  }
}
function idRule(id, path, what, add, extra) {
  if (typeof id !== 'string' || !id) return;
  if (!isCamel(id)) {
    var to = camelOf(id);
    add('warn', 'id-case', what + ' id "' + id + '" should be camelCase' + (to ? ' ("' + to + '")' : ''), path, to ? [{ label: 'Rename to ' + to, type: 'rename', path: path, from: id, to: to }] : []);
  } else if (extra) extra();
}
function checkTemplates(site, fpath, ctx, add, index, stopKeys) {
  var names = namesOf(site.scope.concat(site.own)), callNames = site.callScope ? namesOf(site.callScope.concat(site.own)) : names;
  var shared = site.scope.filter(function (s) { return s.shared; }).map(function (s) { return s.name; });
  walkStrings(site.a, site.path, function (s, p) {
    templateRefs(s).forEach(function (r) {
      var rel = p.slice(site.path.length), nm = site.kind === 'test' && ['vars', 'body', 'auth'].indexOf(rel[0]) >= 0 ? callNames : names;
      var ln = lineOf(index, p);
      if (r.ns === 'var') {
        if (!r.name) { add('error', 'tpl', '"' + r.raw + '" needs a name after var.', p); return; }
        if (nm.indexOf(r.name) >= 0) return;
        var sg = suggest(r.name, nm);
        var where = ctx.apis.filter(function (a) { return a.saves.indexOf(r.name) >= 0; }).map(function (a) { return a.full; })[0];
        var fixes = [];
        if (sg) fixes.push({ label: 'Use ' + sg, type: 'text', line: ln, find: '${var.' + r.name, replace: '${var.' + sg });
        if (site.kind !== 'vars') fixes.push({ label: 'Add ' + r.name + ' to vars here', type: 'addVar', base: site.path.concat('vars'), name: r.name });
        add('error', 'var-unset', '${var.' + r.name + '} is used but never set' + (sg ? '; did you mean ' + sg + '?' : '. Set it in vars here or in the file, or save it in an earlier step' + (where ? ' (' + where + ' saves it)' : '')), p, fixes, ln);
      } else if (r.ns === 'item') {
        if (!site.forEach) add('error', 'tpl', '${item} is only set inside an action with forEach', p, [], ln);
      } else if (r.ns === 'example') {
        if (site.ex === null || site.ex === undefined) add('error', 'tpl', '${example.' + r.name + '} is used but this test has no examples', p, [], ln);
        else if (Array.isArray(site.ex) && site.ex.indexOf(r.name) < 0) { var se = suggest(r.name, site.ex); add('error', 'tpl', 'No examples column "' + r.name + '"' + (se ? '; did you mean ' + se + '?' : ''), p, se ? [{ label: 'Use ' + se, type: 'text', line: ln, find: '${example.' + r.name, replace: '${example.' + se }] : [], ln); }
      } else if (r.ns === 'dataset') {
        var ids = ctx.datasets.map(function (d) { return d.id; });
        if (ids.indexOf(r.name) < 0) { var sd = suggest(r.name, ids); add('error', 'tpl', 'No dataset "' + r.name + '"' + (sd ? '; did you mean ' + sd + '?' : ''), p, sd ? [{ label: 'Use ' + sd, type: 'text', line: ln, find: '${dataset.' + r.name, replace: '${dataset.' + sd }] : [], ln); }
      } else if (NAMESPACES.indexOf(r.ns) < 0 && (site.ids || []).indexOf(r.ns) < 0) {
        var so = suggest(r.ns, NAMESPACES.concat(site.ids || []));
        add('error', 'tpl', 'Unknown reference "' + r.raw + '"' + (so ? '; did you mean ' + so + '?' : ''), p, so ? [{ label: 'Use ' + so, type: 'text', line: ln, find: '${' + r.ns, replace: '${' + so }] : [], ln);
      }
    });
    if (site.api && ['PUT', 'PATCH', 'DELETE'].indexOf(site.api.method) >= 0 && (rel0(p, site) === 'vars')) {
      templateRefs(s).forEach(function (r) {
        if (r.ns === 'var' && shared.indexOf(r.name) >= 0) add('warn', 'shared-write', site.api.full + ' (' + site.api.method + ') uses ${var.' + r.name + '…} from a shared block; shared values are read-only, so create the data in beforeEach or before', p, [], lineOf(index, p));
      });
    }
  }, function (p) { var rel = p.slice(site.path.length); return rel.length === 1 && stopKeys && stopKeys.indexOf(rel[0]) >= 0; });
}
function rel0(p, site) { return p[site.path.length]; }
function checkActionSite(site, fpath, ctx, add, index) {
  var a = site.a, path = site.path;
  if (!isObj(a)) { add('error', 'shape', 'Each action is a mapping like "call: users.create"', path); return; }
  if (site.kind === 'test') {
    if (a.call === undefined && a.steps === undefined) add('error', 'test-shape', 'A test needs call or steps', path.concat('id'));
    if (a.call !== undefined && a.steps !== undefined) add('error', 'test-shape', 'A test has either call or steps, not both', path.concat('steps'));
  } else {
    unknownKeys(a, 'action', path, add, index);
    var kinds = ['call', 'test', 'function', 'command'].filter(function (k) { return a[k] !== undefined; });
    if (kinds.length === 0) add('error', 'action-shape', 'An action needs one of call, test, function or command', path);
    if (kinds.length > 1) add('error', 'action-shape', 'An action has one of call, test, function or command, not ' + kinds.join(' and '), path.concat(kinds[1]));
    if (a.id !== undefined) idRule(a.id, path.concat('id'), site.kind === 'step' ? 'Step' : 'Action', add);
  }
  if (typeof a.call === 'string' && !ctx.apiById[a.call]) {
    var s = suggest(a.call, ctx.apis.map(function (x) { return x.full; })), ln = lineOf(index, path.concat('call'));
    add('error', 'unknown-api', 'Unknown api "' + a.call + '"' + (s ? '; did you mean ' + s + '?' : ''), path.concat('call'), s ? [{ label: 'Use ' + s, type: 'text', line: ln, find: a.call, replace: s }] : [], ln);
  }
  if (typeof a.test === 'string' && !ctx.testById[a.test]) {
    var st = suggest(a.test, ctx.tests.map(function (x) { return x.full; })), lt = lineOf(index, path.concat('test'));
    add('error', 'unknown-test', 'Unknown test "' + a.test + '"' + (st ? '; did you mean ' + st + '?' : ''), path.concat('test'), st ? [{ label: 'Use ' + st, type: 'text', line: lt, find: a.test, replace: st }] : [], lt);
  }
  if (typeof a['function'] === 'string') {
    var hr = hookRef(a['function'], fpath, ctx);
    if (!hr.ok) add('error', 'hook', hr.msg, path.concat('function'), hr.replace ? [{ label: 'Use ' + hr.replace.slice(1), type: 'text', line: lineOf(index, path.concat('function')), find: hr.find, replace: hr.replace }] : []);
  }
  if (site.api) {
    var have = namesOf((site.callScope || site.scope).concat(site.own));
    site.api.needs.forEach(function (nm) {
      if (have.indexOf(nm) >= 0) return;
      var saver = ctx.apis.filter(function (x) { return x.saves.indexOf(nm) >= 0; }).map(function (x) { return x.full; })[0];
      add('error', 'var-unset', site.api.full + ' needs ${var.' + nm + '}, and nothing sets it before this call' + (saver ? '; ' + saver + ' saves it, so call it in an earlier step or in before' : ''), path.concat('call'),
        [{ label: 'Add ' + nm + ' to vars here', type: 'addVar', base: path.concat('vars'), name: nm }]);
    });
  }
  if (isObj(a.cache) && a.cache.check !== undefined) {
    var c = a.cache.check;
    if (isObj(c) && typeof c.call === 'string' && !ctx.apiById[c.call]) {
      var sc = suggest(c.call, ctx.apis.map(function (x) { return x.full; })), lc = lineOf(index, path.concat('cache', 'check', 'call'));
      add('error', 'unknown-api', 'Unknown api "' + c.call + '"' + (sc ? '; did you mean ' + sc + '?' : ''), path.concat('cache', 'check', 'call'), sc ? [{ label: 'Use ' + sc, type: 'text', line: lc, find: c.call, replace: sc }] : [], lc);
    }
  }
  checkTemplates(site, fpath, ctx, add, index, site.kind === 'test' ? ['before', 'after', 'steps'] : []);
}
function expectedTestFileId(fpath) { var m = /^services\/([^\/]+)\/(.+)\.test\.ya?ml$/.exec(fpath); return m ? m[1] + '.' + m[2] : null; }
function expectedApiFileId(fpath) { var m = /^services\/([^\/]+)\/[^\/]+\.api\.ya?ml$/.exec(fpath); return m ? m[1] : null; }
function diagnose(fpath, kind, text, P, ctx) {
  var D = [], index = P && P.index;
  function add(sev, code, msg, path, fixes, line) {
    D.push({ sev: sev, code: code, msg: msg, path: path || null, line: line !== undefined && line !== null ? line : lineOf(index, path), fixes: fixes || [] });
  }
  if (kind === 'schema') { if (!P.ok) add('error', 'syntax', 'JSON syntax error: ' + P.error.msg, null, [], P.error.line - 1); return D; }
  if (kind === 'dataset') {
    if (!P.ok) { add('error', 'syntax', 'CSV error: ' + P.error.msg, null, [], P.error.line - 1); return D; }
    var nc = P.value.cols.length;
    if (!nc || P.value.cols.some(function (c) { return !c.trim(); })) add('error', 'csv', 'Every column needs a name in the header row', null, [], 0);
    P.value.rows.forEach(function (r, i) { if (r.length !== nc && !(r.length === 1 && r[0] === '')) add('error', 'csv', 'Row ' + (i + 1) + ' has ' + r.length + ' values; the header has ' + nc, ['rows', i], [], P.lines[i + 1] - 1); });
    return D;
  }
  if (kind === 'hook' || kind === 'text') return D;
  if (!P.ok) { add('error', 'syntax', 'YAML syntax error: ' + P.error.msg, null, [], P.error.line - 1); return D; }
  var m = P.value;
  if (kind === 'openapi') return D;
  if (!isObj(m)) { add('error', 'shape', 'This file should be a mapping of keys', null, [], 0); return D; }
  if (kind === 'test') {
    unknownKeys(m, 'testFile', [], add, index);
    var exp = expectedTestFileId(fpath);
    if (!m.id) add('error', 'file-id', 'A test file needs an id' + (exp ? ' (' + exp + ')' : ''), ['id'], exp ? [{ label: 'Set id to ' + exp, type: 'set', path: ['id'], value: exp }] : [], 0);
    else if (exp && m.id !== exp) add('warn', 'file-id', 'File id "' + m.id + '" does not match its path; services/<service>/<area>.test.yaml has the id ' + exp, ['id'], [{ label: 'Set id to ' + exp, type: 'set', path: ['id'], value: exp }]);
    checkTags(m.tags, ['tags'], add);
    checkSkip(m.skip, ['skip'], add, index);
    checkProfiles(m.profiles, add, ctx);
    var seen = {};
    (Array.isArray(m.tests) ? m.tests : []).forEach(function (t, i) {
      var tp = ['tests', i];
      if (!isObj(t)) { add('error', 'shape', 'Each test is a mapping with an id', tp); return; }
      unknownKeys(t, 'test', tp, add, index);
      if (!t.id) add('error', 'test-id', 'This test needs an id', tp.concat('id'), t.name ? [{ label: 'Use ' + camelOf(t.name), type: 'set', path: tp.concat('id'), value: camelOf(t.name) }] : []);
      else if (seen[t.id] !== undefined) add('error', 'dup-id', 'Duplicate test id "' + t.id + '"; ids must be unique in the file', tp.concat('id'), [{ label: 'Rename to ' + uniqueId(t.id, Object.keys(seen)), type: 'set', path: tp.concat('id'), value: uniqueId(t.id, Object.keys(seen)) }]);
      else idRule(t.id, tp.concat('id'), 'Test', add);
      if (t.id) seen[t.id] = i;
      checkTags(t.tags, tp.concat('tags'), add);
      checkSkip(t.skip, tp.concat('skip'), add, index);
      if (Array.isArray(t.steps)) { var sids = {}; t.steps.forEach(function (s, si) { if (isObj(s) && s.id) { if (sids[s.id]) add('error', 'dup-id', 'Duplicate step id "' + s.id + '"', tp.concat('steps', si, 'id')); sids[s.id] = 1; } }); }
      if (t.repeat !== undefined && !(typeof t.repeat === 'number' && t.repeat >= 1 && Math.floor(t.repeat) === t.repeat)) add('error', 'repeat', 'repeat must be a whole number of 1 or more', tp.concat('repeat'));
      if (isObj(t.retry)) unknownKeys(t.retry, 'retry', tp.concat('retry'), add, index);
      if (t.auth !== undefined) checkAuthRef(t.auth, tp.concat('auth'), add, ctx, index);
    });
    sitesFor('test', m, ctx).forEach(function (site) {
      if (site.kind === 'vars') { checkTemplates({ path: ['vars'], a: m.vars, scope: site.scope, own: [], kind: 'vars', ids: [] }, fpath, ctx, add, index, []); return; }
      checkActionSite(site, fpath, ctx, add, index);
    });
  }
  if (kind === 'api') {
    unknownKeys(m, 'apiFile', [], add, index);
    var ex2 = expectedApiFileId(fpath);
    if (ex2 && m.id !== ex2) add('warn', 'file-id', 'Api file id should be "' + ex2 + '", the service folder name', ['id'], [{ label: 'Set id to ' + ex2, type: 'set', path: ['id'], value: ex2 }]);
    if (m.auth !== undefined) checkAuthRef(m.auth, ['auth'], add, ctx, index);
    checkProfiles(m.profiles, add, ctx);
    var ids = {}, fid = m.id || ex2 || '';
    (Array.isArray(m.apis) ? m.apis : []).forEach(function (a, i) {
      var ap = ['apis', i];
      if (!isObj(a)) { add('error', 'shape', 'Each api is a mapping with an id', ap); return; }
      unknownKeys(a, 'api', ap, add, index);
      if (!a.id) add('error', 'api-id', 'This api needs an id', ap.concat('id'));
      else if (ids[a.id]) add('error', 'dup-id', 'Duplicate api id "' + a.id + '"', ap.concat('id'), [{ label: 'Rename to ' + uniqueId(a.id, Object.keys(ids)), type: 'set', path: ap.concat('id'), value: uniqueId(a.id, Object.keys(ids)) }]);
      else idRule(a.id, ap.concat('id'), 'Api', add, function () {
        if (fid && a.id.toLowerCase().indexOf(fid.toLowerCase()) === 0 && a.id.length > fid.length && /[A-Z]/.test(a.id[fid.length])) {
          var to = a.id[fid.length].toLowerCase() + a.id.slice(fid.length + 1);
          add('warn', 'id-prefix', 'Api id "' + a.id + '" repeats the service name; the file id already provides it (' + fid + '.' + to + ')', ap.concat('id'), [{ label: 'Rename to ' + to, type: 'rename', path: ap.concat('id'), from: a.id, to: to }]);
        }
      });
      if (a.id) ids[a.id] = 1;
      if (a.method !== undefined && METHODS.indexOf(String(a.method).toUpperCase()) < 0) { var sm = suggest(String(a.method).toUpperCase(), METHODS); add('error', 'method', 'Unknown method "' + a.method + '"', ap.concat('method'), sm ? [{ label: 'Use ' + sm, type: 'set', path: ap.concat('method'), value: sm }] : []); }
      if (typeof a.path === 'string' && a.path[0] !== '/') add('warn', 'path', 'The path should start with "/"', ap.concat('path'), [{ label: 'Use /' + a.path, type: 'set', path: ap.concat('path'), value: '/' + a.path }]);
      if (a.auth !== undefined) checkAuthRef(a.auth, ap.concat('auth'), add, ctx, index);
      if (isObj(a.cleanup) && typeof a.cleanup.call === 'string') {
        var local = (m.apis || []).filter(isObj).map(function (x) { return x.id; });
        if (local.indexOf(a.cleanup.call) < 0 && !ctx.apiById[a.cleanup.call]) {
          var sc = suggest(a.cleanup.call, local), lc = lineOf(index, ap.concat('cleanup', 'call'));
          add('error', 'unknown-api', 'Cleanup calls unknown api "' + a.cleanup.call + '"' + (sc ? '; did you mean ' + sc + '?' : ''), ap.concat('cleanup', 'call'), sc ? [{ label: 'Use ' + sc, type: 'text', line: lc, find: a.cleanup.call, replace: sc }] : [], lc);
        }
      }
    });
  }
  if (kind === 'config') {
    unknownKeys(m, 'config', [], add, index);
    var prs = Array.isArray(m.profiles) ? m.profiles : [];
    if (!prs.length) add('error', 'profiles', 'A project needs at least one profile; vc init creates "default"', ['profiles'], [{ label: 'Add profile default', type: 'set', path: ['profiles'], value: [{ id: 'default', 'default': true }] }]);
    var defs = prs.filter(function (p) { return isObj(p) && p['default'] === true; });
    if (prs.length && !defs.length) add('warn', 'profiles', 'No profile is marked default; runs must pass --profile', ['profiles']);
    if (defs.length > 1) add('error', 'profiles', 'Only one profile can be the default', ['profiles', prs.indexOf(defs[1]), 'default']);
    prs.forEach(function (p, i) { if (isObj(p) && typeof p.id === 'string' && !/^[a-z][a-z0-9]*$/.test(p.id)) add('warn', 'profile-id', 'Profile ids are lowercase single words', ['profiles', i, 'id'], [{ label: 'Rename to ' + words(p.id).join(''), type: 'set', path: ['profiles', i, 'id'], value: words(p.id).join('') }]); });
    (Array.isArray(m.auth) ? m.auth : []).forEach(function (a, i) {
      if (!isObj(a)) return;
      idRule(a.id, ['auth', i, 'id'], 'Auth', add);
      if (AUTH_TYPES.indexOf(a.type) < 0) { var st = suggest(String(a.type), AUTH_TYPES); add('error', 'auth-type', 'Unknown auth type "' + a.type + '"', ['auth', i, 'type'], st ? [{ label: 'Use ' + st, type: 'set', path: ['auth', i, 'type'], value: st }] : []); }
    });
    if (isObj(m.reports) && Array.isArray(m.reports.formats)) m.reports.formats.forEach(function (f, i) { if (REPORT_FORMATS.indexOf(f) < 0) add('error', 'report', 'Unknown report format "' + f + '"', ['reports', 'formats', i]); });
    if (isObj(m.secrets) && m.secrets.provider !== undefined && ['varlock', 'env'].indexOf(m.secrets.provider) < 0) add('error', 'secrets', 'secrets.provider is varlock or env', ['secrets', 'provider']);
    sitesFor('config', m, ctx).forEach(function (site) { checkActionSite(site, fpath, ctx, add, index); });
  }
  if (kind === 'load') {
    unknownKeys(m, 'load', [], add, index);
    if (typeof m.id === 'string' && !/^[a-z][a-zA-Z0-9]*\.[a-z][a-zA-Z0-9]*$/.test(m.id)) add('warn', 'file-id', 'Load ids look like <area>.<scenario>, for example checkout.peak', ['id']);
    (Array.isArray(m.mix) ? m.mix : []).forEach(function (x, i) {
      if (!isObj(x)) return;
      unknownKeys(x, 'mix', ['mix', i], add, index);
      if (typeof x.test === 'string' && !ctx.testById[x.test]) { var s = suggest(x.test, ctx.tests.map(function (t) { return t.full; })), lt = lineOf(index, ['mix', i, 'test']); add('error', 'unknown-test', 'Unknown test "' + x.test + '"' + (s ? '; did you mean ' + s + '?' : ''), ['mix', i, 'test'], s ? [{ label: 'Use ' + s, type: 'text', line: lt, find: x.test, replace: s }] : [], lt); }
      if (!(typeof x.weight === 'number' && x.weight > 0 && Math.floor(x.weight) === x.weight)) add('error', 'weight', 'weight must be a whole number above 0', ['mix', i, 'weight']);
    });
    if (!Array.isArray(m.mix) || !m.mix.length) add('error', 'mix', 'A load file needs at least one test in mix', ['mix']);
    (Array.isArray(m.stages) ? m.stages : []).forEach(function (s, i) {
      if (!isObj(s)) return;
      if (!/^\d+(\.\d+)?(ms|s|m|h)$/.test(String(s.duration))) add('error', 'duration', 'Stage duration looks like 30s, 5m or 1h', ['stages', i, 'duration']);
      if (!(typeof s.users === 'number' && s.users >= 0)) add('error', 'users', 'users must be a number of 0 or more', ['stages', i, 'users']);
    });
    if (isObj(m.thresholds)) Object.keys(m.thresholds).forEach(function (k) { if (LOAD_METRICS.indexOf(k) < 0) { var sm = suggest(k, LOAD_METRICS); add('error', 'metric', 'Unknown metric "' + k + '"', ['thresholds', k], sm ? [{ label: 'Rename to ' + sm, type: 'renameKey', path: ['thresholds'], from: k, to: sm }] : []); } });
    if (m.checks !== undefined && ['api', 'full'].indexOf(m.checks) < 0) add('error', 'checks', 'checks is api or full', ['checks'], [{ label: 'Use api', type: 'set', path: ['checks'], value: 'api' }]);
  }
  walkExpects(m, [], function (e, p) { checkExpect(e, p, fpath, ctx, add, index); });
  return D;
}
function uniqueId(id, taken) { var n = 2; while (taken.indexOf(id + n) >= 0) n++; return id + n; }
function checkTags(tags, path, add) {
  if (tags === undefined) return;
  if (!Array.isArray(tags)) { add('error', 'tags', 'tags is a list, like [orders, smoke]', path); return; }
  tags.forEach(function (t, i) {
    if (typeof t !== 'string' || /^[a-z0-9]+(-[a-z0-9]+)*$/.test(t)) return;
    var to = kebabOf(t);
    add('warn', 'tag', 'Tag "' + t + '" should be lowercase kebab-case' + (to ? ' ("' + to + '")' : ''), path.concat(i), to ? [{ label: 'Use ' + to, type: 'set', path: path.concat(i), value: to }] : []);
  });
}
function checkProfiles(prs, add, ctx) {
  if (!Array.isArray(prs)) return;
  var ids = ctx.profiles.map(function (p) { return p.id; });
  prs.forEach(function (p, i) {
    if (!isObj(p)) return;
    if (ids.indexOf(p.id) < 0) { var s = suggest(String(p.id), ids); add('error', 'profile', 'Profile "' + p.id + '" is not defined in vibranium.config.yaml', ['profiles', i, 'id'], s ? [{ label: 'Use ' + s, type: 'set', path: ['profiles', i, 'id'], value: s }] : []); }
  });
}
function checkAuthRef(v, path, add, ctx, index) {
  var id = isObj(v) ? v.id : v, ok = ['none', 'inherit'].concat(ctx.authIds);
  if (typeof id !== 'string' || ok.indexOf(id) >= 0) return;
  var s = suggest(id, ok), p = isObj(v) ? path.concat('id') : path, ln = lineOf(index, p);
  add('error', 'auth', 'Unknown auth "' + id + '"' + (s ? '; did you mean ' + s + '?' : ''), p, s ? [{ label: 'Use ' + s, type: 'text', line: ln, find: id, replace: s }] : [], ln);
}
function renameRefs(v, from, to) {
  if (typeof v === 'string') return v.split('${' + from + '.').join('${' + to + '.');
  if (Array.isArray(v)) return v.map(function (x) { return renameRefs(x, from, to); });
  if (isObj(v)) { var o = {}; Object.keys(v).forEach(function (k) { o[k] = renameRefs(v[k], from, to); }); return o; }
  return v;
}
function applyFix(text, kind, fix) {
  if (fix.type === 'text') {
    var lines = String(text).split('\n'), l = lines[fix.line];
    if (l === undefined || l.indexOf(fix.find) < 0) return text;
    lines[fix.line] = l.replace(fix.find, fix.replace);
    return lines.join('\n');
  }
  var P = yParseSafe(text);
  if (!P.ok) return text;
  var m = clone(P.value);
  if (fix.type === 'set') setIn(m, fix.path, clone(fix.value));
  else if (fix.type === 'del') setIn(m, fix.path, undefined);
  else if (fix.type === 'addVar') { if (!isObj(getIn(m, fix.base))) setIn(m, fix.base, {}); setIn(m, fix.base.concat(fix.name), ''); }
  else if (fix.type === 'rename') { setIn(m, fix.path, fix.to); m = renameRefs(m, fix.from, fix.to); }
  else if (fix.type === 'renameKey') { if (!fix.path.length) m = renameKey(m, fix.from, fix.to); else setIn(m, fix.path, renameKey(getIn(m, fix.path), fix.from, fix.to)); }
  else return text;
  return yApplyModel(text, kind, m);
}

// ===================== completion =====================
function filterItems(items, partial, max) {
  var p = String(partial || '').toLowerCase();
  var a = items.filter(function (it) { return it.label.toLowerCase().indexOf(p) === 0; });
  var b = items.filter(function (it) { return a.indexOf(it) < 0 && (it.label.toLowerCase().indexOf(p) > 0 || (it.detail || '').toLowerCase().indexOf(p) >= 0); });
  return a.concat(b).slice(0, max || 12);
}
function apiItems(ctx) { return ctx.apis.map(function (a) { return { label: a.full, detail: a.method + ' ' + a.path, file: a.file, method: a.method, insert: a.full }; }); }
function testItems(ctx) { return ctx.tests.map(function (t) { return { label: t.full, detail: t.name, file: t.file, insert: t.full }; }); }
function hookItems(ctx, fpath) {
  var out = [];
  ctx.hooks.forEach(function (h) { h.exports.forEach(function (e) { var ref = (fpath === 'vibranium.config.yaml' ? h.path : relPath(fpath, h.path)) + '#' + e.name; out.push({ label: e.name, detail: h.path, insert: ref }); }); });
  return out;
}
function scopeItems(scope, ctx) {
  var seen = {}, items = [];
  scope.slice().reverse().forEach(function (s) {
    if (seen[s.name]) return; seen[s.name] = 1;
    items.push({ label: s.name, detail: s.src, insert: s.name, close: true });
  });
  scope.forEach(function (s) {
    if (!s.shared) return;
    var src = s.src.split(' · ')[1] || '', api = ctx.apiById[src];
    if (api) api.saves.forEach(function (f) { items.push({ label: s.name + '.' + f, detail: 'saved by ' + api.full + ', shared', insert: s.name + '.' + f, close: true }); });
  });
  return items;
}
function tplCompletion(before, env) {
  var m;
  if ((m = /\$\{\s*var\.([\w.\[\]*]*)$/.exec(before))) return { from: before.length - m[1].length, items: filterItems(scopeItems(env.scope || [], env.ctx), m[1], 40), kind: 'var' };
  if ((m = /\$\{\s*dataset\.(\w*)$/.exec(before))) return { from: before.length - m[1].length, items: filterItems(env.ctx.datasets.map(function (d) { return { label: d.id, detail: d.path + ' · ' + d.rows + ' rows', insert: d.id, close: true }; }), m[1]) };
  if ((m = /\$\{\s*fake\.([\w.]*)$/.exec(before))) return { from: before.length - m[1].length, items: filterItems(FAKES.map(function (f) { return { label: f, detail: 'generated value', insert: f, close: true }; }), m[1]) };
  if ((m = /\$\{\s*example\.(\w*)$/.exec(before))) return { from: before.length - m[1].length, items: filterItems((Array.isArray(env.ex) ? env.ex : []).map(function (k) { return { label: k, detail: 'examples column', insert: k, close: true }; }), m[1]) };
  if ((m = /\$\{\s*response\.([\w.]*)$/.exec(before))) return { from: before.length - m[1].length, items: filterItems(['status', 'body.', 'headers.', 'timing.total'].map(function (k) { return { label: k, detail: 'from this response', insert: k, close: k.slice(-1) !== '.', reopen: k.slice(-1) === '.' }; }), m[1]) };
  if ((m = /\$\{\s*([A-Za-z]*)$/.exec(before))) {
    var ns = [
      { label: 'var.', detail: 'a var in scope', insert: 'var.', reopen: true },
      { label: 'response.', detail: 'the current response', insert: 'response.', reopen: true },
      { label: 'example.', detail: 'a column of examples', insert: 'example.', reopen: true },
      { label: 'item', detail: 'the forEach element', insert: 'item', close: true },
      { label: 'iteration.index', detail: 'loop counter from 1', insert: 'iteration.index', close: true },
      { label: 'fake.', detail: 'generated data', insert: 'fake.', reopen: true },
      { label: 'dataset.', detail: 'rows from datasets/', insert: 'dataset.', reopen: true },
      { label: 'secret.', detail: 'from .env.schema, redacted', insert: 'secret.' },
      { label: 'env.', detail: 'operating-system environment', insert: 'env.' },
      { label: 'run.id', detail: 'this run', insert: 'run.id', close: true }
    ];
    (env.ids || []).forEach(function (id) { ns.push({ label: id + '.', detail: 'result of step ' + id, insert: id + '.body.' }); });
    return { from: before.length - m[1].length, items: filterItems(ns, m[1], 20) };
  }
  return null;
}
function fieldCompletion(value, caret, mode, env) {
  var before = String(value).slice(0, caret);
  if (mode === 'call') return { from: 0, to: String(value).length, items: filterItems(apiItems(env.ctx), before.trim()), whole: true };
  if (mode === 'test') return { from: 0, to: String(value).length, items: filterItems(testItems(env.ctx), before.trim()), whole: true };
  if (mode === 'localApi') return { from: 0, to: String(value).length, items: filterItems((env.local || []).map(function (id) { return { label: id, detail: 'api in this file', insert: id }; }), before.trim()), whole: true };
  if (mode === 'fn') return { from: 0, to: String(value).length, items: filterItems(hookItems(env.ctx, env.fpath), before.replace(/^.*#/, '')), whole: true };
  var r = tplCompletion(before, env);
  if (r) r.to = caret;
  return r;
}
function textAncestors(lines, lineNo, col) {
  var chain = [], ind = col;
  for (var i = lineNo - 1; i >= 0 && ind > 0; i--) {
    var l = lines[i];
    if (!l.trim() || /^\s*#/.test(l)) continue;
    var li = /^ */.exec(l)[0].length, c = l.slice(li).replace(/\s+$/, '');
    if (isSeqLine(c)) {
      var off = 1; while (c[off] === ' ') off++;
      var kc = li + off, e = mapEntry(c.slice(off));
      if (e && kc < ind) { chain.unshift(e.key); ind = kc; }
      if (li < ind) { chain.unshift('#'); ind = li; }
      continue;
    }
    if (li < ind) { var e2 = mapEntry(c); if (e2) chain.unshift(e2.key); ind = li; }
  }
  return chain;
}
function pathAtLine(index, lineNo) {
  var best = null, bl = -1;
  Object.keys(index || {}).forEach(function (k) {
    if (!k) return;
    var nd = index[k], end = nd.end !== undefined ? nd.end : nd.line;
    if (nd.line <= lineNo && end >= lineNo) {
      var p = k.split(PK).map(function (s) { return /^\d+$/.test(s) ? +s : s; });
      if (p.length > bl) { bl = p.length; best = p; }
    }
  });
  return best || [];
}
function yamlCompletion(text, caret, env) {
  var ls = text.lastIndexOf('\n', caret - 1) + 1, before = text.slice(ls, caret), lineNo = text.slice(0, ls).split('\n').length - 1, m;
  var fpath = env.fpath, ctx = env.ctx, kind = env.kind;
  if (before.indexOf('${') >= 0 && /\$\{[^}]*$/.test(before)) {
    var cp = pathAtLine(env.index, lineNo), scope = env.model ? scopeForPath(kind, env.model, ctx, cp) : baseScope(ctx);
    var site = env.model ? siteAt(sitesFor(kind, env.model, ctx), cp) : null;
    var r = tplCompletion(before, { scope: scope, ctx: ctx, ex: site ? site.ex : null, ids: site ? site.ids : [] });
    if (r) { r.from += ls; r.to = caret; }
    return r;
  }
  function val(re, items) { var mm = re.exec(before); if (!mm) return null; var part = mm[mm.length - 1]; return { from: caret - part.length, to: caret, items: filterItems(items(), part) }; }
  var lines = text.split('\n');
  var chainHere = function () { var ind = /^\s*/.exec(before)[0].length; return textAncestors(lines, lineNo, ind); };
  var r2 = null;
  if (/(?:^|\s)call:\s*[\w.]*$/.test(before)) {
    var ch = chainHere();
    if (kind === 'api' && ch[ch.length - 1] === 'cleanup') r2 = val(/call:\s*([\w.]*)$/, function () { var mdl = env.model || {}; return (mdl.apis || []).filter(isObj).map(function (a) { return { label: a.id, detail: (a.method || 'GET') + ' ' + (a.path || ''), insert: a.id }; }); });
    else r2 = val(/call:\s*([\w.]*)$/, function () { return apiItems(ctx); });
  }
  r2 = r2 || val(/(?:^|\s)test:\s*([\w.]*)$/, function () { return testItems(ctx); });
  r2 = r2 || val(/function:\s*\S*#?([\w$]*)$/, function () { return hookItems(ctx, fpath); });
  if (r2 && /function:/.test(before)) { r2.from = ls + before.indexOf('function:') + 9; while (text[r2.from] === ' ') r2.from++; }
  r2 = r2 || val(/(?:^|\s)auth:\s*(\w*)$/, function () { return [{ label: 'none', detail: 'send no credentials', insert: 'none' }].concat(ctx.authDefs.map(function (a) { return { label: a.id, detail: a.type, insert: a.id }; })); });
  r2 = r2 || val(/\$file:\s*([\w.\/-]*)$/, function () { return ctx.schemas.concat(ctx.datasets.map(function (d) { return d.path; })).map(function (p) { return { label: relPath(fpath, p), detail: p, insert: relPath(fpath, p) }; }); });
  r2 = r2 || val(/(?:^|\s)method:\s*(\w*)$/, function () { return METHODS.map(function (x) { return { label: x, detail: 'HTTP method', insert: x }; }); });
  r2 = r2 || val(/(?:^|\s)checks:\s*(\w*)$/, function () { return [{ label: 'api', detail: "evaluate only the apis' expect", insert: 'api' }, { label: 'full', detail: "evaluate every test's expect", insert: 'full' }]; });
  r2 = r2 || val(/(?:^|\s)format:\s*(\w*)$/, function () { return ['email', 'uuid', 'uri', 'date', 'dateTime', 'ipv4'].map(function (x) { return { label: x, detail: 'format matcher', insert: x }; }); });
  r2 = r2 || val(/(?:^|\s)type:\s*(\w*)$/, function () { var c2 = chainHere(); if (kind === 'config' && c2[0] === 'auth') return AUTH_TYPES.map(function (x) { return { label: x, detail: 'auth type', insert: x }; }); return ['string', 'number', 'integer', 'boolean', 'object', 'array', 'null'].map(function (x) { return { label: x, detail: 'type matcher', insert: x }; }); });
  if (r2) return r2.items.length ? r2 : null;
  if ((m = /^(\s*)(-\s+)?([A-Za-z$][\w$]*)?$/.exec(before)) && (m[3] || env.force)) {
    var col = m[1].length + (m[2] ? m[2].length : 0), chain = textAncestors(lines, lineNo, m[2] ? m[1].length : col);
    if (m[2]) chain.push('#');
    var part = m[3] || '', items = [];
    var last = chain[chain.length - 1], prev = chain[chain.length - 2];
    if (prev === 'match' && last !== '#') items = MATCHERS.map(function (x) { return { label: x, detail: 'matcher', insert: x + ': ' }; });
    else if (last === 'match') items = [];
    else if (last === 'timing') items = TIMING.map(function (x) { return { label: x, detail: 'timing phase', insert: x + ':' }; });
    else {
      var sh = shapeOf(kind, chain.map(function (x) { return x === '#' ? 0 : x; }));
      if (sh && KEYS[sh]) items = KEYS[sh].map(function (k) { return { label: k, detail: sh + ' key', insert: k + ': ' }; });
    }
    var fl = filterItems(items, part);
    if (fl.length === 1 && fl[0].label === part) return null;
    return fl.length ? { from: caret - part.length, to: caret, items: fl } : null;
  }
  return null;
}
