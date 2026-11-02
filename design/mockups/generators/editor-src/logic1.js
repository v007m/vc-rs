// ===================== pure logic: helpers =====================
var PK = '\u0001';
var TODAY = '2026-10-23';
function pk(path) { return path.join(PK); }
function isObj(v) { return v !== null && typeof v === 'object' && !Array.isArray(v); }
function isScalar(v) { return v === null || typeof v !== 'object'; }
function clone(v) { return v === undefined ? undefined : JSON.parse(JSON.stringify(v)); }
function deepEq(a, b) {
  if (a === b) return true;
  if (typeof a === 'number' && typeof b === 'number' && isNaN(a) && isNaN(b)) return true;
  if (Array.isArray(a)) {
    if (!Array.isArray(b) || a.length !== b.length) return false;
    for (var i = 0; i < a.length; i++) if (!deepEq(a[i], b[i])) return false;
    return true;
  }
  if (isObj(a)) {
    if (!isObj(b)) return false;
    var ka = Object.keys(a), kb = Object.keys(b);
    if (ka.length !== kb.length) return false;
    for (var j = 0; j < ka.length; j++) { if (!Object.prototype.hasOwnProperty.call(b, ka[j]) || !deepEq(a[ka[j]], b[ka[j]])) return false; }
    return true;
  }
  return false;
}
function sameOrderEq(a, b) {
  if (!deepEq(a, b)) return false;
  if (isObj(a)) { var ka = Object.keys(a), kb = Object.keys(b); if (ka.join(PK) !== kb.join(PK)) return false; return ka.every(function (k) { return sameOrderEq(a[k], b[k]); }); }
  if (Array.isArray(a)) return a.every(function (x, i) { return sameOrderEq(x, b[i]); });
  return true;
}
function getIn(o, path) {
  for (var i = 0; i < path.length; i++) { if (o == null || typeof o !== 'object') return undefined; o = o[path[i]]; }
  return o;
}
var KEEP_EMPTY = { tests: 1, apis: 1, mix: 1, stages: 1, profiles: 1 };
function setIn(o, path, v) {
  var cur = o;
  for (var i = 0; i < path.length - 1; i++) {
    var k = path[i];
    if (cur[k] == null || typeof cur[k] !== 'object') cur[k] = typeof path[i + 1] === 'number' ? [] : {};
    cur = cur[k];
  }
  var last = path[path.length - 1];
  if (v === undefined) {
    if (Array.isArray(cur)) cur.splice(last, 1); else delete cur[last];
    pruneUp(o, path.slice(0, -1));
  } else cur[last] = v;
}
function pruneUp(o, path) {
  for (var n = path.length; n > 0; n--) {
    var p = path.slice(0, n), v = getIn(o, p), k = p[p.length - 1];
    var empty = (isObj(v) && Object.keys(v).length === 0) || (Array.isArray(v) && v.length === 0 && !KEEP_EMPTY[k]);
    if (!empty) return;
    var par = getIn(o, p.slice(0, -1));
    if (Array.isArray(par)) return;
    delete par[k];
  }
}
function renameKey(obj, from, to) {
  var out = {};
  Object.keys(obj).forEach(function (k) { out[k === from ? to : k] = obj[k]; });
  return out;
}
function insertKeyAfter(obj, after, key, val) {
  var out = {}, done = false;
  Object.keys(obj).forEach(function (k) { out[k] = obj[k]; if (k === after) { out[key] = val; done = true; } });
  if (!done) out[key] = val;
  return out;
}
function words(s) {
  return String(s || '').replace(/([a-z0-9])([A-Z])/g, '$1 $2').toLowerCase().replace(/[^a-z0-9]+/g, ' ').trim().split(' ').filter(Boolean);
}
var STOPWORDS = ['a', 'an', 'the'];
function camelOf(s) {
  var w = words(s).filter(function (x) { return STOPWORDS.indexOf(x) < 0; });
  if (!w.length) return '';
  var out = w[0] + w.slice(1).map(function (x) { return x[0].toUpperCase() + x.slice(1); }).join('');
  if (/^[0-9]/.test(out)) out = 'n' + out;
  return out;
}
function kebabOf(s) { return words(s).join('-'); }
function isCamel(s) { return typeof s === 'string' && /^[a-z][a-zA-Z0-9]*$/.test(s); }
function lev(a, b) {
  a = String(a); b = String(b);
  var m = a.length, n = b.length, d = [], i, j;
  for (i = 0; i <= m; i++) { d[i] = [i]; }
  for (j = 0; j <= n; j++) d[0][j] = j;
  for (i = 1; i <= m; i++) for (j = 1; j <= n; j++) d[i][j] = Math.min(d[i - 1][j] + 1, d[i][j - 1] + 1, d[i - 1][j - 1] + (a[i - 1] === b[j - 1] ? 0 : 1));
  return d[m][n];
}
function suggest(name, cands) {
  var best = null, bd = 1e9, lname = String(name).toLowerCase();
  cands.forEach(function (c) {
    var d = lev(lname, String(c).toLowerCase());
    if (d < bd) { bd = d; best = c; }
  });
  if (best === null) return null;
  var lim = Math.max(2, Math.floor(String(name).length / 3));
  return bd <= lim ? best : null;
}
function dirOf(p) { var i = p.lastIndexOf('/'); return i < 0 ? '' : p.slice(0, i); }
function baseOf(p) { return p.slice(p.lastIndexOf('/') + 1); }
function resolvePath(fromFile, rel) {
  var parts = (dirOf(fromFile) ? dirOf(fromFile).split('/') : []).concat(String(rel).split('/'));
  var out = [];
  parts.forEach(function (s) { if (s === '..') out.pop(); else if (s !== '.' && s !== '') out.push(s); });
  return out.join('/');
}
function relPath(fromFile, target) {
  var a = dirOf(fromFile) ? dirOf(fromFile).split('/') : [], b = target.split('/');
  var i = 0; while (i < a.length && i < b.length - 1 && a[i] === b[i]) i++;
  var up = a.slice(i).map(function () { return '..'; });
  return up.concat(b.slice(i)).join('/');
}
function fileKind(path) {
  if (path === 'vibranium.config.yaml') return 'config';
  if (/^openapi\//.test(path)) return 'openapi';
  if (/\.api\.ya?ml$/.test(path)) return 'api';
  if (/\.test\.ya?ml$/.test(path)) return 'test';
  if (/\.load\.ya?ml$/.test(path)) return 'load';
  if (/\.json$/.test(path)) return 'schema';
  if (/\.csv$/.test(path)) return 'dataset';
  if (/\.(ts|js)$/.test(path)) return 'hook';
  return 'text';
}
function isYamlKind(k) { return k === 'config' || k === 'api' || k === 'test' || k === 'load' || k === 'openapi'; }

// ===================== YAML subset parser with line index =====================
function yScalar(s) {
  if (/^(?:null|Null|NULL|~)?$/.test(s)) return null;
  if (/^(?:true|True|TRUE)$/.test(s)) return true;
  if (/^(?:false|False|FALSE)$/.test(s)) return false;
  if (/^[-+]?[0-9]+$/.test(s)) return parseInt(s, 10);
  if (/^0o[0-7]+$/.test(s)) return parseInt(s.slice(2), 8);
  if (/^0x[0-9a-fA-F]+$/.test(s)) return parseInt(s.slice(2), 16);
  if (/^[-+]?(?:\.[0-9]+|[0-9]+(?:\.[0-9]*)?)(?:[eE][-+]?[0-9]+)?$/.test(s)) return parseFloat(s);
  if (/^[-+]?\.(?:inf|Inf|INF)$/.test(s)) return s[0] === '-' ? -Infinity : Infinity;
  if (/^\.(?:nan|NaN|NAN)$/.test(s)) return NaN;
  return s;
}
function YErr(msg, line) { var e = new Error(msg); e.yLine = line; return e; }
function scanQuoted(s, p) {
  var q = s[p], i = p + 1, out = '';
  if (q === "'") {
    while (i < s.length) {
      if (s[i] === "'") { if (s[i + 1] === "'") { out += "'"; i += 2; continue; } return { val: out, end: i + 1 }; }
      out += s[i++];
    }
    return null;
  }
  var ESC = { n: '\n', t: '\t', r: '\r', '"': '"', '\\': '\\', '/': '/', '0': '\0', b: '\b', f: '\f', ' ': ' ', e: '\x1b' };
  while (i < s.length) {
    var c = s[i];
    if (c === '\\') {
      var nx = s[i + 1];
      if (nx === 'u' || nx === 'x' || nx === 'U') {
        var len = nx === 'u' ? 4 : nx === 'x' ? 2 : 8;
        out += String.fromCodePoint(parseInt(s.substr(i + 2, len), 16)); i += 2 + len; continue;
      }
      if (ESC[nx] === undefined) throw YErr('unknown escape sequence', -1);
      out += ESC[nx]; i += 2; continue;
    }
    if (c === '"') return { val: out, end: i + 1 };
    out += c; i++;
  }
  return null;
}
function isSeqLine(c) { return c === '-' || c.slice(0, 2) === '- '; }
function mapEntry(c) {
  if (!c || c[0] === '#' || isSeqLine(c)) return null;
  var key, j;
  if (c[0] === '"' || c[0] === "'") {
    var q;
    try { q = scanQuoted(c, 0); } catch (e) { return null; }
    if (!q) return null;
    j = q.end; while (c[j] === ' ') j++;
    if (c[j] !== ':' || !(j + 1 === c.length || c[j + 1] === ' ')) return null;
    key = q.val;
  } else {
    if ('[{'.indexOf(c[0]) >= 0) return null;
    if ((c[0] === '?' || c[0] === ':') && (c.length === 1 || c[1] === ' ')) return null;
    j = -1;
    for (var k = 0; k < c.length; k++) {
      if (c[k] === ':' && (k + 1 === c.length || c[k + 1] === ' ')) { j = k; break; }
      if (c[k] === ' ' && c[k + 1] === '#') return null;
    }
    if (j < 0) return null;
    key = c.slice(0, j).replace(/\s+$/, '');
  }
  var ro = j + 1; while (c[ro] === ' ') ro++;
  return { key: key, rest: c.slice(ro), restOff: ro, colon: j };
}
function flowParse(s) {
  var p = 0;
  function ws() { while (p < s.length && (s[p] === ' ' || s[p] === '\t')) p++; }
  function INC() { throw { inc: true }; }
  function BAD(m) { throw { flow: m || 'missed comma between flow collection entries' }; }
  function val(ctx) {
    ws(); if (p >= s.length) INC();
    var ch = s[p];
    if (ch === '[') {
      p++; var a = []; ws(); if (p >= s.length) INC(); if (s[p] === ']') { p++; return a; }
      for (;;) {
        a.push(val('seq')); ws(); if (p >= s.length) INC();
        if (s[p] === ',') { p++; ws(); if (p >= s.length) INC(); if (s[p] === ']') { p++; return a; } continue; }
        if (s[p] === ']') { p++; return a; }
        BAD();
      }
    }
    if (ch === '{') {
      p++; var o = {}; ws(); if (p >= s.length) INC(); if (s[p] === '}') { p++; return o; }
      for (;;) {
        ws(); var k = val('key'); ws(); if (p >= s.length) INC();
        var v = null;
        if (s[p] === ':') { p++; ws(); if (p >= s.length) INC(); if (s[p] === ',' || s[p] === '}') v = null; else v = val('map'); ws(); }
        if (Object.prototype.hasOwnProperty.call(o, String(k))) BAD('duplicated mapping key');
        o[String(k)] = v; if (p >= s.length) INC();
        if (s[p] === ',') { p++; ws(); if (p >= s.length) INC(); if (s[p] === '}') { p++; return o; } continue; }
        if (s[p] === '}') { p++; return o; }
        BAD();
      }
    }
    if (ch === '"' || ch === "'") { var q = scanQuoted(s, p); if (!q) INC(); p = q.end; return q.val; }
    if (ch === ']' || ch === '}' || ch === ',') BAD('unexpected ' + ch);
    var st = p, depth = 0;
    while (p < s.length) {
      var c = s[p];
      if (c === '$' && s[p + 1] === '{') { depth++; p += 2; continue; }
      if (c === '}' && depth > 0) { depth--; p++; continue; }
      if (depth === 0 && (c === ',' || c === ']' || c === '}')) break;
      if (depth === 0 && c === ':' && (p + 1 >= s.length || ' ,]}'.indexOf(s[p + 1]) >= 0)) break;
      if (c === ' ' && s[p + 1] === '#') break;
      p++;
    }
    var t = s.slice(st, p).trim();
    return ctx === 'key' ? t : yScalar(t);
  }
  try { var v = val('top'); return { v: v, end: p }; } catch (e) { if (e && e.inc) return { incomplete: true }; throw e; }
}

function yParse(text) {
  var raw = String(text).split('\n'), n = raw.length, L = [], i;
  var index = {}, comments = {}, beforeOf = {}, usedBefore = {}, last = 0;
  function err(msg, li) { throw YErr(msg, li + 1); }
  for (i = 0; i < n; i++) {
    var r = raw[i].replace(/\r$/, '');
    var m = /^( *)(.*)$/.exec(r), c = m[2].replace(/\s+$/, ''), t = c.trim();
    L.push({ ind: m[1].length, c: c, raw: r, blank: t === '' || t === '---' || t === '...', com: t[0] === '#' });
    if (c[0] === '\t') err('tab characters must not be used in indentation', i);
  }
  var pend = [];
  for (i = 0; i < n; i++) {
    if (L[i].com) pend.push(i); else if (!L[i].blank) { if (pend.length) beforeOf[i] = pend; pend = []; }
  }
  function nextSig(j) { while (j < n && (L[j].blank || L[j].com)) j++; return j; }
  function record(path, li, info) {
    var key = pk(path);
    if (!index[key]) index[key] = { line: li, cstart: li };
    for (var k in info) index[key][k] = info[k];
    if (beforeOf[li] && !usedBefore[li]) {
      usedBefore[li] = true;
      comments[key] = comments[key] || {};
      comments[key].before = beforeOf[li].map(function (x) { return L[x].raw; });
      index[key].cstart = beforeOf[li][0];
    }
  }
  function setNode(path, info) { var key = pk(path); index[key] = index[key] || { line: info.vline, cstart: info.vline }; for (var k in info) index[key][k] = info[k]; }
  function setTrail(path, s) { if (!s || !/#/.test(s)) return; var key = pk(path); comments[key] = comments[key] || {}; comments[key].trailing = s; }

  function inlineValue(li, pind, col, s, path) {
    var ch = s[0];
    if (ch === '[' || ch === '{') {
      var buf = s, j = li, res;
      for (;;) {
        try { res = flowParse(buf); } catch (e) { if (e && e.flow) err(e.flow, j); throw e; }
        if (!res.incomplete) break;
        j++; while (j < n && L[j].blank) j++;
        if (j >= n) err('unexpected end of the stream within a flow collection', li);
        buf += ' ' + L[j].c.trim();
      }
      var after = buf.slice(res.end);
      if (after.trim() !== '' && !/^\s+#/.test(after)) err('unexpected characters after a flow collection', j);
      last = Math.max(last, j);
      setNode(path, { style: j === li ? 'flow' : 'flowml', vline: li, vs: col, ve: col + res.end });
      if (j === li) setTrail(path, after);
      return { v: res.v, next: j + 1 };
    }
    if (ch === '"' || ch === "'") {
      var q;
      try { q = scanQuoted(s, 0); } catch (e) { err('unknown escape sequence in a double quoted scalar', li); }
      if (!q) err('unexpected end of the stream within a ' + (ch === '"' ? 'double' : 'single') + ' quoted scalar', li);
      var aft = s.slice(q.end);
      if (aft.trim() !== '' && !/^\s+#/.test(aft)) err('unexpected characters after a quoted scalar', li);
      setNode(path, { style: ch === '"' ? 'dq' : 'sq', vline: li, vs: col, ve: col + q.end });
      setTrail(path, aft);
      return { v: q.val, next: li + 1 };
    }
    if (ch === '|' || ch === '>') {
      var hdr = /^([|>])([+-]?)\s*(#.*)?$/.exec(s);
      if (!hdr) err('invalid block scalar header', li);
      var k = li + 1, lines = [], bind = -1;
      while (k < n) {
        if (L[k].raw.trim() === '') { lines.push(''); k++; continue; }
        if (L[k].ind <= pind) break;
        if (bind < 0) bind = L[k].ind;
        if (L[k].ind < bind) break;
        lines.push(L[k].raw.slice(bind)); last = k; k++;
      }
      var trail = 0; while (lines.length && lines[lines.length - 1] === '') { lines.pop(); trail++; }
      var body;
      if (hdr[1] === '|') body = lines.join('\n');
      else { body = ''; lines.forEach(function (ln, x) { if (x === 0) body = ln; else if (ln === '' || lines[x - 1] === '') body += '\n' + ln; else body += ' ' + ln; }); }
      if (hdr[2] === '-') { /* strip */ } else if (hdr[2] === '+') body += '\n' + new Array(trail + 1).join('\n'); else if (lines.length) body += '\n';
      setNode(path, { style: 'block', vline: li });
      return { v: body, next: k };
    }
    if (ch === '&' || ch === '*' || ch === '!') err('anchors, aliases and tags are not supported here', li);
    if (ch === '@' || ch === '`') err('the character ' + ch + ' cannot start a plain value; put the value in quotes', li);
    var cm = s.search(/\s#/), txt = (cm >= 0 ? s.slice(0, cm) : s).replace(/\s+$/, '');
    if (/:\s/.test(txt) || /:$/.test(txt)) err('bad indentation of a mapping entry', li);
    var parts = [txt], kk = li + 1, ml = false;
    while (kk < n) {
      if (L[kk].blank || L[kk].com) break;
      if (L[kk].ind <= pind) break;
      if (mapEntry(L[kk].c)) err('bad indentation of a mapping entry', kk);
      parts.push(L[kk].c.trim()); last = kk; ml = true; kk++;
    }
    setNode(path, { style: ml ? 'plainml' : 'plain', vline: li, vs: col, ve: col + txt.length });
    if (!ml) setTrail(path, s.slice(txt.length));
    return { v: yScalar(parts.join(' ')), next: kk };
  }
  function valueAfterKey(li, ind, e, path) {
    var col = ind + e.restOff, rest = e.rest, after = ind + e.colon + 1;
    if (rest === '' || rest[0] === '#') {
      setTrail(path, L[li].raw.slice(after));
      setNode(path, { style: 'empty', vline: li, vs: after, ve: after });
      var j = nextSig(li + 1);
      if (j < n && L[j].ind > ind) { index[pk(path)].style = 'blockv'; return parseNode(j, L[j].ind, path, ind); }
      if (j < n && L[j].ind === ind && isSeqLine(L[j].c)) { index[pk(path)].style = 'blockv'; return parseSeq(j, ind, path); }
      return { v: null, next: li + 1 };
    }
    return inlineValue(li, ind, col, rest, path);
  }
  function parseMap(li, ind, path) {
    var obj = {};
    for (;;) {
      li = nextSig(li);
      if (li >= n || L[li].ind < ind) break;
      if (L[li].ind > ind) err('bad indentation of a mapping entry', li);
      var c = L[li].c, e = mapEntry(c);
      if (!e) {
        if (isSeqLine(c)) break;
        err('expected a "key: value" line here', li);
      }
      if (Object.prototype.hasOwnProperty.call(obj, e.key)) err('duplicated mapping key "' + e.key + '"', li);
      var cp = path.concat(e.key);
      record(cp, li, { col: ind, kind: 'entry' });
      last = Math.max(last, li);
      var r = valueAfterKey(li, ind, e, cp);
      obj[e.key] = r.v;
      index[pk(cp)].end = last;
      li = r.next;
    }
    return { v: obj, next: li };
  }
  function parseSeq(li, ind, path) {
    var arr = [];
    for (;;) {
      li = nextSig(li);
      if (li >= n || L[li].ind < ind) break;
      if (L[li].ind > ind) err('bad indentation of a sequence entry', li);
      var c = L[li].c;
      if (!isSeqLine(c)) break;
      var ip = path.concat(arr.length);
      record(ip, li, { col: ind, kind: 'item' });
      last = Math.max(last, li);
      var off = 1; while (c[off] === ' ') off++;
      var rest = c.slice(off), col = ind + off, r;
      if (rest === '' || rest[0] === '#') {
        var j = nextSig(li + 1);
        if (j < n && L[j].ind > ind) r = parseNode(j, L[j].ind, ip, ind);
        else r = { v: null, next: li + 1 };
      } else if (isSeqLine(rest) || mapEntry(rest)) {
        L[li] = { ind: col, c: rest, raw: L[li].raw, blank: false, com: false };
        r = parseNode(li, col, ip, ind);
      } else {
        r = inlineValue(li, ind, col, rest, ip);
      }
      arr.push(r.v);
      index[pk(ip)].end = last;
      li = r.next;
    }
    return { v: arr, next: li };
  }
  function parseNode(li, ind, path, pind) {
    var c = L[li].c;
    if (isSeqLine(c)) return parseSeq(li, ind, path);
    if (mapEntry(c)) return parseMap(li, ind, path);
    last = Math.max(last, li);
    return inlineValue(li, pind, ind, c, path);
  }
  var i0 = nextSig(0), res = { v: null, next: n };
  if (i0 < n) {
    res = parseNode(i0, L[i0].ind, [], -1);
    var jx = nextSig(res.next);
    if (jx < n) err(L[jx].ind > 0 ? 'bad indentation of a mapping entry' : 'end of the stream or a document separator is expected', jx);
  }
  index[''] = { line: i0, cstart: 0, end: last, col: 0, kind: 'root' };
  return { value: res.v, index: index, comments: comments };
}
function yParseSafe(text) {
  try { var r = yParse(text); r.ok = true; return r; }
  catch (e) { return { ok: false, error: { msg: e.message, line: e.yLine > 0 ? e.yLine : 1 }, index: {}, comments: {} }; }
}

// ===================== YAML serializer (house style) =====================
var ORDER = {
  testFile: ['id', 'name', 'tags', 'skip', 'vars', 'profiles', 'beforeAll', 'beforeEach', 'afterEach', 'afterAll', 'tests'],
  test: ['id', 'name', 'tags', 'skip', 'before', 'call', 'steps', 'vars', 'auth', 'body', 'expect', 'save', 'poll', 'examples', 'repeat', 'retry', 'timeout', 'after', 'standalone'],
  action: ['id', 'call', 'test', 'function', 'command', 'forEach', 'repeat', 'parallel', 'vars', 'auth', 'body', 'expect', 'save', 'poll', 'retry', 'keep', 'cache', 'timeout'],
  expect: ['status', 'headers', 'body', 'size', 'timing', 'cel', 'custom'],
  bodyExpect: ['schema', 'match', 'equals'],
  poll: ['until', 'every', 'timeout'],
  skip: ['reason', 'until'],
  retry: ['attempts', 'delay'],
  cache: ['for', 'check'],
  authRef: ['id', 'vars'],
  apiFile: ['id', 'name', 'baseUrl', 'auth', 'headers', 'vars', 'profiles', 'source', 'beforeRequest', 'afterResponse', 'apis'],
  api: ['id', 'name', 'method', 'path', 'operationId', 'deprecated', 'vars', 'query', 'headers', 'body', 'auth', 'timeout', 'expect', 'save', 'cleanup', 'beforeRequest', 'afterResponse'],
  cleanup: ['call', 'vars'],
  source: ['openapi', 'headers', 'include'],
  config: ['syntaxVersion', 'project', 'paths', 'defaults', 'vars', 'profiles', 'auth', 'secrets', 'generators', 'datasets', 'reports', 'history', 'alerts', 'plugins', 'redact', 'beforeRun', 'afterRun'],
  profile: ['id', 'name', 'default', 'vars', 'agents'],
  authDef: ['id', 'name', 'type', 'tokenUrl', 'authorizeUrl', 'clientId', 'clientSecret', 'refreshToken', 'scopes', 'audience', 'pkce', 'redirectPort', 'username', 'password', 'token', 'in', 'value', 'plugin', 'config', 'cache', 'vars', 'steps', 'apply', 'expiresIn', 'refreshBefore', 'refreshOn'],
  flowStep: ['id', 'call', 'method', 'url', 'headers', 'body', 'expect', 'save'],
  defaults: ['timeout', 'concurrency', 'perHost', 'retry', 'headers'],
  reports: ['dir', 'formats'],
  secrets: ['provider', 'schema'],
  load: ['id', 'name', 'mix', 'stages', 'thinkTime', 'checks', 'thresholds', 'abortOn'],
  mix: ['test', 'weight', 'thresholds'],
  stage: ['duration', 'users'],
  thinkTime: ['min', 'max']
};
function shapeOf(kind, path) {
  var p = path.map(function (s) { return typeof s === 'number' ? '#' : s; });
  if (!p.length) return { test: 'testFile', api: 'apiFile', config: 'config', load: 'load' }[kind] || null;
  var s = p.join('.'), last = p[p.length - 1], prev = p[p.length - 2];
  if (kind === 'test') {
    if (s === 'tests.#') return 'test';
    if (/^(beforeAll|beforeEach|afterEach|afterAll)\.#$/.test(s) || /^tests\.#\.(before|after|steps)\.#$/.test(s)) return 'action';
  }
  if (kind === 'api') { if (s === 'apis.#') return 'api'; if (s === 'apis.#.cleanup') return 'cleanup'; if (s === 'source') return 'source'; }
  if (kind === 'config') {
    if (s === 'profiles.#') return 'profile'; if (s === 'auth.#') return 'authDef'; if (s === 'auth.#.steps.#') return 'flowStep';
    if (/^(beforeRun|afterRun)\.#$/.test(s)) return 'action';
    if (s === 'defaults' || s === 'reports' || s === 'secrets') return s;
  }
  if (kind === 'load') { if (s === 'mix.#') return 'mix'; if (s === 'stages.#') return 'stage'; if (s === 'thinkTime') return 'thinkTime'; }
  if (last === 'check' && prev === 'cache') return 'action';
  if (last === 'expect' || last === 'until') return 'expect';
  if (last === 'body' && (prev === 'expect' || prev === 'until')) return 'bodyExpect';
  if (last === 'poll') return 'poll';
  if (last === 'skip') return 'skip';
  if (last === 'retry') return 'retry';
  if (last === 'cache') return 'cache';
  if (last === 'auth') return 'authRef';
  return null;
}
function orderedKeys(obj, kind, path) {
  var sh = shapeOf(kind, path), ks = Object.keys(obj);
  if (!sh) return ks;
  var out = [];
  ORDER[sh].forEach(function (k) { if (ks.indexOf(k) >= 0) out.push(k); });
  ks.forEach(function (k) { if (out.indexOf(k) < 0) out.push(k); });
  return out;
}
var FLOW_MAX = 96;
function sp(n) { return new Array(n + 1).join(' '); }
function yNeedsQuote(s, isKey) {
  if (s === '') return true;
  if (!isKey) { var r = yScalar(s); if (r !== s) return true; }
  if (/^\s|\s$/.test(s)) return true;
  if (/[\u0000-\u001f]/.test(s)) return true;
  if (/^[\[\]{},#&*!|>'"%@`]/.test(s)) return true;
  if (/^[-?:](\s|$)/.test(s)) return true;
  if (/:(\s|$)/.test(s)) return true;
  if (/\s#/.test(s)) return true;
  if (s === '---' || s === '...') return true;
  return false;
}
function yFmt(v) {
  if (v === null || v === undefined) return 'null';
  if (v === true) return 'true';
  if (v === false) return 'false';
  if (typeof v === 'number') return isNaN(v) ? '.nan' : v === Infinity ? '.inf' : v === -Infinity ? '-.inf' : String(v);
  var s = String(v);
  return yNeedsQuote(s, false) ? JSON.stringify(s) : s;
}
function yKey(k) { return yNeedsQuote(String(k), true) ? JSON.stringify(String(k)) : String(k); }
function flowable(arr) {
  return arr.every(function (x) { return isScalar(x) && !(typeof x === 'string' && (/[\n,\[\]{}#]/.test(x) || x.indexOf('${') >= 0)); });
}
function inlineOf(v, prefixLen) {
  if (isScalar(v)) { if (typeof v === 'string' && v.indexOf('\n') >= 0) return null; return yFmt(v); }
  if (Array.isArray(v)) {
    if (!v.length) return '[]';
    if (!flowable(v)) return null;
    var t = '[' + v.map(yFmt).join(', ') + ']';
    return (prefixLen || 0) + t.length > FLOW_MAX ? null : t;
  }
  return Object.keys(v).length ? null : '{}';
}
function isBlockish(v) { return inlineOf(v, 0) === null; }
function blockScalar(pre, v, ind, out, t) {
  var m = /\n*$/.exec(v)[0].length, ch = m === 0 ? '|-' : m === 1 ? '|' : '|+';
  out.push(pre + ' ' + ch + (t || ''));
  var body = v.replace(/\n+$/, '');
  body.split('\n').forEach(function (l) { out.push(l === '' ? '' : sp(ind + 2) + l); });
  for (var i = 1; i < m && ch === '|+'; i++) out.push('');
}
function dumpEntry(k, v, kind, cm, cp, ind, out, trailing) {
  var pre = sp(ind) + yKey(k) + ':', t = trailing || '';
  if (typeof v === 'string' && v.indexOf('\n') >= 0) { blockScalar(pre, v, ind, out, t); return; }
  var inl = inlineOf(v, pre.length + 1);
  if (inl !== null) { out.push(pre + ' ' + inl + t); return; }
  out.push(pre + t);
  if (Array.isArray(v)) dumpSeq(v, kind, cm, cp, ind + 2, out); else dumpMap(v, kind, cm, cp, ind + 2, out);
}
function dumpMap(obj, kind, cm, path, ind, out) {
  orderedKeys(obj, kind, path).forEach(function (k) {
    var cp = path.concat(k), c = cm && cm[pk(cp)];
    if (path.length === 0 && out.length > 0 && isBlockish(obj[k])) out.push('');
    if (c && c.before) c.before.forEach(function (l) { out.push(l); });
    dumpEntry(k, obj[k], kind, cm, cp, ind, out, c && c.trailing);
  });
}
function dumpSeq(arr, kind, cm, path, ind, out) {
  arr.forEach(function (item, i) {
    var ip = path.concat(i), c = cm && cm[pk(ip)];
    if (c && c.before) c.before.forEach(function (l) { out.push(l); });
    dumpItem(item, kind, cm, ip, ind, out, c && c.trailing);
  });
}
function dumpItem(item, kind, cm, ip, ind, out, trailing) {
  var t = trailing || '';
  if (typeof item === 'string' && item.indexOf('\n') >= 0) { blockScalar(sp(ind) + '-', item, ind, out, t); return; }
  var inl = inlineOf(item, ind + 2);
  if (inl !== null) { out.push(sp(ind) + '- ' + inl + t); return; }
  var tmp = [];
  if (Array.isArray(item)) dumpSeq(item, kind, cm, ip, ind + 2, tmp); else dumpMap(item, kind, cm, ip, ind + 2, tmp);
  var j = 0; while (j < tmp.length && /^\s*#/.test(tmp[j])) { out.push(tmp[j]); j++; }
  tmp[j] = sp(ind) + '- ' + tmp[j].slice(ind + 2);
  for (; j < tmp.length; j++) out.push(tmp[j]);
}
function yDump(v, kind, cm) {
  var out = [];
  if (isObj(v)) { if (Object.keys(v).length) dumpMap(v, kind, cm, [], 0, out); else out.push('{}'); }
  else if (Array.isArray(v)) { if (v.length) dumpSeq(v, kind, cm, [], 0, out); else out.push('[]'); }
  else out.push(yFmt(v));
  if (cm && cm['\u0002end']) cm['\u0002end'].before.forEach(function (l) { out.push(l); });
  return out.join('\n') + '\n';
}

// ===================== surgical model -> text edits =====================
function mdiff(a, b, path, out) {
  if (sameOrderEq(a, b)) return;
  if (Array.isArray(a) && Array.isArray(b)) {
    if (a.length === b.length) { for (var i = 0; i < a.length; i++) mdiff(a[i], b[i], path.concat(i), out); return; }
    if (b.length === a.length + 1 && a.length > 0) {
      for (var k = 0; k <= a.length; k++) if (deepEq(a.slice(0, k), b.slice(0, k)) && deepEq(a.slice(k), b.slice(k + 1))) { out.push({ op: 'ains', path: path, idx: k }); return; }
    }
    if (a.length === b.length + 1 && b.length > 0) {
      for (var k2 = 0; k2 < a.length; k2++) if (deepEq(a.slice(0, k2), b.slice(0, k2)) && deepEq(a.slice(k2 + 1), b.slice(k2))) { out.push({ op: 'adel', path: path.concat(k2) }); return; }
    }
  }
  if (isObj(a) && isObj(b)) {
    var ka = Object.keys(a), kb = Object.keys(b);
    var ca = ka.filter(function (k) { return Object.prototype.hasOwnProperty.call(b, k); });
    var cb = kb.filter(function (k) { return Object.prototype.hasOwnProperty.call(a, k); });
    if (ca.join(PK) === cb.join(PK) && ca.length > 0) {
      kb.forEach(function (k) { if (!Object.prototype.hasOwnProperty.call(a, k)) out.push({ op: 'ins', path: path, key: k }); });
      ka.forEach(function (k) { if (!Object.prototype.hasOwnProperty.call(b, k)) out.push({ op: 'del', path: path.concat(k) }); });
      ca.forEach(function (k) { mdiff(a[k], b[k], path.concat(k), out); });
      return;
    }
  }
  out.push({ op: 'set', path: path });
}
function isPrefix(p, q) { if (p.length > q.length) return false; for (var i = 0; i < p.length; i++) if (p[i] !== q[i]) return false; return true; }
function yApplyModel(text, kind, b) {
  var P = yParseSafe(text);
  if (!P.ok) return yDump(b, kind, null);
  var a = P.value, changes = [];
  mdiff(a, b, [], changes);
  if (!changes.length) return text;
  var lines = String(text).split('\n'), I = P.index;
  function node(path) { return I[pk(path)]; }
  function isCompact(path) {
    if (!path.length) return false;
    var nd = node(path), par = node(path.slice(0, -1));
    return !!(nd && par && par.kind === 'item' && nd.line === par.line && nd.kind === 'entry');
  }
  function plan(ch) {
    var path = ch.op === 'ins' || ch.op === 'ains' ? ch.path : ch.path;
    if (ch.op === 'set') {
      if (!path.length) return null;
      var nd = node(path), nv = getIn(b, path);
      if (!nd) return { esc: path.slice(0, -1) };
      var inl = inlineOf(nv, nd.vs || 0);
      if ((nd.style === 'plain' || nd.style === 'dq' || nd.style === 'sq' || nd.style === 'flow' || nd.style === 'empty') && inl !== null) {
        var raw = lines[nd.vline];
        return { s: nd.vline, e: nd.vline + 1, lines: [raw.slice(0, nd.vs) + (nd.style === 'empty' ? ' ' : '') + inl + raw.slice(nd.ve)] };
      }
      var out = [], last = path[path.length - 1], c = P.comments[pk(path)];
      if (typeof last === 'number') {
        dumpItem(nv, kind, P.comments, path, nd.col, out, c && c.trailing);
      } else {
        dumpEntry(last, nv, kind, P.comments, path, nd.col, out, c && c.trailing);
        if (isCompact(path)) out[0] = lines[nd.line].slice(0, nd.col) + out[0].slice(nd.col);
      }
      return { s: nd.line, e: nd.end + 1, lines: out };
    }
    if (ch.op === 'del') {
      var dn = node(path);
      if (!dn || isCompact(path)) return { esc: path.slice(0, -1) };
      var s = dn.cstart, e = dn.end + 1;
      if (path.length === 1 && s > 0 && lines[s - 1].trim() === '' && (e >= lines.length || lines[e].trim() === '')) s--;
      return { s: s, e: e, lines: [] };
    }
    if (ch.op === 'ins') {
      var mobj = getIn(b, path), aobj = getIn(a, path), keys = orderedKeys(mobj, kind, path);
      var pos = keys.indexOf(ch.key), prev = null, next = null, q;
      for (q = pos - 1; q >= 0; q--) if (aobj && Object.prototype.hasOwnProperty.call(aobj, keys[q]) && node(path.concat(keys[q]))) { prev = keys[q]; break; }
      if (prev === null) for (q = pos + 1; q < keys.length; q++) if (aobj && Object.prototype.hasOwnProperty.call(aobj, keys[q]) && node(path.concat(keys[q]))) { next = keys[q]; break; }
      var ref = prev !== null ? node(path.concat(prev)) : next !== null ? node(path.concat(next)) : null;
      if (!ref || (prev === null && isCompact(path.concat(next)))) return { esc: path };
      var o2 = [], nv2 = mobj[ch.key];
      dumpEntry(ch.key, nv2, kind, null, path.concat(ch.key), ref.col, o2, '');
      if (!path.length && isBlockish(nv2)) { if (prev !== null) o2.unshift(''); else o2.push(''); }
      var at = prev !== null ? ref.end + 1 : ref.cstart;
      return { s: at, e: at, lines: o2, ins: true };
    }
    if (ch.op === 'ains') {
      var an = node(path), arrA = getIn(a, path), arrB = getIn(b, path);
      if (!an || an.style === 'flow' || an.style === 'flowml' || !arrA || !arrA.length) return { esc: path };
      var refi = ch.idx < arrA.length ? node(path.concat(ch.idx)) : node(path.concat(arrA.length - 1));
      if (!refi) return { esc: path };
      var o3 = [];
      dumpItem(arrB[ch.idx], kind, null, path.concat(ch.idx), refi.col, o3, '');
      var at2 = ch.idx < arrA.length ? refi.cstart : refi.end + 1;
      return { s: at2, e: at2, lines: o3, ins: true };
    }
    if (ch.op === 'adel') {
      var dn2 = node(path);
      if (!dn2) return { esc: path.slice(0, -1) };
      return { s: dn2.cstart, e: dn2.end + 1, lines: [] };
    }
    return null;
  }
  var edits = null;
  for (var round = 0; round < 8; round++) {
    // drop changes nested under a 'set' of an ancestor
    changes = changes.filter(function (c, i) {
      var tc = c.op === 'ins' ? c.path.concat(c.key) : c.op === 'ains' ? c.path.concat('#') : c.path;
      return !changes.some(function (d, j) {
        if (j === i || d.op !== 'set' || !isPrefix(d.path, tc)) return false;
        if (d.path.length < tc.length) return true;
        return c.op !== 'set' || j < i;
      });
    });
    var esc = false, plans = [];
    for (var ci = 0; ci < changes.length; ci++) {
      var pl = plan(changes[ci]);
      if (!pl) { edits = null; esc = 'full'; break; }
      if (pl.esc) { changes[ci] = { op: 'set', path: pl.esc }; esc = true; break; }
      plans.push(pl);
    }
    if (esc === 'full') break;
    if (esc) continue;
    edits = plans; break;
  }
  if (edits) {
    edits.sort(function (x, y) { return y.s - x.s || (y.ins ? 1 : 0) - (x.ins ? 1 : 0); });
    var ok = true;
    for (var z = 0; z + 1 < edits.length; z++) { if (edits[z + 1].e > edits[z].s || (edits[z + 1].s === edits[z].s && edits[z + 1].ins && edits[z].ins)) ok = false; }
    if (ok) {
      var L2 = lines.slice();
      edits.forEach(function (ed) { Array.prototype.splice.apply(L2, [ed.s, ed.e - ed.s].concat(ed.lines)); });
      var outText = L2.join('\n');
      var Q = yParseSafe(outText);
      if (Q.ok && deepEq(Q.value, b)) return outText;
    }
  }
  return yDump(b, kind, P.comments);
}
