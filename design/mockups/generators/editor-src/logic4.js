// ===================== new-item planning (naming conventions) =====================
function servicesOf(files) {
  var out = [];
  Object.keys(files).forEach(function (p) { var m = /^services\/([^\/]+)\//.exec(p); if (m && out.indexOf(m[1]) < 0) out.push(m[1]); });
  return out;
}
function planNew(nw, files, ctx) {
  var k = nw.kind, plan = { kind: k, errors: [], notes: [], path: '', id: '', exists: false, newFile: false, item: false };
  var svc = nw.service === '+new' ? words(nw.newService).join('') : nw.service;
  var derived = '';
  if (k === 'test' || k === 'api' || k === 'hook') derived = camelOf(nw.name);
  if (k === 'dataset') derived = camelOf(nw.name);
  plan.derived = derived;
  var id = nw.idEdited ? String(nw.id || '') : derived;
  plan.id = id;
  function checkCamel(what) {
    if (!id) { plan.errors.push('Type a name to derive the ' + what + ' id'); return false; }
    if (!isCamel(id)) { plan.errors.push('"' + id + '" is not a valid id: ids are camelCase, letters and digits, starting with a lowercase letter'); plan.sugg = camelOf(id) || null; return false; }
    return true;
  }
  if (k === 'test') {
    plan.item = true;
    var target = nw.file;
    if (target === '+new') {
      var area = words(nw.area).join('.');
      if (!svc) plan.errors.push('Pick or name a service');
      if (!area) plan.errors.push('Name the area, for example checkout');
      target = svc && area ? 'services/' + svc + '/' + area + '.test.yaml' : '';
      plan.fileId = svc && area ? svc + '.' + area : '';
      plan.newFile = true;
      if (target && files[target]) { plan.errors.push(target + ' already exists; pick it from the list instead'); }
    }
    plan.path = target;
    if (checkCamel('test') && target && files[target]) {
      var P = yParseSafe(files[target].text), tests = P.ok && isObj(P.value) && Array.isArray(P.value.tests) ? P.value.tests : [];
      if (tests.some(function (t) { return isObj(t) && t.id === id; })) { plan.errors.push(id + ' already exists in ' + target); plan.sugg = uniqueId(id, tests.map(function (t) { return t.id; })); }
      plan.fileId = P.ok && isObj(P.value) ? P.value.id : '';
    }
    if (!String(nw.name || '').trim()) plan.errors.push('Describe the behaviour as a sentence, for example "Rejects a declined card"');
    plan.full = (plan.fileId || '?') + '.' + (id || '?');
  }
  if (k === 'api') {
    plan.item = true;
    if (!svc) plan.errors.push('Pick or name a service');
    plan.path = svc ? 'services/' + svc + '/' + svc + '.api.yaml' : '';
    plan.newFile = !!plan.path && !files[plan.path];
    if (checkCamel('api') && plan.path && files[plan.path]) {
      var P2 = yParseSafe(files[plan.path].text), apis = P2.ok && isObj(P2.value) && Array.isArray(P2.value.apis) ? P2.value.apis : [];
      if (apis.some(function (a) { return isObj(a) && a.id === id; })) { plan.errors.push(id + ' already exists in ' + plan.path); plan.sugg = uniqueId(id, apis.map(function (a) { return a.id; })); }
    }
    if (id && svc && id.toLowerCase().indexOf(svc.toLowerCase()) === 0 && id.length > svc.length) { plan.notes.push('Drop the service prefix: the file id already provides it (' + svc + '.' + id[svc.length].toLowerCase() + id.slice(svc.length + 1) + ')'); }
    if (!/^\//.test(nw.apiPath || '')) plan.errors.push('The path starts with "/", for example /users/${var.userId}');
    plan.full = svc + '.' + id;
  }
  if (k === 'testFile') {
    var ar = words(nw.area).join('.');
    if (!svc) plan.errors.push('Pick or name a service');
    if (!ar) plan.errors.push('Name the area, for example checkout or history');
    plan.id = svc && ar ? svc + '.' + ar : '';
    plan.path = svc && ar ? 'services/' + svc + '/' + ar + '.test.yaml' : '';
    plan.newFile = true;
  }
  if (k === 'load') {
    var la = words(nw.area).join(''), sc = camelOf(nw.scenario);
    if (!la) plan.errors.push('Name the area, for example checkout');
    if (!sc) plan.errors.push('Name the scenario, for example peak');
    plan.id = la && sc ? la + '.' + sc : '';
    plan.path = la ? 'load/' + la + '.load.yaml' : '';
    plan.newFile = true;
  }
  if (k === 'schema') {
    var th = kebabOf(nw.name);
    if (!svc) plan.errors.push('Pick or name a service');
    if (!th) plan.errors.push('Name the thing it describes, for example order or line item');
    plan.path = svc && th ? 'schemas/' + svc + '/' + th + '.json' : '';
    plan.id = plan.path;
    plan.newFile = true;
  }
  if (k === 'dataset') {
    var st = kebabOf(nw.name);
    if (!st) plan.errors.push('Name the dataset, for example world cities');
    plan.path = st ? 'datasets/' + st + '.csv' : '';
    plan.id = camelOf(st);
    plan.newFile = true;
    if (!String(nw.columns || '').trim()) plan.errors.push('List the columns, separated by commas');
  }
  if (k === 'hook') {
    checkCamel('export');
    var hf = nw.hookFile;
    if (hf === '+new') { var pu = kebabOf(nw.purpose); if (!pu) plan.errors.push('Name the purpose of the new file, for example sign or checks'); hf = pu ? 'hooks/' + pu + '.ts' : ''; plan.newFile = true; }
    plan.path = hf;
    if (hf && files[hf] && hookExports(files[hf].text).some(function (e) { return e.name === id; })) { plan.errors.push(id + ' is already exported from ' + hf); plan.sugg = uniqueId(id, hookExports(files[hf].text).map(function (e) { return e.name; })); }
    plan.item = !plan.newFile;
    plan.full = hf + '#' + id;
  }
  if (plan.newFile && plan.path && files[plan.path] && k !== 'test') plan.errors.push(plan.path + ' already exists');
  plan.ok = plan.errors.length === 0;
  return plan;
}
function happyPathTest(id, name, apiFull, ctx) {
  var t = { id: id, name: name };
  if (!apiFull || !ctx.apiById[apiFull]) return t;
  var api = ctx.apiById[apiFull], before = [], have = Object.keys(ctx.configVars);
  api.needs.forEach(function (nm) {
    if (have.indexOf(nm) >= 0) return;
    var saver = ctx.apis.filter(function (a) { return a.fileId === api.fileId && a.saves.indexOf(nm) >= 0; })[0] || ctx.apis.filter(function (a) { return a.saves.indexOf(nm) >= 0; })[0];
    if (saver && !before.some(function (b) { return b.call === saver.full; })) before.push({ call: saver.full });
  });
  if (before.length) t.before = before;
  t.call = apiFull;
  var st = api.status !== undefined ? api.status : api.method === 'POST' ? 201 : api.method === 'DELETE' ? 204 : 200;
  t.expect = { status: Array.isArray(st) ? st[0] : st };
  return t;
}
function newFileText(plan, nw, ctx) {
  var k = plan.kind;
  if (k === 'testFile') return yDump({ id: plan.id, name: String(nw.name || '').trim() || plan.id, tags: [plan.id.split('.')[0]], tests: [] }, 'test');
  if (k === 'load') return yDump({ id: plan.id, name: String(nw.name || '').trim() || plan.id, mix: [], stages: [{ duration: '1m', users: 10 }], checks: 'api' }, 'load');
  if (k === 'schema') return JSON.stringify({ $schema: 'https://json-schema.org/draft/2020-12/schema', title: String(nw.name || '').trim(), type: 'object', required: [], properties: {} }, null, 2) + '\n';
  if (k === 'dataset') return csvStringify({ cols: String(nw.columns).split(',').map(function (c) { return c.trim(); }).filter(Boolean), rows: [] });
  if (k === 'hook') return 'import type { CustomCheck } from "@vibranium/sdk";\n\n' + hookStub(plan.id);
  return '';
}
function hookStub(id) { return 'export const ' + id + ': CustomCheck = ({ response }) => {\n  return response.status < 500 || "server error";\n};\n'; }

// ===================== simulated run =====================
function simulateRun(fpath, m, sel, diags, profile) {
  var errs = diags.filter(function (d) { return d.sev === 'error'; }).length;
  if (errs) return { status: 'blocked', title: 'vc check failed with ' + errs + ' error' + (errs > 1 ? 's' : '') + '; nothing was run', lines: [] };
  var tests = isObj(m) && Array.isArray(m.tests) ? m.tests : [], idxs = [];
  var ti = /^test:(\d+)$/.exec(sel || '');
  if (ti) idxs = [+ti[1]]; else idxs = tests.map(function (t, i) { return i; });
  var res = idxs.map(function (i) { return runOne(tests[i], profile); }).filter(Boolean);
  if (!res.length) return { status: 'skipped', title: 'No tests to run', lines: [] };
  if (res.length === 1) return res[0];
  var c = { passed: 0, failed: 0, skipped: 0 };
  res.forEach(function (r) { c[r.status] = (c[r.status] || 0) + 1; });
  var tot = res.reduce(function (s, r) { return s + (r.ms || 0); }, 0);
  return { status: c.failed ? 'failed' : 'passed', title: (m.id || 'file') + ': ' + c.passed + ' passed, ' + c.failed + ' failed, ' + c.skipped + ' skipped in ' + (tot / 1000).toFixed(2) + ' s on ' + profile,
    lines: res.map(function (r) { return { ok: r.status === 'passed', skip: r.status === 'skipped', txt: r.title }; }) };
}
function runOne(t, profile) {
  if (!isObj(t)) return null;
  var id = t.id || 'test';
  if (t.skip && !(isObj(t.skip) && t.skip.until && String(t.skip.until) < TODAY)) return { status: 'skipped', title: id + ' skipped: ' + (isObj(t.skip) ? t.skip.reason : t.skip), lines: [], ms: 0 };
  var e = isObj(t.expect) ? t.expect : {}, lines = [], failed = false;
  var ms = 180 + (id.length * 37) % 900;
  if (e.status !== undefined) lines.push({ ok: true, txt: 'status ' + fmtStatus(e.status) });
  if (isObj(e.body) && e.body.schema) lines.push({ ok: true, txt: 'body matches ' + (isObj(e.body.schema) && e.body.schema.$file ? baseOf(String(e.body.schema.$file)) : 'the inline schema') });
  if (isObj(e.body) && isObj(e.body.match)) Object.keys(e.body.match).forEach(function (p) {
    var v = e.body.match[p];
    if (id === 'placesOrder' && p === 'total') { failed = true; lines.push({ ok: false, txt: 'body.match.total expected 59.98, got 64.78 (orders now add a tax line)' }); }
    else lines.push({ ok: true, txt: 'body.match.' + p + ' ' + (isObj(v) ? flowOf(v) : '= ' + cellText(v)) });
  });
  if (isObj(e.timing)) Object.keys(e.timing).forEach(function (k) { lines.push({ ok: true, txt: 'timing.' + k + ' ' + flowOf(e.timing[k]) + ' (took ' + Math.round(ms / 3) + 'ms)' }); });
  if (isObj(e.cel)) Object.keys(e.cel).forEach(function (k) { lines.push({ ok: !(id === 'placesOrder' && failed), txt: 'cel: ' + k }); });
  if (Array.isArray(e.custom)) e.custom.forEach(function (c) { if (isObj(c)) lines.push({ ok: true, txt: 'custom: ' + String(c['function']).split('#').pop() }); });
  if (!lines.length) lines.push({ ok: true, txt: 'the api\'s own checks' });
  if (id === 'placesOrder' && failed) ms = 1420;
  return { status: failed ? 'failed' : 'passed', title: id + (failed ? ' failed' : ' passed') + ' in ' + (ms / 1000).toFixed(2) + ' s on ' + profile, lines: lines, ms: ms };
}

// ===================== source highlighting =====================
function tokTemplates(s, base) {
  var out = [], re = /\$\{[^}]*\}?/g, last = 0, m;
  while ((m = re.exec(s))) { if (m.index > last) out.push({ t: s.slice(last, m.index), c: base }); out.push({ t: m[0], c: 'y-tpl' }); last = m.index + m[0].length; }
  if (last < s.length) out.push({ t: s.slice(last), c: base });
  return out;
}
function tokYamlValue(v) {
  var out = [], cm = v.search(/(^|\s)#/), com = '';
  if (cm >= 0 && !/^["']/.test(v)) { com = v.slice(cm); v = v.slice(0, cm); }
  var core = v.replace(/\s+$/, ''), trail = v.slice(core.length);
  if (/^["']/.test(core)) out.push({ t: core, c: 'y-str' });
  else if (/^[\[\]{},]/.test(core)) {
    var parts = core.split(/([\[\]{},:])/);
    parts.forEach(function (p) { if (!p) return; if (/^[\[\]{},:]$/.test(p)) out.push({ t: p, c: 'y-punc' }); else { var tr = p.trim(); var c = tr === '' ? '' : yScalar(tr) !== tr ? 'y-num' : 'y-str'; tokTemplates(p, c).forEach(function (x) { out.push(x); }); } });
  } else if (/^[|>][+-]?$/.test(core)) out.push({ t: core, c: 'y-punc' });
  else if (core) { var r = yScalar(core); tokTemplates(core, r !== core ? 'y-num' : 'y-str').forEach(function (x) { out.push(x); }); }
  if (trail) out.push({ t: trail, c: '' });
  if (com) out.push({ t: com, c: 'y-com' });
  return out;
}
function tokYaml(line) {
  var out = [], m = /^(\s*)(.*)$/.exec(line), ind = m[1], c = m[2];
  if (ind) out.push({ t: ind, c: '' });
  if (/^#/.test(c)) { out.push({ t: c, c: 'y-com' }); return out; }
  while (/^-(\s|$)/.test(c)) { var d = /^-\s*/.exec(c)[0]; out.push({ t: d, c: 'y-punc' }); c = c.slice(d.length); }
  var e = mapEntry(c);
  if (e) {
    out.push({ t: c.slice(0, e.colon), c: 'y-key' });
    out.push({ t: ':', c: 'y-punc' });
    var rest = c.slice(e.colon + 1);
    var sp0 = /^\s*/.exec(rest)[0];
    if (sp0) out.push({ t: sp0, c: '' });
    tokYamlValue(rest.slice(sp0.length)).forEach(function (x) { out.push(x); });
  } else if (c) tokYamlValue(c).forEach(function (x) { out.push(x); });
  return out;
}
function tokJson(line) {
  var out = [], re = /("(?:[^"\\]|\\.)*")(\s*:)?|(-?\d+(?:\.\d+)?(?:[eE][-+]?\d+)?|true|false|null)|([{}\[\],:])|(\s+)|([^\s"{}\[\],:]+)/g, m;
  while ((m = re.exec(line))) {
    if (m[1]) { out.push({ t: m[1], c: m[2] ? 'y-key' : 'y-str' }); if (m[2]) out.push({ t: m[2], c: 'y-punc' }); }
    else if (m[3]) out.push({ t: m[3], c: 'y-num' });
    else if (m[4]) out.push({ t: m[4], c: 'y-punc' });
    else out.push({ t: m[0], c: '' });
  }
  return out;
}
var TS_KW = /^(import|export|from|const|let|var|function|async|await|return|type|interface|new|if|else|of|in|for|true|false|null|undefined)$/;
function tokTs(line) {
  var out = [], re = /(\/\/.*$)|("(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|`(?:[^`\\]|\\.)*`)|([A-Za-z_$][\w$]*)|(\d+(?:\.\d+)?)|(\s+)|(.)/g, m;
  while ((m = re.exec(line))) {
    if (m[1]) out.push({ t: m[1], c: 'y-com' });
    else if (m[2]) out.push({ t: m[2], c: 'y-str' });
    else if (m[3]) out.push({ t: m[3], c: TS_KW.test(m[3]) ? 'y-key' : '' });
    else if (m[4]) out.push({ t: m[4], c: 'y-num' });
    else out.push({ t: m[0], c: m[6] && /[{}()\[\];,.:=<>]/.test(m[6]) ? 'y-punc' : '' });
  }
  return out;
}
function tokCsv(line, no) {
  var out = [];
  line.split(/(,)/).forEach(function (p) { if (p === ',') out.push({ t: p, c: 'y-punc' }); else if (p) out.push({ t: p, c: no === 1 ? 'y-key' : /^-?\d+(\.\d+)?$/.test(p) ? 'y-num' : '' }); });
  return out;
}
function tokLine(kind, line, no) {
  if (isYamlKind(kind)) return tokYaml(line);
  if (kind === 'schema') return tokJson(line);
  if (kind === 'hook') return tokTs(line);
  if (kind === 'dataset') return tokCsv(line, no);
  return [{ t: line, c: '' }];
}
function rampPoints(stages, W, H) {
  var t = 0, pts = [{ x: 0, y: 0 }], maxU = 1;
  function secs(d) { var m = /^(\d+(?:\.\d+)?)(ms|s|m|h)$/.exec(String(d)); if (!m) return 0; return +m[1] * { ms: 0.001, s: 1, m: 60, h: 3600 }[m[2]]; }
  (stages || []).forEach(function (s) { if (!isObj(s)) return; t += secs(s.duration); var u = typeof s.users === 'number' ? s.users : 0; pts.push({ x: t, y: u, d: s.duration }); if (u > maxU) maxU = u; });
  var T = t || 1;
  var P = pts.map(function (p) { return { x: Math.round(p.x / T * (W - 2)) + 1, y: Math.round(H - 4 - p.y / maxU * (H - 22)), u: p.y, d: p.d }; });
  return { pts: P.map(function (p) { return p.x + ',' + p.y; }).join(' '), area: '1,' + (H - 4) + ' ' + P.map(function (p) { return p.x + ',' + p.y; }).join(' ') + ' ' + P[P.length - 1].x + ',' + (H - 4), peaks: P.slice(1), total: t, maxU: maxU };
}
function fmtDur(s) { if (s >= 3600) return (s / 3600).toFixed(1).replace(/\.0$/, '') + ' h'; if (s >= 60) return (s / 60).toFixed(1).replace(/\.0$/, '') + ' min'; return s + ' s'; }
function fidOf(path) { return 'f-' + path.join('-').replace(/[^A-Za-z0-9_-]/g, '_'); }
