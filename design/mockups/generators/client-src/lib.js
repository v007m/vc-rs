/* ---------- pure logic (no DOM, no state) ---------- */
var LB = '{' + '{', RB = '}' + '}';
var FAKE = { 'person.fullName': 'Amara Okafor', 'person.firstName': 'Amara', 'internet.email': 'amara.okafor@example.com', 'commerce.productName': 'Desk lamp', 'location.city': 'Lisbon' };
var SERVICES = ['users', 'orders', 'carts', 'catalog', 'geo', 'payments'];
var PORTS = { users: 8081, orders: 8082, carts: 8083, catalog: 8084, geo: 8085, payments: 8086, coupons: 8087 };
var PROFILES = [
  { id: 'dev', tokenStatus: 'cached, 58 min left', tone: 'pass' },
  { id: 'staging', tokenStatus: 'cached, 41 min left', tone: 'pass' },
  { id: 'prod', tokenStatus: 'no token cached, fetched on Send', tone: 'block' }
];
var LEVEL_ORDER = ['call', 'saved', 'api', 'file', 'profile', 'config'];
var LEVEL_LABEL = { call: 'call', saved: 'saved', api: 'api default', file: 'file', profile: 'profile', config: 'config' };

function clone(x) { return x === undefined ? undefined : JSON.parse(JSON.stringify(x)); }
function cap(s) { return s ? s.charAt(0).toUpperCase() + s.slice(1) : s; }
function words(s) { return String(s || '').replace(/([a-z0-9])([A-Z])/g, '$1 $2').replace(/[^A-Za-z0-9]+/g, ' ').trim().split(/\s+/).filter(Boolean); }
function toCamel(s, dropArticles) {
  var w = words(s).map(function (x) { return x.toLowerCase(); });
  if (dropArticles) w = w.filter(function (x) { return ['a', 'an', 'the'].indexOf(x) < 0; });
  if (!w.length) return '';
  var out = w[0] + w.slice(1).map(cap).join('');
  if (/^[0-9]/.test(out)) out = 'n' + out;
  return out;
}
function validId(id) { return /^[a-z][A-Za-z0-9]*$/.test(id || ''); }
function singular(w) {
  if (/ies$/i.test(w)) return w.slice(0, -3) + 'y';
  if (/(sses|xes|ches|shes)$/i.test(w)) return w.slice(0, -2);
  if (/(ss|us|is)$/i.test(w)) return w;
  if (/s$/i.test(w)) return w.slice(0, -1);
  return w;
}
function isPlural(w) { return /s$/i.test(w) && !/(ss|us|is)$/i.test(w); }
function humanize(id) { var w = words(id).map(function (x) { return x.toLowerCase(); }); return w.length ? cap(w.join(' ')) : ''; }

/* ---------- config, profiles, catalog ---------- */
function initialConfig() {
  var profiles = {};
  PROFILES.forEach(function (p) {
    var v = {};
    SERVICES.forEach(function (s) {
      v[s + 'Url'] = p.id === 'dev' ? 'http://localhost:' + PORTS[s] : p.id === 'prod' ? 'https://' + s + '.shop.example' : 'https://' + s + '.staging.shop.example';
    });
    profiles[p.id] = v;
  });
  return { vars: { currency: 'EUR', locale: 'en-GB' }, headers: { accept: 'application/json', 'user-agent': 'vibranium/2.0.0' }, profiles: profiles };
}

function initialCatalog() {
  return [
    { id: 'users', name: 'User accounts service', path: 'services/users/users.api.yaml', baseUrl: '${var.usersUrl}/api/v1', auth: 'shopOauth', headers: {}, vars: {}, apis: [
      { id: 'create', name: 'Create a user', method: 'POST', path: '/users', vars: { name: '${fake.person.fullName}', email: '${fake.internet.email}', role: 'member' }, body: { name: '${var.name}', email: '${var.email}', role: '${var.role}' }, expect: { status: 201 }, save: { userId: '${response.body.id}' } },
      { id: 'get', name: 'Get a user', method: 'GET', path: '/users/${var.userId}', expect: { status: 200 } },
      { id: 'list', name: 'List users', method: 'GET', path: '/users', query: { page: '1', limit: '20' } },
      { id: 'update', name: 'Update a user', method: 'PATCH', path: '/users/${var.userId}', vars: { role: 'admin' }, body: { role: '${var.role}' } },
      { id: 'delete', name: 'Delete a user', method: 'DELETE', path: '/users/${var.userId}', expect: { status: [200, 204, 404] } },
      { id: 'startExport', name: 'Start a user export', method: 'POST', path: '/users/exports', vars: { format: 'csv' }, body: { format: '${var.format}' }, expect: { status: 202 }, save: { exportId: '${response.body.exportId}' } },
      { id: 'exportStatus', name: 'Get export status', method: 'GET', path: '/users/exports/${var.exportId}' },
      { id: 'deactivate', name: 'Deactivate a user', method: 'POST', path: '/users/${var.userId}/deactivate', vars: { reason: 'requested' }, body: { reason: '${var.reason}' }, expect: { status: 200, match: { status: 'deactivated' } } },
      { id: 'rename', name: 'Rename a user', method: 'PATCH', path: '/users/${var.userId}/name', vars: { name: '${fake.person.fullName}' }, body: { name: '${var.name}' } }
    ] },
    { id: 'orders', name: 'Orders service', path: 'services/orders/orders.api.yaml', baseUrl: '${var.ordersUrl}/api/v1', auth: 'shopOauth', headers: { 'x-shop-region': 'eu' }, vars: { orderTotal: 59.98 }, apis: [
      { id: 'place', name: 'Place an order', method: 'POST', path: '/orders', body: { cartId: '${var.cartId}', userId: '${var.userId}', currency: '${var.currency}' }, expect: { status: 201, match: { total: '${var.orderTotal}' } }, save: { orderId: '${response.body.id}' } },
      { id: 'get', name: 'Get an order', method: 'GET', path: '/orders/${var.orderId}', expect: { status: 200 } },
      { id: 'cancel', name: 'Cancel an order', method: 'POST', path: '/orders/${var.orderId}/cancel', expect: { status: 200 } },
      { id: 'refund', name: 'Refund an order', method: 'POST', path: '/orders/${var.orderId}/refund', vars: { amount: '${var.orderTotal}' }, body: { amount: '${var.amount}' }, expect: { status: 202 } }
    ] },
    { id: 'carts', name: 'Shopping carts', path: 'services/carts/carts.api.yaml', baseUrl: '${var.cartsUrl}/api/v1', auth: 'shopOauth', headers: {}, vars: { quantity: 1 }, apis: [
      { id: 'create', name: 'Create a cart', method: 'POST', path: '/carts', body: { userId: '${var.userId}' }, expect: { status: 201 }, save: { cartId: '${response.body.id}' } },
      { id: 'addItem', name: 'Add an item to a cart', method: 'POST', path: '/carts/${var.cartId}/items', vars: { sku: 'SKU-1' }, body: { sku: '${var.sku}', quantity: '${var.quantity}' }, expect: { status: 201 } },
      { id: 'get', name: 'Get a cart', method: 'GET', path: '/carts/${var.cartId}', expect: { status: 200 } }
    ] },
    { id: 'catalog', name: 'Product catalog', path: 'services/catalog/catalog.api.yaml', baseUrl: '${var.catalogUrl}/api/v1', auth: 'shopOauth', headers: {}, vars: {}, apis: [
      { id: 'seedProducts', name: 'Seed test products', method: 'POST', path: '/admin/products/seed', body: { count: 2 }, expect: { status: 201 }, save: { productIds: '${response.body.productIds}' } },
      { id: 'getProduct', name: 'Get a product', method: 'GET', path: '/products/${var.sku}', vars: { sku: 'SKU-1' }, expect: { status: 200 } },
      { id: 'search', name: 'Search products', method: 'GET', path: '/products', vars: { query: 'desk lamp' }, query: { q: '${var.query}' }, expect: { status: 200 } }
    ] },
    { id: 'geo', name: 'Geography', path: 'services/geo/geo.api.yaml', baseUrl: '${var.geoUrl}/api/v1', auth: 'shopOauth', headers: {}, vars: {}, apis: [
      { id: 'createCity', name: 'Create a city', method: 'POST', path: '/cities', vars: { name: 'Lisbon', country: 'Portugal' }, body: { name: '${var.name}', country: '${var.country}' }, expect: { status: 201 }, save: { cityId: '${response.body.id}' } },
      { id: 'deleteCity', name: 'Delete a city', method: 'DELETE', path: '/cities/${var.cityId}', expect: { status: [200, 204] } },
      { id: 'createLocationGroup', name: 'Create a location group', method: 'POST', path: '/location-groups', vars: { name: 'Iberia' }, body: { name: '${var.name}', cityIds: ['${var.cityId}'] }, expect: { status: 201 }, save: { groupId: '${response.body.id}' } }
    ] },
    { id: 'payments', name: 'Payments', path: 'services/payments/payments.api.yaml', baseUrl: '${var.paymentsUrl}/api/v1', auth: 'shopOauth', headers: {}, vars: {}, apis: [
      { id: 'authorize', name: 'Authorize a payment', method: 'POST', path: '/payments/authorize', vars: { method: 'card' }, body: { orderId: '${var.orderId}', method: '${var.method}' }, expect: { status: 200 }, save: { paymentId: '${response.body.id}' } }
    ] }
  ];
}

function initialFlows() {
  return [
    { id: 'checkoutHappyPath', name: 'Checkout happy path', stopOnFail: true, stages: [
      { uid: 'f1a', api: 'users.create', vars: {} },
      { uid: 'f1b', api: 'carts.create', vars: {} },
      { uid: 'f1c', api: 'carts.addItem', vars: { sku: '${item}' }, loop: { kind: 'forEach', items: 'SKU-1, SKU-2', count: 2 } },
      { uid: 'f1d', api: 'orders.place', vars: {} },
      { uid: 'f1e', api: 'orders.get', vars: {}, poll: { field: 'status', equals: 'paid', every: '1s', timeout: '30s' } }
    ] },
    { id: 'deactivateUser', name: 'Deactivate user', stopOnFail: true, stages: [
      { uid: 'f2a', api: 'users.create', vars: {} },
      { uid: 'f2b', api: 'users.deactivate', vars: {} },
      { uid: 'f2c', api: 'users.get', vars: {} }
    ] }
  ];
}

function findApi(catalog, fullId) {
  var i = String(fullId || '').indexOf('.');
  if (i < 0) return null;
  var fid = fullId.slice(0, i), aid = fullId.slice(i + 1);
  for (var f = 0; f < catalog.length; f++) {
    if (catalog[f].id !== fid) continue;
    for (var a = 0; a < catalog[f].apis.length; a++) if (catalog[f].apis[a].id === aid) return { file: catalog[f], api: catalog[f].apis[a], fullId: fullId };
  }
  return null;
}
function allApis(catalog) {
  var out = [];
  catalog.forEach(function (f) { f.apis.forEach(function (a) { out.push({ fullId: f.id + '.' + a.id, file: f, api: a }); }); });
  return out;
}

/* ---------- templates ---------- */
function lookupPath(obj, path) {
  if (!path) return obj;
  var parts = String(path).split('.');
  var cur = obj;
  for (var i = 0; i < parts.length; i++) {
    if (cur === null || cur === undefined) return undefined;
    cur = cur[parts[i]];
  }
  return cur;
}
function splitPipes(expr) {
  var out = [], buf = '', q = null;
  for (var i = 0; i < expr.length; i++) {
    var c = expr[i];
    if (q) { if (c === q) q = null; buf += c; continue; }
    if (c === '"' || c === "'") { q = c; buf += c; continue; }
    if (c === '|') { out.push(buf); buf = ''; continue; }
    buf += c;
  }
  out.push(buf);
  return out.map(function (s) { return s.trim(); });
}
function b64(s) {
  var chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/', out = '', i = 0;
  s = String(s);
  while (i < s.length) {
    var a = s.charCodeAt(i++), b = s.charCodeAt(i++), c = s.charCodeAt(i++);
    var n = (a << 16) | ((b || 0) << 8) | (c || 0);
    out += chars[(n >> 18) & 63] + chars[(n >> 12) & 63] + (isNaN(b) ? '=' : chars[(n >> 6) & 63]) + (isNaN(c) ? '=' : chars[n & 63]);
  }
  return out;
}
function applyPipe(v, pipe) {
  var m = /^(\w+)(?:\((.*)\))?$/.exec(pipe);
  if (!m) return v;
  var arg = m[2] === undefined ? undefined : m[2].trim().replace(/^['"]|['"]$/g, '');
  switch (m[1]) {
    case 'number': return v === undefined ? v : Number(v);
    case 'int': return v === undefined ? v : parseInt(v, 10);
    case 'bool': return v === undefined ? v : (v === true || v === 'true' || v === 1 || v === '1');
    case 'string': return v === undefined ? v : String(v);
    case 'lower': return v === undefined ? v : String(v).toLowerCase();
    case 'trim': return v === undefined ? v : String(v).trim();
    case 'json': case 'jsonString': return v === undefined ? v : JSON.stringify(v);
    case 'base64': return v === undefined ? v : b64(v);
    case 'default': return (v === undefined || v === null || v === '') ? arg : v;
    case 'take': return Array.isArray(v) ? v.slice(0, Number(arg)) : v;
    default: return v;
  }
}
function resolveExpr(expr, ctx, depth) {
  var parts = splitPipes(expr);
  var head = parts[0];
  var dot = head.indexOf('.');
  var ns = dot < 0 ? head : head.slice(0, dot), rest = dot < 0 ? '' : head.slice(dot + 1);
  var v;
  if (ns === 'var') {
    v = lookupPath(ctx.vars || {}, rest);
    if (typeof v === 'string' && v.indexOf('${') >= 0 && depth < 6) v = resolveValue(v, ctx, depth + 1);
  } else if (ns === 'fake') v = FAKE[rest];
  else if (ns === 'secret') v = (ctx.secrets || {})[rest] !== undefined ? '••••••' : undefined;
  else if (ns === 'env') v = (ctx.env || {})[rest];
  else if (ns === 'item') v = rest ? lookupPath(ctx.item, rest) : ctx.item;
  else if (ns === 'response') v = ctx.response ? lookupPath(ctx.response, rest) : undefined;
  else if (ns === 'iteration') v = rest === 'index' ? ctx.iteration : undefined;
  else if (ns === 'run') v = rest === 'id' ? 'r_483' : undefined;
  for (var i = 1; i < parts.length; i++) v = applyPipe(v, parts[i]);
  return v;
}
function resolveValue(str, ctx, depth) {
  depth = depth || 0;
  if (typeof str !== 'string') return str;
  var whole = /^\$\{([^}]*)\}$/.exec(str);
  if (whole) {
    var v = resolveExpr(whole[1], ctx, depth);
    if (v === undefined) { if (ctx.missing) ctx.missing.push(whole[1].trim()); return str; }
    return v;
  }
  return str.replace(/\$\{([^}]*)\}/g, function (all, e) {
    var r = resolveExpr(e, ctx, depth);
    if (r === undefined) { if (ctx.missing) ctx.missing.push(e.trim()); return all; }
    return typeof r === 'object' ? JSON.stringify(r) : String(r);
  });
}
function resolveDeep(x, ctx) {
  if (Array.isArray(x)) return x.map(function (y) { return resolveDeep(y, ctx); });
  if (x && typeof x === 'object') { var o = {}; Object.keys(x).forEach(function (k) { o[k] = resolveDeep(x[k], ctx); }); return o; }
  return resolveValue(x, ctx, 0);
}
function varRefs(str) {
  var out = [], re = /\$\{\s*var\.([A-Za-z0-9_]+)/g, m;
  str = String(str || '');
  while ((m = re.exec(str))) if (out.indexOf(m[1]) < 0) out.push(m[1]);
  return out;
}

/* ---------- requests ---------- */
function rowsFrom(obj) { return Object.keys(obj || {}).map(function (k) { return { k: k, v: String(obj[k]), on: true }; }); }
function reqFromApi(found, key) {
  var api = found.api, file = found.file;
  return {
    key: key, kind: 'request', apiId: found.fullId, name: found.fullId, method: api.method || 'GET',
    urlTpl: file.baseUrl + api.path, query: rowsFrom(api.query), headers: rowsFrom(api.headers),
    bodyMode: api.body ? 'json' : 'none', bodyText: api.body ? JSON.stringify(api.body, null, 2) : '', form: [], fileName: '',
    authMode: 'inherit', bearer: '', basicUser: '', basicPass: '', callVars: {}, extraChecks: [],
    saves: Object.keys(api.save || {}).map(function (k) { return { name: k, expr: api.save[k] }; }),
    mode: 'request', flowId: null, stageUid: null, reqTab: 'params', respTab: 'body', response: null, sending: false
  };
}
function blankReq(key) {
  return { key: key, kind: 'request', apiId: null, name: 'Untitled request', method: 'GET', urlTpl: '', query: [], headers: [], bodyMode: 'none', bodyText: '', form: [], fileName: '',
    authMode: 'inherit', bearer: '', basicUser: '', basicPass: '', callVars: {}, extraChecks: [], saves: [], mode: 'request', flowId: null, stageUid: null, reqTab: 'params', respTab: 'body', response: null, sending: false };
}
function savedValues(saved) { var o = {}; Object.keys(saved || {}).forEach(function (k) { o[k] = saved[k] && typeof saved[k] === 'object' && 'value' in saved[k] ? saved[k].value : saved[k]; }); return o; }
function levelsFor(req, env, savedVals) {
  var found = req.apiId ? findApi(env.catalog, req.apiId) : null;
  return {
    call: req.callVars || {},
    saved: savedVals || {},
    api: found ? (found.api.vars || {}) : {},
    file: found ? (found.file.vars || {}) : {},
    profile: (env.config.profiles || {})[env.profile] || {},
    config: env.config.vars || {}
  };
}
function mergeLevels(L) { return Object.assign({}, L.config, L.profile, L.file, L.api, L.saved, L.call); }
function requestStrings(req) {
  var s = [req.urlTpl];
  (req.query || []).forEach(function (r) { if (r.on) s.push(r.v); });
  (req.headers || []).forEach(function (r) { if (r.on) s.push(r.v); });
  if (req.bodyMode === 'json' || req.bodyMode === 'text') s.push(req.bodyText);
  if (req.bodyMode === 'form' || req.bodyMode === 'multipart') (req.form || []).forEach(function (r) { if (r.on) s.push(r.v); });
  return s;
}
function usedVars(req, levels) {
  var seen = [];
  var queue = [];
  requestStrings(req).forEach(function (s) { varRefs(s).forEach(function (n) { queue.push(n); }); });
  Object.keys(req.callVars || {}).forEach(function (n) { queue.push(n); });
  while (queue.length) {
    var n = queue.shift();
    if (seen.indexOf(n) >= 0) continue;
    seen.push(n);
    LEVEL_ORDER.forEach(function (l) { var v = (levels[l] || {})[n]; if (typeof v === 'string') varRefs(v).forEach(function (m) { queue.push(m); }); });
  }
  return seen;
}
function varRows(req, levels, ctx) {
  return usedVars(req, levels).map(function (name) {
    var defs = [];
    LEVEL_ORDER.forEach(function (l) { var lv = levels[l] || {}; if (Object.prototype.hasOwnProperty.call(lv, name) && lv[name] !== '') defs.push({ level: l, value: lv[name] }); });
    var win = defs[0] || null;
    var r = resolveValue('${var.' + name + '}', { vars: ctx.vars, secrets: ctx.secrets, missing: [] }, 0);
    return { name: name, defs: defs, win: win, resolved: r, unresolved: !win, inUrl: varRefs(req.urlTpl).indexOf(name) >= 0, callValue: (req.callVars || {})[name] === undefined ? '' : String(req.callVars[name]) };
  });
}
function bodyContentType(mode) {
  return { json: 'application/json', text: 'text/plain', form: 'application/x-www-form-urlencoded', multipart: 'multipart/form-data; boundary=vc9f3k', file: 'application/octet-stream' }[mode] || null;
}
function authHeader(req, file, profile) {
  var mode = req.authMode || 'inherit';
  if (mode === 'none') return null;
  if (mode === 'bearer') return { k: 'authorization', v: 'Bearer ' + (req.bearer || '…'), src: 'auth: bearer' };
  if (mode === 'basic') return { k: 'authorization', v: 'Basic ' + b64((req.basicUser || '') + ':' + (req.basicPass || '')), src: 'auth: basic' };
  if (mode === 'partnerLogin') return { k: 'cookie', v: 'partner_session=…', src: 'auth partnerLogin (flow)' };
  var id = file ? file.auth : 'shopOauth';
  if (!id) return null;
  return { k: 'authorization', v: profile === 'prod' ? 'Bearer (fetched on Send)' : 'Bearer eyJhbGciOiJSUzI1NiIs…', src: 'auth ' + id };
}
function inheritedHeaders(req, env) {
  var found = req.apiId ? findApi(env.catalog, req.apiId) : null;
  var out = [];
  Object.keys(env.config.headers || {}).forEach(function (k) { out.push({ k: k, v: env.config.headers[k], src: 'config defaults' }); });
  if (found) Object.keys(found.file.headers || {}).forEach(function (k) { out.push({ k: k, v: found.file.headers[k], src: found.file.id + '.api.yaml' }); });
  var a = authHeader(req, found ? found.file : null, env.profile);
  if (a) out.push(a);
  var ct = bodyContentType(req.bodyMode);
  if (ct) out.push({ k: 'content-type', v: ct, src: 'body: ' + req.bodyMode });
  var own = (req.headers || []).filter(function (r) { return r.on; }).map(function (r) { return r.k.toLowerCase(); });
  out.forEach(function (h) { h.overridden = own.indexOf(h.k.toLowerCase()) >= 0; });
  return out;
}
function jsonScan(text) {
  var i = 0, src = String(text);
  function fail(msg) { var e = new Error(msg); e.pos = i; throw e; }
  function ws() { while (i < src.length && /\s/.test(src[i])) i++; }
  function str() {
    i++;
    while (i < src.length) {
      var c = src[i];
      if (c === '\\') { i += 2; continue; }
      if (c === '"') { i++; return; }
      if (c === '\n') fail('Unterminated string');
      i++;
    }
    fail('Unterminated string');
  }
  function val() {
    ws();
    var c = src[i];
    if (c === '{') {
      i++; ws();
      if (src[i] === '}') { i++; return; }
      for (;;) {
        ws();
        if (src[i] !== '"') fail(src[i] === undefined ? 'Unexpected end of input' : 'Expected a quoted property name, found ' + src[i]);
        str(); ws();
        if (src[i] !== ':') fail("Expected ':' after property name");
        i++; val(); ws();
        if (src[i] === ',') { i++; ws(); if (src[i] === '}') fail('Trailing comma'); continue; }
        if (src[i] === '}') { i++; return; }
        fail(src[i] === undefined ? 'Unexpected end of input' : "Expected ',' or '}', found " + src[i]);
      }
    }
    if (c === '[') {
      i++; ws();
      if (src[i] === ']') { i++; return; }
      for (;;) {
        val(); ws();
        if (src[i] === ',') { i++; ws(); if (src[i] === ']') fail('Trailing comma'); continue; }
        if (src[i] === ']') { i++; return; }
        fail(src[i] === undefined ? 'Unexpected end of input' : "Expected ',' or ']', found " + src[i]);
      }
    }
    if (c === '"') { str(); return; }
    var m = /^(-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?|true|false|null)/.exec(src.slice(i));
    if (m) { i += m[0].length; return; }
    fail(c === undefined ? 'Unexpected end of input' : 'Unexpected ' + c);
  }
  val(); ws();
  if (i < src.length) fail('Unexpected ' + src[i] + ' after the value');
}
function jsonError(text) {
  if (!String(text).trim()) return null;
  try { jsonScan(text); } catch (e) {
    var pos = Math.min(e.pos, String(text).length), before = String(text).slice(0, pos);
    var line = before.split('\n').length, col = pos - before.lastIndexOf('\n');
    return e.message + ' at line ' + line + ', column ' + col;
  }
  try { JSON.parse(text); return null; } catch (e2) { return 'Invalid JSON'; }
}
function buildRequest(req, env, savedVals) {
  var levels = levelsFor(req, env, savedVals);
  var vars = mergeLevels(levels);
  var secrets = env.secrets || { PAYMENTS_KEY: 'set' };
  var missing = [];
  var ctx = { vars: vars, secrets: secrets, missing: missing, item: env.item, iteration: env.iteration };
  var url = resolveValue(req.urlTpl || '', ctx, 0);
  var q = (req.query || []).filter(function (r) { return r.on && r.k; }).map(function (r) { return encodeURIComponent(r.k) + '=' + encodeURIComponent(String(resolveValue(r.v, ctx, 0))).replace(/%20/g, '+'); });
  if (q.length) url += (url.indexOf('?') >= 0 ? '&' : '?') + q.join('&');
  var body = null, bodyText = '', bodyErr = null;
  if (req.bodyMode === 'json') {
    bodyErr = jsonError(req.bodyText);
    if (!bodyErr && String(req.bodyText).trim()) { body = resolveDeep(JSON.parse(req.bodyText), ctx); bodyText = JSON.stringify(body, null, 2); }
  } else if (req.bodyMode === 'text') { bodyText = resolveValue(req.bodyText, ctx, 0); }
  else if (req.bodyMode === 'form' || req.bodyMode === 'multipart') {
    body = {};
    (req.form || []).forEach(function (r) { if (r.on && r.k) body[r.k] = resolveValue(r.v, ctx, 0); });
    bodyText = Object.keys(body).map(function (k) { return k + '=' + body[k]; }).join('&');
  } else if (req.bodyMode === 'file') { bodyText = req.fileName ? '<' + req.fileName + '>' : ''; }
  var headers = inheritedHeaders(req, env).filter(function (h) { return !h.overridden; }).map(function (h) { return { k: h.k, v: h.v, src: h.src }; });
  (req.headers || []).forEach(function (r) { if (r.on && r.k) headers.push({ k: r.k, v: String(resolveValue(r.v, ctx, 0)), src: 'request' }); });
  var uniq = [];
  missing.forEach(function (m) { if (uniq.indexOf(m) < 0) uniq.push(m); });
  return { method: req.method, url: url, headers: headers, body: body, bodyText: bodyText, bodyErr: bodyErr, missing: uniq, vars: vars, levels: levels, ctx: ctx };
}

/* ---------- matching urls to apis ---------- */
function shapeOf(path) {
  return String(path || '').split('?')[0].split('/').map(function (s) { return /\$\{/.test(s) || /^\{.*\}$/.test(s) ? '{}' : s; }).join('/').replace(/\/+$/, '');
}
function splitUrl(url) {
  var m = /^(https?):\/\/([^\/?#]+)([^?#]*)(\?[^#]*)?/.exec(String(url || ''));
  if (!m) return null;
  return { scheme: m[1], host: m[2], path: m[3] || '/', query: m[4] ? m[4].slice(1) : '' };
}
function matchApiByUrl(catalog, method, url, profileVars) {
  var u = splitUrl(url);
  if (!u) return null;
  var origin = u.scheme + '://' + u.host;
  var list = allApis(catalog);
  for (var i = 0; i < list.length; i++) {
    var x = list[i];
    if ((x.api.method || 'GET') !== method) continue;
    var base = resolveValue(x.file.baseUrl, { vars: profileVars, missing: [] }, 0);
    var bu = splitUrl(base);
    if (!bu || bu.scheme + '://' + bu.host !== origin) continue;
    var full = shapeOf(bu.path.replace(/\/+$/, '') + x.api.path);
    var mine = u.path.split('/').map(function (s, idx) { var parts = full.split('/'); return parts[idx] === '{}' ? '{}' : s; }).join('/').replace(/\/+$/, '');
    if (mine === full) return x;
  }
  return null;
}

/* ---------- simulated server ---------- */
var TIMING = {
  'users.create': [4, 11, 18, 96, 13], 'users.get': [2, 8, 15, 41, 5], 'users.list': [3, 9, 16, 88, 21], 'orders.place': [5, 12, 19, 188, 17],
  'orders.get': [2, 9, 15, 52, 6], 'carts.create': [3, 10, 17, 61, 6], 'carts.addItem': [2, 8, 15, 47, 5], 'catalog.seedProducts': [4, 12, 18, 1204, 3],
  'payments.authorize': [3, 11, 18, 362, 8]
};
function timingFor(id, profile, attempt) {
  var t = (TIMING[id] || [3, 9, 16, 70, 8]).slice();
  if (attempt) t = [0, 0, 0, t[3] - 6 + attempt * 3, t[4]];
  if (profile === 'dev') { t[0] = 0; t[1] = 1; t[2] = 0; t[3] = Math.round(t[3] * 0.4); }
  if (profile === 'prod') { t[3] = Math.round(t[3] * 0.8); }
  return { dns: t[0], connect: t[1], tls: t[2], firstByte: t[3], download: t[4], total: t[0] + t[1] + t[2] + t[3] + t[4] };
}
function user(id, v) { return { id: id, name: v('name') || 'Amara Okafor', email: v('email') || 'amara.okafor@example.com', role: v('role') || 'member', status: 'active', createdAt: '2026-10-23T14:20:07Z', verified: false }; }
function orderBody(v, status) {
  return { id: v('orderId') || 'ord_8Kq2Tz', userId: v('userId'), cartId: v('cartId'), status: status, currency: v('currency') || 'EUR',
    items: [{ sku: 'SKU-1', name: 'Desk lamp', price: 24.99, quantity: 1 }, { sku: 'SKU-2', name: 'Lamp shade', price: 34.99, quantity: 1 }],
    subtotal: 59.98, tax: 4.8, total: 64.78, createdAt: '2026-10-23T14:21:40Z' };
}
function cannedResponse(id, vars, body, opts) {
  opts = opts || {};
  var v = function (k) { var x = vars[k]; return typeof x === 'string' && x.indexOf('${') >= 0 ? undefined : x; };
  var b = body || {};
  switch (id) {
    case 'users.create': return { status: 201, body: { id: 'u_7f3k29qp', name: b.name, email: b.email, role: b.role, status: 'active', createdAt: '2026-10-23T14:20:07Z', verified: false }, location: '/api/v1/users/u_7f3k29qp' };
    case 'users.get': return opts.deactivated ? { status: 200, body: Object.assign(user(v('userId'), v), { status: 'deactivated' }) } : { status: 200, body: user(v('userId'), v) };
    case 'users.list': return { status: 200, body: { items: [user('u_7f3k29qp', v), { id: 'u_2kd81mzz', name: 'Jonas Weber', email: 'jonas.weber@example.com', role: 'admin', status: 'active', createdAt: '2026-09-02T08:11:52Z', verified: true }], page: Number(v('page') || 1), limit: 20, total: 128 } };
    case 'users.update': return { status: 200, body: Object.assign(user(v('userId'), v), b) };
    case 'users.rename': return { status: 200, body: Object.assign(user(v('userId'), v), { name: b.name }) };
    case 'users.delete': return { status: 204, body: null };
    case 'users.startExport': return { status: 202, body: { exportId: 'exp_20QmT1', status: 'queued', format: b.format || 'csv' } };
    case 'users.exportStatus': return { status: 200, body: { exportId: v('exportId'), status: 'ready', rows: 128, url: 'https://exports.staging.shop.example/exp_20QmT1.csv' } };
    case 'users.deactivate': return { status: 200, body: { id: v('userId'), status: 'deactivated', reason: b.reason, deactivatedAt: '2026-10-23T14:22:31Z' } };
    case 'orders.place': return { status: 201, body: orderBody(function (k) { return k === 'orderId' ? 'ord_8Kq2Tz' : v(k); }, 'pending'), location: '/api/v1/orders/ord_8Kq2Tz' };
    case 'orders.get': return { status: 200, body: orderBody(v, opts.attempt && opts.attempt < 3 ? 'pending' : 'paid') };
    case 'orders.cancel': return { status: 200, body: { id: v('orderId'), status: 'cancelled' } };
    case 'orders.refund': return { status: 202, body: { id: 'rf_19Xc', orderId: v('orderId'), amount: b.amount, status: 'pending' } };
    case 'carts.create': return { status: 201, body: { id: 'c_51Hx9w', userId: b.userId, items: [], currency: 'EUR' }, location: '/api/v1/carts/c_51Hx9w' };
    case 'carts.addItem': return { status: 201, body: { cartId: v('cartId'), sku: b.sku, quantity: b.quantity, itemCount: (opts.iteration || 0) + 1 } };
    case 'carts.get': return { status: 200, body: { id: v('cartId'), userId: v('userId'), items: [{ sku: 'SKU-1', quantity: 1 }, { sku: 'SKU-2', quantity: 1 }], currency: 'EUR' } };
    case 'catalog.seedProducts': return opts.profile === 'staging' ? { status: 503, body: { error: 'unavailable', message: 'catalog-db is read-only during reindex' } } : { status: 201, body: { seeded: 2, productIds: ['SKU-1', 'SKU-2'] } };
    case 'catalog.getProduct': return { status: 200, body: { sku: v('sku'), name: v('sku') === 'SKU-2' ? 'Lamp shade' : 'Desk lamp', price: v('sku') === 'SKU-2' ? 34.99 : 24.99, currency: 'EUR', stock: 42 } };
    case 'catalog.search': return { status: 200, body: { query: v('query'), items: [{ sku: 'SKU-1', name: 'Desk lamp', price: 24.99 }, { sku: 'SKU-2', name: 'Lamp shade', price: 34.99 }], total: 2 } };
    case 'geo.createCity': return { status: 201, body: { id: 'city_4410', name: b.name, country: b.country } };
    case 'geo.deleteCity': return { status: 204, body: null };
    case 'geo.createLocationGroup': return { status: 201, body: { id: 'grp_77Ka', name: b.name, cityIds: b.cityIds } };
    case 'payments.authorize': return { status: 200, body: { id: 'pay_3Lr8Vq', orderId: b.orderId, status: 'authorized', amount: 64.78, currency: 'EUR' } };
    case 'payments.listPaymentMethods': return { status: 200, body: { items: [{ type: 'card', label: 'Card ending 4242' }, { type: 'sepa', label: 'SEPA direct debit' }] } };
    case 'payments.capture': return { status: 200, body: { id: v('paymentId'), status: 'captured' } };
    case 'coupons.validate': return { status: 200, body: { code: b.code, valid: true, discount: 0 } };
    default:
      if (id) return { status: 200, body: { ok: true } };
      return { status: 404, body: { error: 'not_found', message: 'No route for this path on the target host' } };
  }
}
var STATUS_TEXT = { 200: 'OK', 201: 'Created', 202: 'Accepted', 204: 'No Content', 400: 'Bad Request', 404: 'Not Found', 409: 'Conflict', 503: 'Service Unavailable' };

function statusExpected(exp) { return Array.isArray(exp) ? exp : [exp]; }
function apiChecks(found, ctx) {
  var out = [];
  if (!found) return out;
  var ex = found.api.expect || {};
  if (ex.status !== undefined) out.push({ kind: 'status', label: 'status', op: 'in', expected: statusExpected(ex.status), source: 'api' });
  Object.keys(ex.match || {}).forEach(function (k) { out.push({ kind: 'body', label: 'body.match.' + k, path: k, op: 'eq', expected: resolveValue(ex.match[k], ctx, 0), source: 'api' }); });
  return out;
}
function extraToCheck(c) {
  if (c.kind === 'status') return { kind: 'status', label: 'status', op: 'in', expected: [Number(c.value)], source: 'added' };
  if (c.kind === 'timing') return { kind: 'timing', label: 'timing.' + (c.path || 'total'), path: c.path || 'total', op: c.op || 'lt', expected: Number(c.value), source: 'added' };
  return { kind: 'body', label: 'body.' + c.path, path: c.path, op: c.op || 'eq', expected: c.value, source: 'added' };
}
function fmt(v) { return v === undefined ? '(missing)' : typeof v === 'object' ? JSON.stringify(v) : String(v); }
function looseEq(a, b) {
  if (a === b) return true;
  if (typeof a === 'number' || typeof b === 'number') return Number(a) === Number(b) && String(a).trim() !== '' && String(b).trim() !== '';
  return fmt(a) === fmt(b);
}
function evalCheck(c, resp) {
  var actual, pass;
  if (c.kind === 'status') { actual = resp.status; pass = c.expected.some(function (e) { return Number(e) === resp.status; }); }
  else if (c.kind === 'timing') { actual = resp.timing[c.path]; pass = c.op === 'gt' ? actual > c.expected : actual < c.expected; }
  else {
    actual = lookupPath(resp.body, c.path);
    if (c.op === 'exists') pass = actual !== undefined && actual !== null;
    else if (c.op === 'contains') pass = actual !== undefined && fmt(actual).indexOf(String(c.expected)) >= 0;
    else if (c.op === 'lt') pass = Number(actual) < Number(c.expected);
    else if (c.op === 'gt') pass = Number(actual) > Number(c.expected);
    else pass = looseEq(actual, c.expected);
  }
  var expText = c.kind === 'status' ? c.expected.join(' or ') : c.op === 'exists' ? 'exists' : (c.op === 'eq' ? '' : c.op + ' ') + fmt(c.expected) + (c.kind === 'timing' ? ' ms' : '');
  return { label: c.label, expected: expText, actual: fmt(actual) + (c.kind === 'timing' ? ' ms' : ''), pass: pass, source: c.source };
}
function applySaves(saves, resp) {
  return (saves || []).filter(function (s) { return s.name; }).map(function (s) {
    var miss = [];
    var val = resolveValue(s.expr, { response: { body: resp.body, headers: resp.headerMap, status: resp.status }, missing: miss }, 0);
    return { name: s.name, expr: s.expr, value: miss.length ? undefined : val, ok: !miss.length };
  });
}
function execRequest(req, env, savedVals, opts) {
  opts = opts || {};
  var built = buildRequest(req, Object.assign({}, env, { item: opts.item, iteration: opts.iteration }), savedVals);
  if (built.missing.length) return { error: 'missing', missing: built.missing, built: built };
  if (built.bodyErr) return { error: 'body', message: built.bodyErr, built: built };
  var found = req.apiId ? findApi(env.catalog, req.apiId) : null;
  if (!found) {
    var m = matchApiByUrl(env.catalog, req.method, built.url, Object.assign({}, env.config.vars, env.config.profiles[env.profile]));
    if (m) found = m;
  }
  var id = found ? found.fullId : null;
  if (found && req.method !== (found.api.method || 'GET')) id = null;
  var c = cannedResponse(id, built.vars, built.body, { attempt: opts.attempt, iteration: opts.iteration, profile: env.profile, deactivated: opts.deactivated });
  var bodyText = c.body === null ? '' : JSON.stringify(c.body, null, 2);
  var size = bodyText.length;
  var timing = timingFor(id, env.profile, opts.attempt);
  var headers = [['content-type', 'application/json; charset=utf-8'], ['content-length', String(size)], ['date', 'Fri, 23 Oct 2026 14:2' + (opts.attempt || 1) + ':07 GMT'], ['x-request-id', 'req_' + (id ? id.replace('.', '_') : 'none') + '_' + (opts.iteration || 0)], ['server-timing', 'app;dur=' + Math.max(1, timing.firstByte - 9)]];
  if (c.location) headers.push(['location', c.location]);
  if (c.body === null) headers = headers.filter(function (h) { return h[0] !== 'content-type'; });
  var headerMap = {};
  headers.forEach(function (h) { headerMap[h[0]] = h[1]; });
  var resp = { status: c.status, statusText: STATUS_TEXT[c.status] || '', body: c.body, bodyText: bodyText, headers: headers, headerMap: headerMap, size: size, timing: timing, url: built.url, method: req.method, apiId: id };
  var checks = apiChecks(found, built.ctx).concat((req.extraChecks || []).map(extraToCheck));
  resp.checks = checks.map(function (ch) { return evalCheck(ch, resp); });
  resp.saved = applySaves(req.saves, resp);
  resp.failed = resp.checks.filter(function (x) { return !x.pass; }).length;
  resp.reqHeaders = built.headers;
  resp.reqBody = built.bodyText;
  return resp;
}
function fmtSize(n) { return n < 1024 ? n + ' B' : (n / 1024).toFixed(1) + ' KB'; }

/* ---------- flows ---------- */
function stageReq(stage, env, key) {
  var found = findApi(env.catalog, stage.api);
  if (!found) return null;
  var r = reqFromApi(found, key || stage.uid);
  r.callVars = clone(stage.vars || {});
  return r;
}
function loopItems(stage) {
  var l = stage.loop || { kind: 'once' };
  if (l.kind === 'forEach') return String(l.items || '').split(',').map(function (s) { return s.trim(); }).filter(Boolean);
  if (l.kind === 'repeat') { var n = Math.max(1, Math.min(10, Number(l.count) || 2)), a = []; for (var i = 0; i < n; i++) a.push(i); return a; }
  return null;
}
function runStage(stage, env, saved) {
  var req = stageReq(stage, env);
  if (!req) return { status: 'failed', note: 'unknown api ' + stage.api, time: 0, saved: {}, checks: [] };
  var items = loopItems(stage);
  var iters = items ? items : [undefined];
  var total = 0, failed = 0, last = null, checks = [], newSaved = {}, attempts = 0, note = '', missing = null;
  for (var i = 0; i < iters.length; i++) {
    var vals = Object.assign({}, saved, newSaved);
    var opts = { item: iters[i], iteration: items ? i : undefined, deactivated: !!saved.__deactivated };
    var resp;
    if (stage.poll) {
      var attempt = 0;
      do { attempt++; opts.attempt = attempt; resp = execRequest(req, env, vals, opts); if (resp.error) break; total += resp.timing.total + (attempt > 1 ? 1000 : 0); }
      while (attempt < 10 && String(lookupPath(resp.body, stage.poll.field)) !== String(stage.poll.equals));
      attempts = attempt;
    } else {
      resp = execRequest(req, env, vals, opts);
      if (!resp.error) total += resp.timing.total;
    }
    if (resp.error) { missing = resp.missing || [resp.message]; failed++; break; }
    last = resp;
    resp.checks.forEach(function (c) { if (!c.pass) failed++; });
    checks = resp.checks;
    resp.saved.forEach(function (s) { if (s.ok) newSaved[s.name] = s.value; });
    if (stage.api === 'users.deactivate' && resp.status === 200) newSaved.__deactivated = true;
  }
  if (missing) note = 'not sent: ${' + missing[0] + '} is not set';
  else if (items && stage.loop.kind === 'forEach') note = '×' + items.length + ' · ' + items.join(', ');
  else if (items) note = '×' + items.length;
  if (stage.poll && !missing) note = attempts + ' polls · ' + stage.poll.field + ' ' + lookupPath(last.body, stage.poll.field);
  var failedCheck = checks.filter(function (c) { return !c.pass; })[0];
  if (failedCheck) note = failedCheck.label.replace('body.match.', '') + ' ' + failedCheck.actual + ', expected ' + failedCheck.expected;
  return { status: failed ? 'failed' : 'passed', code: last ? last.status : null, time: total, note: note, saved: newSaved, checks: checks, response: last, missing: missing };
}
function runFlow(flow, env, onlyUid, prev) {
  var results = prev ? clone(prev.results) : {}, snapshots = prev ? clone(prev.snapshots) : {};
  var saved = {}, failedAt = -1;
  for (var i = 0; i < flow.stages.length; i++) {
    var st = flow.stages[i];
    if (onlyUid && st.uid !== onlyUid) { if (snapshots[st.uid]) saved = clone(snapshots[st.uid]); continue; }
    if (!onlyUid && failedAt >= 0 && flow.stopOnFail) { results[st.uid] = { status: 'skipped', note: 'not run: stage ' + (failedAt + 1) + ' failed', time: 0, saved: {}, checks: [] }; delete snapshots[st.uid]; continue; }
    var r = runStage(st, env, saved);
    Object.assign(saved, r.saved);
    results[st.uid] = r;
    snapshots[st.uid] = clone(saved);
    if (r.status === 'failed' && failedAt < 0) failedAt = i;
    if (onlyUid) break;
  }
  return { results: results, snapshots: snapshots };
}
function stageInfo(flow, catalog) {
  var producedBy = {};
  return flow.stages.map(function (st, i) {
    var found = findApi(catalog, st.api);
    var consumes = [], saves = [];
    if (found) {
      var req = reqFromApi(found, 'x');
      var own = Object.keys(found.api.vars || {});
      var refs = [];
      requestStrings(req).forEach(function (s) { varRefs(s).forEach(function (n) { if (refs.indexOf(n) < 0) refs.push(n); }); });
      Object.keys(st.vars || {}).forEach(function (k) { varRefs(st.vars[k]).forEach(function (n) { if (refs.indexOf(n) < 0) refs.push(n); }); });
      refs.forEach(function (n) {
        if (/Url$/.test(n)) return;
        if (own.indexOf(n) >= 0 && !(st.vars || {})[n] && producedBy[n] === undefined) return;
        var from = producedBy[n] !== undefined ? 'stage ' + (producedBy[n] + 1) : (st.vars || {})[n] !== undefined ? 'stage var' : (found.file.vars || {})[n] !== undefined ? found.file.id + ' file' : own.indexOf(n) >= 0 ? 'api default' : 'config';
        consumes.push({ name: n, from: from, chained: producedBy[n] !== undefined });
      });
      saves = Object.keys(found.api.save || {});
      saves.forEach(function (n) { producedBy[n] = i; });
    }
    return { uid: st.uid, found: !!found, method: found ? found.api.method || 'GET' : '?', consumes: consumes, saves: saves };
  });
}
function flowVarNames(flow, catalog) {
  var names = [];
  stageInfo(flow, catalog).forEach(function (s) {
    s.consumes.forEach(function (c) { if (names.indexOf(c.name) < 0) names.push(c.name); });
    s.saves.forEach(function (n) { if (names.indexOf(n) < 0) names.push(n); });
  });
  return names;
}

/* ---------- YAML ---------- */
function yNeedsQuote(s, flow) {
  if (s === '') return true;
  if (/^\s|\s$/.test(s)) return true;
  if (/^[\[\]{}#&*!|>'"%@,?:`]/.test(s)) return true;
  if (s === '-' || /^- /.test(s) || /^-{3}/.test(s)) return true;
  if (/: |:$| #/.test(s)) return true;
  if (/^(true|false|null|yes|no|on|off|y|n|~)$/i.test(s)) return true;
  if (/^[-+]?(\d[\d_]*)?(\.\d*)?([eE][-+]?\d+)?$/.test(s) && /\d/.test(s)) return true;
  if (/^[-+]?\.(inf|nan)$/i.test(s) || /^0[xo]/i.test(s)) return true;
  if (/^\d{4}-\d\d-\d\d/.test(s)) return true;
  if (flow && /[,\[\]{}]/.test(s)) return true;
  if (/\n/.test(s)) return true;
  return false;
}
function yScalar(v, flow) {
  if (v === null || v === undefined) return 'null';
  if (typeof v === 'number' || typeof v === 'boolean') return String(v);
  var s = String(v);
  if (/\n/.test(s)) return JSON.stringify(s);
  return yNeedsQuote(s, flow) ? "'" + s.replace(/'/g, "''") + "'" : s;
}
function yKey(k) { return /^[A-Za-z_$][\w$.\-]*$/.test(k) && !/^(true|false|null|yes|no|on|off|y|n)$/i.test(k) ? k : "'" + String(k).replace(/'/g, "''") + "'"; }
function isScalar(v) { return v === null || typeof v !== 'object'; }
function toYaml(v, indent) {
  indent = indent || 0;
  var pad = new Array(indent + 1).join(' ');
  var lines = [];
  if (Array.isArray(v)) {
    if (!v.length) return pad + '[]';
    v.forEach(function (item) {
      if (isScalar(item)) lines.push(pad + '- ' + yScalar(item));
      else if (Array.isArray(item) && (!item.length || item.every(isScalar))) lines.push(pad + '- ' + flowSeq(item));
      else {
        if (!Array.isArray(item) && !Object.keys(item).length) { lines.push(pad + '- {}'); return; }
        var sub = toYaml(item, indent + 2).split('\n');
        lines.push(pad + '- ' + sub[0].slice(indent + 2));
        for (var i = 1; i < sub.length; i++) lines.push(sub[i]);
      }
    });
    return lines.join('\n');
  }
  Object.keys(v).forEach(function (k) {
    var x = v[k];
    if (x === undefined) return;
    if (isScalar(x)) lines.push(pad + yKey(k) + ': ' + yScalar(x));
    else if (Array.isArray(x)) {
      if (!x.length) lines.push(pad + yKey(k) + ': []');
      else if (x.every(isScalar) && flowSeq(x).length <= 60) lines.push(pad + yKey(k) + ': ' + flowSeq(x));
      else { lines.push(pad + yKey(k) + ':'); lines.push(toYaml(x, indent + 2)); }
    } else {
      if (!Object.keys(x).length) lines.push(pad + yKey(k) + ': {}');
      else { lines.push(pad + yKey(k) + ':'); lines.push(toYaml(x, indent + 2)); }
    }
  });
  return lines.join('\n');
}
function flowSeq(arr) { return '[' + arr.map(function (x) { return yScalar(x, true); }).join(', ') + ']'; }

function apiToObj(api) {
  var o = { id: api.id, name: api.name };
  if (api.method && api.method !== 'GET') o.method = api.method; else o.method = undefined;
  if (api.method === 'GET') o.method = undefined;
  o.path = api.path;
  ['query', 'headers', 'vars', 'body', 'expect', 'save'].forEach(function (k) {
    if (api[k] === undefined || api[k] === null) return;
    if (typeof api[k] === 'object' && !Array.isArray(api[k]) && !Object.keys(api[k]).length) return;
    if (k === 'expect') {
      var e = {};
      if (api.expect.status !== undefined) e.status = api.expect.status;
      if (api.expect.match) e.body = { match: api.expect.match };
      o.expect = e;
    } else o[k] = api[k];
  });
  if (o.method === undefined) delete o.method;
  return o;
}
function apiFileYaml(file) {
  var o = { id: file.id, name: file.name, baseUrl: file.baseUrl, auth: file.auth };
  if (Object.keys(file.headers || {}).length) o.headers = file.headers;
  if (Object.keys(file.vars || {}).length) o.vars = file.vars;
  o.apis = file.apis.map(apiToObj);
  return toYaml(o);
}

/* ---------- tokenizers for display ---------- */
function mergeSegs(segs) {
  var out = [];
  segs.forEach(function (s) { if (!s.t) return; var l = out[out.length - 1]; if (l && l.cls === s.cls) l.t += s.t; else out.push({ t: s.t, cls: s.cls }); });
  return out;
}
function tokenizeJson(line) {
  var re = /("(?:[^"\\]|\\.)*")(\s*:)?|(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)|(true|false|null)|([{}\[\],])|(\s+)|([^\s])/g, m, segs = [];
  while ((m = re.exec(line))) {
    if (m[1]) { if (m[2]) { segs.push({ t: m[1], cls: 'y-key' }); segs.push({ t: m[2], cls: 'y-punc' }); } else segs.push({ t: m[1], cls: /\$\{/.test(m[1]) ? 'y-tpl' : 'y-str' }); }
    else if (m[3]) segs.push({ t: m[3], cls: 'y-num' });
    else if (m[4]) segs.push({ t: m[4], cls: 'y-num' });
    else if (m[5]) segs.push({ t: m[5], cls: 'y-punc' });
    else if (m[6]) segs.push({ t: m[6], cls: 'ws' });
    else segs.push({ t: m[7], cls: 'y-str' });
  }
  return mergeSegs(segs);
}
function yValueSegs(v) {
  var segs = [];
  var hash = v.search(/\s#\s/);
  var com = '';
  if (hash >= 0 && !/^['"]/.test(v)) { com = v.slice(hash); v = v.slice(0, hash); }
  if (/^-?\d+(\.\d+)?$/.test(v.trim()) || /^(true|false|null)$/.test(v.trim())) segs.push({ t: v, cls: 'y-num' });
  else {
    var parts = v.split(/(\$\{[^}]*\}|[\[\],{}])/);
    parts.forEach(function (p) {
      if (!p) return;
      if (/^\$\{/.test(p)) segs.push({ t: p, cls: 'y-tpl' });
      else if (/^[\[\],{}]$/.test(p)) segs.push({ t: p, cls: 'y-punc' });
      else if (/^\s*-?\d+(\.\d+)?\s*$/.test(p)) segs.push({ t: p, cls: 'y-num' });
      else segs.push({ t: p, cls: 'y-str' });
    });
  }
  if (com) segs.push({ t: com, cls: 'y-com' });
  return segs;
}
function tokenizeYaml(line) {
  if (/^\s*#/.test(line)) return [{ t: line, cls: 'y-com' }];
  var m = /^(\s*)((?:- )*)([^\s'"#\-\[{][^:]*?|'[^']*'|"[^"]*")(:)(\s+|$)(.*)$/.exec(line);
  if (m) {
    var segs = [{ t: m[1], cls: 'ws' }, { t: m[2], cls: 'y-punc' }, { t: m[3], cls: 'y-key' }, { t: m[4], cls: 'y-punc' }, { t: m[5], cls: 'ws' }];
    return mergeSegs(segs.concat(yValueSegs(m[6])));
  }
  var n = /^(\s*)((?:- )*)(.*)$/.exec(line);
  return mergeSegs([{ t: n[1], cls: 'ws' }, { t: n[2], cls: 'y-punc' }].concat(yValueSegs(n[3])));
}
function codeLines(text, lang) {
  return String(text).split('\n').map(function (l, i) { return { no: i + 1, segs: lang === 'json' ? tokenizeJson(l) : lang === 'plain' ? [{ t: l, cls: 'y-str' }] : tokenizeYaml(l), cls: 'ln' }; });
}

/* ---------- diff ---------- */
function lineDiff(a, b) {
  var A = String(a).split('\n'), B = String(b).split('\n');
  if (a === '') A = [];
  var n = A.length, m = B.length, dp = [];
  for (var i = 0; i <= n; i++) { dp.push(new Array(m + 1).fill(0)); }
  for (i = n - 1; i >= 0; i--) for (var j = m - 1; j >= 0; j--) dp[i][j] = A[i] === B[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
  var out = [], x = 0, y = 0;
  while (x < n && y < m) {
    if (A[x] === B[y]) { out.push({ op: ' ', text: A[x], a: x + 1, b: y + 1 }); x++; y++; }
    else if (dp[x + 1][y] >= dp[x][y + 1]) { out.push({ op: '-', text: A[x], a: x + 1 }); x++; }
    else { out.push({ op: '+', text: B[y], b: y + 1 }); y++; }
  }
  while (x < n) { out.push({ op: '-', text: A[x], a: x + 1 }); x++; }
  while (y < m) { out.push({ op: '+', text: B[y], b: y + 1 }); y++; }
  return out;
}
function diffStats(d) { return { add: d.filter(function (l) { return l.op === '+'; }).length, del: d.filter(function (l) { return l.op === '-'; }).length }; }
function diffHunks(d, ctxN) {
  ctxN = ctxN === undefined ? 3 : ctxN;
  var keep = d.map(function () { return false; });
  d.forEach(function (l, i) { if (l.op !== ' ') for (var k = Math.max(0, i - ctxN); k <= Math.min(d.length - 1, i + ctxN); k++) keep[k] = true; });
  var out = [], skipped = 0;
  d.forEach(function (l, i) {
    if (keep[i]) { if (skipped) { out.push({ gap: true, text: skipped + ' unchanged lines' }); skipped = 0; } out.push(l); }
    else skipped++;
  });
  if (skipped) out.push({ gap: true, text: skipped + ' unchanged lines' });
  return out;
}
function diffView(before, after) {
  var d = lineDiff(before, after);
  var rows = diffHunks(d, 3).map(function (l) {
    if (l.gap) return { cls: 'ln gap', no: '', sign: '', segs: [{ t: '⋯ ' + l.text, cls: 'y-com' }] };
    return { cls: 'ln' + (l.op === '+' ? ' add' : l.op === '-' ? ' del' : ''), no: l.op === '-' ? l.a : l.b, sign: l.op === ' ' ? '' : l.op, segs: tokenizeYaml(l.text) };
  });
  var st = diffStats(d);
  return { rows: rows, add: st.add, del: st.del };
}

/* ---------- save as test ---------- */
function deriveTestId(name) { return toCamel(name, true); }
function testIdWarning(id) {
  if (!id) return '';
  var first = words(id)[0] || '';
  return /s$/i.test(first) ? '' : 'Test ids describe behaviour in third person, like createsMember or rejectsBadEmail.';
}
var TEST_FILES = [
  { id: 'users.crud', path: 'services/users/crud.test.yaml' }, { id: 'users.reports', path: 'services/users/reports.test.yaml' },
  { id: 'orders.checkout', path: 'services/orders/checkout.test.yaml' }, { id: 'orders.history', path: 'services/orders/history.test.yaml' },
  { id: 'catalog.admin', path: 'services/catalog/admin.test.yaml' }, { id: 'catalog.browse', path: 'services/catalog/browse.test.yaml' },
  { id: 'catalog.search', path: 'services/catalog/search.test.yaml' }, { id: 'geo.groups', path: 'services/geo/groups.test.yaml' },
  { id: 'carts.pricing', path: 'services/carts/pricing.test.yaml' }
];
var SCHEMAS = { users: '../../schemas/users/user.json', orders: '../../schemas/orders/order.json' };
function testFilePath(fileId) {
  var f = TEST_FILES.filter(function (t) { return t.id === fileId; })[0];
  if (f) return f.path;
  var p = String(fileId).split('.');
  return 'services/' + p[0] + '/' + p.slice(1).join('.') + '.test.yaml';
}
function suggestExpectations(resp, apiFullId) {
  var out = [];
  if (!resp || resp.error) return out;
  out.push({ key: 'status', label: 'status: ' + resp.status, on: true });
  var svc = String(apiFullId || '').split('.')[0];
  if (SCHEMAS[svc] && resp.body && typeof resp.body === 'object' && !Array.isArray(resp.body.items)) out.push({ key: 'schema', label: 'body.schema: ' + SCHEMAS[svc].replace('../../', ''), on: true, value: SCHEMAS[svc] });
  if (resp.body && typeof resp.body === 'object') {
    Object.keys(resp.body).forEach(function (k) {
      var v = resp.body[k];
      if (!isScalar(v) || k === 'id' || /At$/.test(k) || /Id$/.test(k)) return;
      if (k === 'name' || k === 'email') return;
      out.push({ key: 'match.' + k, label: 'body.match.' + k + ': ' + yScalar(v), on: ['role', 'status', 'total', 'valid', 'currency'].indexOf(k) >= 0, value: v, path: k });
    });
  }
  var budget = Math.max(100, Math.ceil(resp.timing.total * 3 / 100) * 100);
  out.push({ key: 'timing', label: 'timing.total: lt ' + budget + 'ms', on: true, value: budget + 'ms' });
  return out;
}
function testFromRequest(opts) {
  var t = { id: opts.id, name: opts.name, call: opts.apiId };
  if (opts.vars && Object.keys(opts.vars).length) t.vars = opts.vars;
  var ex = {};
  (opts.expectations || []).filter(function (e) { return e.on; }).forEach(function (e) {
    if (e.key === 'status') ex.status = opts.status;
    else if (e.key === 'schema') { ex.body = ex.body || {}; ex.body.schema = { $file: e.value }; }
    else if (e.key.indexOf('match.') === 0) { ex.body = ex.body || {}; ex.body.match = ex.body.match || {}; ex.body.match[e.path] = e.value; }
    else if (e.key === 'timing') ex.timing = { total: { lt: e.value } };
  });
  if (Object.keys(ex).length) t.expect = ex;
  if (opts.saves && opts.saves.length) { t.save = {}; opts.saves.forEach(function (s) { t.save[s.name] = s.expr; }); }
  return t;
}
function testBlockYaml(test) { return toYaml({ tests: [test] }).split('\n').slice(1).join('\n'); }
function stageToStep(st) {
  var s = { call: st.api };
  var l = st.loop || { kind: 'once' };
  if (l.kind === 'forEach') s.forEach = loopItems(st);
  if (l.kind === 'repeat') s.repeat = loopItems(st).length;
  if (st.vars && Object.keys(st.vars).length) s.vars = clone(st.vars);
  if (st.poll) { var u = {}; u[st.poll.field] = st.poll.equals; s.poll = { until: { body: { match: u } }, every: st.poll.every || '1s', timeout: st.poll.timeout || '30s' }; }
  return s;
}
function flowToTest(flow, id, name) { return { id: id, name: name, steps: flow.stages.map(stageToStep) }; }
function flowToLifecycle(flow, n, id, name) {
  return { beforeEach: flow.stages.slice(0, n).map(stageToStep), test: { id: id, name: name, steps: flow.stages.slice(n).map(stageToStep) } };
}

/* ---------- cURL ---------- */
function shellSplit(s) {
  var out = [], buf = '', q = null, has = false;
  s = String(s).replace(/\\\r?\n/g, ' ');
  for (var i = 0; i < s.length; i++) {
    var c = s[i];
    if (q === "'") { if (c === "'") q = null; else buf += c; continue; }
    if (q === '"') { if (c === '\\' && i + 1 < s.length) { buf += s[++i]; continue; } if (c === '"') q = null; else buf += c; continue; }
    if (c === "'" || c === '"') { q = c; has = true; continue; }
    if (c === '\\' && i + 1 < s.length) { buf += s[++i]; has = true; continue; }
    if (/\s/.test(c)) { if (buf || has) { out.push(buf); buf = ''; has = false; } continue; }
    buf += c; has = true;
  }
  if (buf || has) out.push(buf);
  return out;
}
function parseCurl(text) {
  var t = shellSplit(text);
  if (!t.length || t[0] !== 'curl') return { error: 'Paste a command that starts with curl' };
  var r = { method: null, url: null, headers: [], body: null, user: null };
  for (var i = 1; i < t.length; i++) {
    var a = t[i];
    if (a === '-X' || a === '--request') r.method = String(t[++i]).toUpperCase();
    else if (a === '-H' || a === '--header') { var h = t[++i] || '', c = h.indexOf(':'); if (c > 0) r.headers.push([h.slice(0, c).trim().toLowerCase(), h.slice(c + 1).trim()]); }
    else if (a === '-d' || a === '--data' || a === '--data-raw' || a === '--data-binary' || a === '--json') { r.body = t[++i]; if (a === '--json') r.headers.push(['content-type', 'application/json']); }
    else if (a === '-u' || a === '--user') r.user = t[++i];
    else if (a === '--url') r.url = t[++i];
    else if (/^-/.test(a)) { if (['-o', '--output', '-A', '--user-agent', '-e', '--referer', '-b', '--cookie'].indexOf(a) >= 0) i++; }
    else if (!r.url) r.url = a;
  }
  if (!r.url) return { error: 'No URL found in the command' };
  if (!r.method) r.method = r.body !== null ? 'POST' : 'GET';
  return r;
}

/* ---------- import: shared ---------- */
function isIdLike(seg) {
  if (!seg) return false;
  if (/^\d+$/.test(seg)) return true;
  if (/^[0-9a-f]{8}-[0-9a-f]{4}-/i.test(seg)) return true;
  if (/\d/.test(seg) && /[_\-]/.test(seg)) return true;
  if (/\d/.test(seg) && /[A-Z]/.test(seg) && /[a-z]/.test(seg)) return true;
  return false;
}
function splitBase(path) {
  var m = /^(\/api\/v\d+)(\/.*)?$/.exec(path);
  return m ? { base: m[1], rest: m[2] || '/' } : { base: '', rest: path };
}
function templatePath(rest, known) {
  var segs = rest.split('/');
  var params = [];
  var out = segs.map(function (s, i) {
    if (!isIdLike(s)) return s;
    var name = known && known[s] ? known[s] : singular(toCamel(segs[i - 1] || 'item')) + 'Id';
    params.push({ name: name, recorded: s });
    return '${var.' + name + '}';
  });
  return { path: out.join('/'), params: params };
}
function deriveApiId(method, path, service) {
  var segs = String(path).split('/').filter(Boolean);
  if (!segs.length) return toCamel(method.toLowerCase());
  var isParam = function (s) { return /^\$\{/.test(s) || /^\{.*\}$/.test(s); };
  var last = segs[segs.length - 1], lastIsParam = isParam(last);
  var resource = lastIsParam ? (segs[segs.length - 2] || service) : last;
  var afterParam = !lastIsParam && segs.length > 1 && isParam(segs[segs.length - 2]);
  var resCamel = toCamel(resource);
  var same = singular(resCamel.toLowerCase()) === singular(String(service).toLowerCase());
  var obj = function (plural) { return same ? '' : cap(plural ? resCamel : singular(resCamel)); };
  var m = String(method).toUpperCase();
  if (lastIsParam) {
    if (m === 'GET') return 'get' + obj(false);
    if (m === 'PUT' || m === 'PATCH') return 'update' + obj(false);
    if (m === 'DELETE') return 'delete' + obj(false);
    return m.toLowerCase() + obj(false);
  }
  if (!isPlural(resource)) return resCamel;
  if (afterParam) {
    var sub = cap(singular(resCamel));
    if (m === 'POST') return 'add' + sub;
    if (m === 'GET') return 'list' + cap(resCamel);
    if (m === 'DELETE') return 'remove' + sub;
    return 'update' + sub;
  }
  if (m === 'GET') return 'list' + obj(true);
  if (m === 'POST') return 'create' + obj(false);
  if (m === 'DELETE') return 'deleteAll' + obj(true);
  return 'update' + obj(true);
}
function nameFromId(id, service) {
  var w = words(id).map(function (x) { return x.toLowerCase(); });
  if (w.length === 1) w.push(singular(String(service)));
  return cap(w.join(' '));
}

/* ---------- import: HAR ---------- */
var AUTHZ = 'Bearer eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1XzdmM2syOXFwIn0.k3Vt';
var COOKIE = 'session=s%3A9bQ1xLw0e7.Tk2; cart_hint=c_51Hx9w';
function apiEntry(method, url, status, time, reqBody, respBody) {
  return { method: method, url: url, status: status, time: time, mime: 'application/json', reqHeaders: [['authorization', AUTHZ], ['cookie', COOKIE], ['accept', 'application/json']].concat(reqBody ? [['content-type', 'application/json']] : []), reqBody: reqBody || null, respBody: respBody || null };
}
function harEntries() {
  var S = 'https://shop.staging.shop.example/assets/';
  var st = function (path, mime, time) { return { method: 'GET', url: S + path, status: 200, time: time, mime: mime }; };
  var tp = function (method, url, mime, time) { return { method: method, url: url, status: method === 'POST' ? 204 : 200, time: time, mime: mime }; };
  var pf = function (url, time) { return { method: 'OPTIONS', url: url, status: 204, time: time, mime: '' }; };
  var U = 'https://users.staging.shop.example/api/v1', C = 'https://carts.staging.shop.example/api/v1', O = 'https://orders.staging.shop.example/api/v1',
    P = 'https://payments.staging.shop.example/api/v1', K = 'https://catalog.staging.shop.example/api/v1', Q = 'https://coupons.staging.shop.example/api/v1';
  var order = function (status) { return { id: 'ord_8Kq2Tz', status: status, total: 64.78, currency: 'EUR' }; };
  return [
    st('app.4f1c9a.js', 'application/javascript', 84), st('vendor.81b2e0.js', 'application/javascript', 131), st('runtime.0c13f5.js', 'application/javascript', 22),
    st('app.6e0b1d.css', 'text/css', 31), st('fonts/geist-400.woff2', 'font/woff2', 40), st('fonts/geist-600.woff2', 'font/woff2', 38),
    tp('GET', 'https://cdn.fontsrv.example/css2?family=Geist', 'text/css', 57), tp('POST', 'https://collect.metricsly.example/v1/event', 'text/plain', 112),
    st('img/logo.svg', 'image/svg+xml', 12), st('icons/user.svg', 'image/svg+xml', 9),
    pf(U + '/users', 18),
    apiEntry('POST', U + '/users', 201, 182, { name: 'Amara Okafor', email: 'amara.okafor@example.com', role: 'member' }, { id: 'u_7f3k29qp', name: 'Amara Okafor', email: 'amara.okafor@example.com', role: 'member' }),
    apiEntry('GET', U + '/users/u_7f3k29qp', 200, 64, null, { id: 'u_7f3k29qp', name: 'Amara Okafor', role: 'member' }),
    st('checkout.2d77aa.js', 'application/javascript', 66), st('checkout.93ac42.css', 'text/css', 19), st('fonts/geist-mono-400.woff2', 'font/woff2', 35),
    apiEntry('GET', K + '/products?q=desk+lamp', 200, 88, null, { items: [{ sku: 'SKU-1' }, { sku: 'SKU-2' }], total: 2 }),
    st('img/lamp-desk.webp', 'image/webp', 74), st('img/lamp-shade.webp', 'image/webp', 69),
    tp('GET', 'https://cdn.imgproxy.example/w_320/lamp-desk.webp', 'image/webp', 92),
    apiEntry('GET', K + '/products/SKU-1', 200, 41, null, { sku: 'SKU-1', name: 'Desk lamp', price: 24.99 }),
    pf(C + '/carts', 16),
    apiEntry('POST', C + '/carts', 201, 97, { userId: 'u_7f3k29qp' }, { id: 'c_51Hx9w', userId: 'u_7f3k29qp', items: [] }),
    pf(C + '/carts/c_51Hx9w/items', 15),
    apiEntry('POST', C + '/carts/c_51Hx9w/items', 201, 73, { sku: 'SKU-1', quantity: 1 }, { cartId: 'c_51Hx9w', itemCount: 1 }),
    apiEntry('POST', C + '/carts/c_51Hx9w/items', 201, 69, { sku: 'SKU-2', quantity: 1 }, { cartId: 'c_51Hx9w', itemCount: 2 }),
    tp('POST', 'https://collect.metricsly.example/v1/event', 'text/plain', 104),
    st('icons/cart.svg', 'image/svg+xml', 8), st('icons/lock.svg', 'image/svg+xml', 8), st('img/card-generic.svg', 'image/svg+xml', 11), st('img/sepa.svg', 'image/svg+xml', 10),
    apiEntry('POST', Q + '/coupons/validate', 200, 55, { code: 'WELCOME10', cartId: 'c_51Hx9w' }, { code: 'WELCOME10', valid: true }),
    apiEntry('GET', P + '/payment-methods?api_key=pk_stg_4eTq9LmZ2x', 200, 120, null, { items: [{ type: 'card' }, { type: 'sepa' }] }),
    pf(O + '/orders', 17),
    apiEntry('POST', O + '/orders', 201, 241, { cartId: 'c_51Hx9w', userId: 'u_7f3k29qp', currency: 'EUR', couponCode: 'WELCOME10' }, order('pending')),
    pf(P + '/payments/authorize', 14),
    apiEntry('POST', P + '/payments/authorize', 200, 410, { orderId: 'ord_8Kq2Tz', method: 'card' }, { id: 'pay_3Lr8Vq', status: 'authorized' }),
    tp('POST', 'https://collect.metricsly.example/v1/event', 'text/plain', 98),
    apiEntry('GET', O + '/orders/ord_8Kq2Tz', 200, 52, null, order('pending')),
    st('img/spinner.svg', 'image/svg+xml', 7),
    apiEntry('GET', O + '/orders/ord_8Kq2Tz', 200, 49, null, order('pending')),
    apiEntry('GET', O + '/orders/ord_8Kq2Tz', 200, 51, null, order('paid')),
    st('img/check.svg', 'image/svg+xml', 7), st('img/confetti.webp', 'image/webp', 58), st('favicon.ico', 'image/x-icon', 6),
    tp('POST', 'https://errors.trackly.example/api/envelope', 'application/json', 77)
  ].map(function (e, i) { e.idx = i; return e; });
}
function classifyEntry(e) {
  var u = splitUrl(e.url);
  if (e.method === 'OPTIONS') return 'preflight';
  if (!u || !/\.staging\.shop\.example$/.test(u.host)) return 'thirdParty';
  if (/^(text\/css|application\/javascript|image\/|font\/)/.test(e.mime || '') || /\.(css|js|svg|png|webp|ico|woff2?)$/.test(u.path)) return 'static';
  return 'api';
}
function harCounts(entries) {
  var c = { total: entries.length, static: 0, thirdParty: 0, preflight: 0, api: 0 };
  entries.forEach(function (e) { c[classifyEntry(e)]++; });
  return c;
}
function harVisible(entries, f) {
  return entries.filter(function (e) {
    var k = classifyEntry(e);
    if (k === 'static' && f.hideStatic) return false;
    if (k === 'thirdParty' && f.hideThird) return false;
    if (k === 'preflight' && f.hidePreflight) return false;
    return true;
  });
}
function harSelected(entries, f, excluded) {
  return harVisible(entries, f).filter(function (e) { return classifyEntry(e) === 'api' ? !excluded[e.idx] : !!(f.included || {})[e.idx]; });
}
function harMap(selected, catalog, overrides) {
  overrides = overrides || {};
  var ids = overrides.ids || {}, actions = overrides.actions || {};
  var known = {};
  var groups = [], byHost = {};
  selected.forEach(function (e) {
    var u = splitUrl(e.url);
    if (!u) return;
    var service = u.host.split('.')[0];
    var sb = splitBase(u.path);
    var tp = templatePath(sb.rest, known);
    var file = catalog.filter(function (f) { return f.id === service; })[0];
    var g = byHost[u.host];
    if (!g) {
      g = byHost[u.host] = { host: u.host, service: service, fileId: service, filePath: 'services/' + service + '/' + service + '.api.yaml', isNew: !file, base: sb.base, apis: [] };
      groups.push(g);
    }
    var shape = e.method + ' ' + shapeOf(tp.path);
    var existing = g.apis.filter(function (a) { return a.shape === shape; })[0];
    var query = {};
    if (u.query) u.query.split('&').forEach(function (kv) { var p = kv.split('='); query[decodeURIComponent(p[0])] = decodeURIComponent((p[1] || '').replace(/\+/g, ' ')); });
    var cur = existing;
    if (existing) { existing.count++; existing.entries.push(e.idx); existing.recorded.push(e); }
    else {
      var match = null;
      if (file) file.apis.forEach(function (a) { if ((a.method || 'GET') === e.method && shapeOf(a.path) === shapeOf(tp.path)) match = a; });
      var key = service + ':' + shape;
      var derived = match ? match.id : deriveApiId(e.method, tp.path, service);
      var recBody = e.reqBody && typeof e.reqBody === 'object' ? e.reqBody : null;
      var extraFields = [];
      if (match && recBody) extraFields = Object.keys(recBody).filter(function (k) { return !match.body || !(k in match.body); });
      var defAction = match ? (extraFields.length ? 'update' : 'skip') : 'add';
      var vars = {}, body = null, chained = [];
      if (recBody) {
        body = {};
        Object.keys(recBody).forEach(function (k) {
          var v = recBody[k];
          var kv = Object.keys(known).filter(function (x) { return x === v; })[0];
          if (kv) { body[k] = '${var.' + known[kv] + '}'; chained.push(known[kv]); }
          else if (isScalar(v)) { vars[k] = v; body[k] = '${var.' + k + '}'; }
          else body[k] = v;
        });
      }
      g.apis.push({ key: key, shape: shape, method: e.method, recordedPath: u.path, path: match ? match.path : tp.path, params: match ? [] : tp.params, detected: tp.params,
        proposedId: derived, id: ids[key] !== undefined ? ids[key] : derived, match: match ? service + '.' + match.id : null, action: actions[key] || defAction,
        extraFields: extraFields, count: 1, entries: [e.idx], recorded: [e], vars: vars, body: body, chained: chained, query: query, status: e.status, produces: null });
      cur = g.apis[g.apis.length - 1];
    }
    var rb = e.respBody;
    if (rb && rb.id && e.method === 'POST' && !/\$\{/.test(tp.path.split('/').pop())) {
      var res = tp.path.split('/').filter(Boolean)[0] || service;
      known[rb.id] = singular(toCamel(res)) + 'Id';
      cur.produces = known[rb.id];
    }
  });
  groups.forEach(function (g) {
    g.apis.forEach(function (a) {
      a.idValid = validId(a.id);
      a.idSuggestion = a.idValid ? '' : (toCamel(a.id) || a.proposedId);
      var dupe = g.apis.filter(function (b) { return b !== a && b.id === a.id && b.action !== 'skip'; }).length;
      var clash = !a.match && (catalog.filter(function (f) { return f.id === g.fileId; })[0] || { apis: [] }).apis.some(function (x) { return x.id === a.id; });
      a.idError = !a.idValid ? 'Ids are camelCase, verb first: try ' + a.idSuggestion : (dupe || clash) ? g.fileId + '.' + a.id + ' already exists' : '';
    });
  });
  return groups;
}
function harSecrets(selected, secretName) {
  var auth = selected.filter(function (e) { return (e.reqHeaders || []).some(function (h) { return h[0] === 'authorization'; }); }).length;
  var cookie = selected.filter(function (e) { return (e.reqHeaders || []).some(function (h) { return h[0] === 'cookie'; }); }).length;
  var keyed = selected.filter(function (e) { return /[?&](api_key|apikey|key|token)=/.test(e.url); });
  var out = [];
  if (auth) out.push({ key: 'authorization', kind: 'header', name: 'authorization', sample: 'Bearer eyJhbGciOiJSUzI1NiIs…', count: auth, outcome: 'Removed from every request. These hosts use auth shopOauth (oauth2 client credentials), so tokens are fetched at run time.' });
  if (cookie) out.push({ key: 'cookie', kind: 'cookie', name: 'session, cart_hint', sample: 'session=s%3A9bQ1…', count: cookie, outcome: 'Removed. Browser session cookies are never written to api files.' });
  keyed.forEach(function (e) {
    var u = splitUrl(e.url);
    var m = /[?&](api_key|apikey|key|token)=([^&]+)/.exec(e.url);
    out.push({ key: 'query:' + u.host, kind: 'query', name: m[1], host: u.host, sample: m[2].slice(0, 7) + '…', count: 1, secret: secretName, outcome: 'Replaced with ${secret.' + secretName + '}. Add ' + secretName + ' to .env.schema and set it in your local .env.' });
  });
  return out;
}
function harFlowSteps(groups, selected, keepReads) {
  var byIdx = {};
  groups.forEach(function (g) { g.apis.forEach(function (a) { a.entries.forEach(function (i) { byIdx[i] = { g: g, a: a }; }); }); });
  var steps = [];
  selected.forEach(function (e) {
    var m = byIdx[e.idx];
    if (!m) return;
    var call = m.g.fileId + '.' + (m.a.match && m.a.action === 'skip' ? m.a.match.split('.')[1] : m.a.id);
    var prev = steps[steps.length - 1];
    if (prev && prev.call === call) { prev.recorded.push(e); return; }
    steps.push({ call: call, method: e.method, recorded: [e], api: m.a });
  });
  steps = steps.map(function (s, i) {
    var step = { call: s.call };
    var n = s.recorded.length;
    if (n > 1 && s.method !== 'GET' && s.recorded[0].reqBody) {
      var diffKey = Object.keys(s.recorded[0].reqBody).filter(function (k) { return s.recorded.some(function (r) { return JSON.stringify(r.reqBody[k]) !== JSON.stringify(s.recorded[0].reqBody[k]); }); })[0];
      if (diffKey) { step.forEach = s.recorded.map(function (r) { return r.reqBody[diffKey]; }); step.vars = {}; step.vars[diffKey] = '${item}'; }
      else step.repeat = n;
    } else if (n > 1 && s.method === 'GET') {
      var lastB = s.recorded[n - 1].respBody || {}, firstB = s.recorded[0].respBody || {};
      var field = Object.keys(lastB).filter(function (k) { return isScalar(lastB[k]) && lastB[k] !== firstB[k]; })[0];
      if (field) { var u = {}; u[field] = lastB[field]; step.poll = { until: { body: { match: u } }, every: '1s', timeout: '30s' }; }
    }
    return { step: step, method: s.method, isLast: i === steps.length - 1, count: n };
  });
  if (!keepReads) steps = steps.filter(function (s) { return s.method !== 'GET' || s.isLast; });
  return steps.map(function (s) { return s.step; });
}
function chainSummary(groups) {
  var list = [];
  groups.forEach(function (g) { g.apis.forEach(function (a) { list.push({ id: g.fileId + '.' + (a.match ? a.match.split('.')[1] : a.id), a: a }); }); });
  return list.filter(function (x) { return x.a.produces; }).map(function (p) {
    var v = p.a.produces;
    var to = list.filter(function (x) {
      return x !== p && (x.a.chained.indexOf(v) >= 0 || varRefs(x.a.path).indexOf(v) >= 0 || (x.a.detected || []).some(function (d) { return d.name === v; }));
    }).map(function (x) { return x.id; });
    return { name: v, from: p.id, to: to };
  });
}
var ENV_SCHEMA_BEFORE = '# @defaultSensitive=false @defaultRequired=infer\n\n# OAuth client for shopOauth\n# @sensitive @required\nSHOP_CLIENT_ID=\n# @sensitive @required\nSHOP_CLIENT_SECRET=\n\n# @sensitive\nPARTNER_PASSWORD=';
var CHECKOUT_BEFORE = 'id: orders.checkout\nname: Checkout\ntags: [orders, checkout]\nbeforeAll:\n  - id: catalog\n    call: catalog.seedProducts\n    cache:\n      for: 12h\nbeforeEach:\n  - call: users.create\n  - call: carts.create\n  - call: carts.addItem\n    forEach: ${var.catalog.productIds}\n    vars:\n      sku: ${item}\ntests:\n  - id: placesOrder\n    name: A cart with two items becomes a paid order\n    steps:\n      - id: place\n        call: orders.place\n      - id: fetch\n        call: orders.get\n  - id: appliesCoupon\n    skip:\n      reason: coupon service flaky on staging, see #212\n      until: 2026-11-15\n    call: orders.place\n    vars:\n      couponCode: WELCOME10';
function configYaml(config) {
  var p = {};
  Object.keys(config.profiles).forEach(function (k) { p[k] = { vars: config.profiles[k] }; });
  return toYaml({ syntaxVersion: 2, project: 'shop', vars: config.vars, profiles: p });
}
function harApply(groups, catalog, secretName) {
  var cat = clone(catalog);
  groups.forEach(function (g) {
    var file = cat.filter(function (f) { return f.id === g.fileId; })[0];
    if (!file) {
      file = { id: g.fileId, name: cap(g.service) + ' service', path: g.filePath, baseUrl: '${var.' + g.service + 'Url}' + g.base, auth: 'shopOauth', headers: {}, vars: {}, apis: [], isNew: true };
      cat.push(file);
    }
    g.apis.forEach(function (a) {
      if (a.action === 'skip' || a.idError) return;
      if (a.match && a.action === 'update') {
        var ex = file.apis.filter(function (x) { return file.id + '.' + x.id === a.match; })[0];
        if (ex) {
          ex.vars = ex.vars || {};
          a.extraFields.forEach(function (k) { ex.body = ex.body || {}; ex.body[k] = '${var.' + k + '}'; ex.vars[k] = a.vars[k] !== undefined ? a.vars[k] : a.recorded[0].reqBody[k]; });
          ex.updated = true;
        }
        return;
      }
      var api = { id: a.id, name: nameFromId(a.id, g.service), method: a.method, path: a.path };
      var q = {};
      Object.keys(a.query).forEach(function (k) { q[k] = /^(api_key|apikey|key|token)$/.test(k) ? '${secret.' + (secretName || 'PAYMENTS_KEY') + '}' : a.query[k]; });
      if (Object.keys(q).length) api.query = q;
      if (Object.keys(a.vars).length) api.vars = clone(a.vars);
      if (a.body) api.body = clone(a.body);
      api.expect = { status: a.status };
      api.isNew = true;
      file.apis.push(api);
    });
  });
  return cat;
}
function harFiles(groups, catalog, opts) {
  var after = harApply(groups, catalog, opts.secretName);
  var files = [];
  after.forEach(function (f) {
    var before = catalog.filter(function (x) { return x.id === f.id; })[0];
    var strip = function (file) { var c = clone(file); c.apis.forEach(function (a) { delete a.isNew; delete a.updated; }); return c; };
    var a = apiFileYaml(strip(f));
    if (!before) files.push({ path: f.path, status: 'new', before: '', after: '# imported from checkout-session.har on 23 Oct 2026\n' + a });
    else { var b = apiFileYaml(strip(before)); if (a !== b) files.push({ path: f.path, status: 'changed', before: b, after: a }); }
  });
  var newHosts = groups.filter(function (g) { return g.isNew; });
  if (newHosts.length) {
    var cfgBefore = configYaml(opts.config);
    var c2 = clone(opts.config);
    newHosts.forEach(function (g) { c2.profiles.staging[g.service + 'Url'] = 'https://' + g.host; });
    files.push({ path: 'vibranium.config.yaml', status: 'changed', before: cfgBefore, after: configYaml(c2), note: 'Recorded on staging only: add ' + newHosts.map(function (g) { return g.service + 'Url'; }).join(', ') + ' for dev and prod (vc check warns until you do).' });
  }
  if (opts.secretName && opts.hasKeySecret) files.push({ path: '.env.schema', status: 'changed', before: ENV_SCHEMA_BEFORE, after: ENV_SCHEMA_BEFORE + '\n\n# Payments API key, found in checkout-session.har (query api_key)\n# @sensitive @required\n' + opts.secretName + '=' });
  if (opts.flowTest) files.push({ path: 'services/orders/checkout.test.yaml', status: 'changed', before: CHECKOUT_BEFORE, after: CHECKOUT_BEFORE + '\n' + testBlockYaml(opts.flowTest) });
  return { files: files, catalog: after };
}
function stepsToStages(steps, prefix) {
  return steps.map(function (s, i) {
    var st = { uid: prefix + i, api: s.call, vars: clone(s.vars || {}) };
    if (s.forEach) st.loop = { kind: 'forEach', items: s.forEach.join(', '), count: s.forEach.length };
    if (s.repeat) st.loop = { kind: 'repeat', count: s.repeat };
    if (s.poll) { var f = Object.keys(s.poll.until.body.match)[0]; st.poll = { field: f, equals: s.poll.until.body.match[f], every: s.poll.every, timeout: s.poll.timeout }; }
    return st;
  });
}

/* ---------- import: OpenAPI and Postman (canned specs) ---------- */
function openapiOps() {
  return [
    { method: 'GET', path: '/payment-methods', operationId: 'listPaymentMethods' },
    { method: 'POST', path: '/payments/authorize', operationId: 'authorizePayment' },
    { method: 'GET', path: '/payments/{paymentId}', operationId: 'getPayment' },
    { method: 'POST', path: '/payments/{paymentId}/capture', operationId: 'capturePayment' },
    { method: 'POST', path: '/payments/{paymentId}/refunds', operationId: null }
  ];
}
function openapiMap(ops, catalog, service) {
  var file = catalog.filter(function (f) { return f.id === service; })[0] || { apis: [] };
  return ops.map(function (op) {
    var tplPath = op.path.replace(/\{(\w+)\}/g, function (a, n) { return '${var.' + n + '}'; });
    var match = file.apis.filter(function (a) { return (a.method || 'GET') === op.method && shapeOf(a.path) === shapeOf(tplPath); })[0];
    var fromOp = op.operationId ? op.operationId.replace(new RegExp(cap(singular(service)) + '$'), '') : null;
    var id = match ? match.id : (fromOp && validId(fromOp) ? fromOp : deriveApiId(op.method, tplPath, service));
    return { method: op.method, path: tplPath, operationId: op.operationId, id: id, match: match ? service + '.' + match.id : null, idFrom: match ? 'existing' : op.operationId ? 'operationId' : 'derived from path' };
  });
}

/* ---------- more helpers ---------- */
function fmtShort(v) { var s = fmt(v); return s.length > 22 ? s.slice(0, 21) + '…' : s; }
function flowVarTable(flow, env, run) {
  var info = stageInfo(flow, env.catalog);
  var names = flowVarNames(flow, env.catalog);
  var shown = (run && run.shown) || {};
  return names.map(function (n) {
    var cells = flow.stages.map(function (st, i) {
      var inf = info[i];
      var snap = run && shown[st.uid] ? run.snapshots[st.uid] : null;
      var sets = inf.saves.indexOf(n) >= 0;
      var cons = inf.consumes.filter(function (c) { return c.name === n; })[0];
      var val = '';
      if (snap && snap[n] !== undefined) val = fmtShort(snap[n]);
      else if (cons && !cons.chained) {
        var items = loopItems(st);
        if ((st.vars || {})[n] === '${item}' && items) val = items.join(', ');
        else { var req = stageReq(st, env); var b = buildRequest(req, env, {}); val = fmtShort(resolveValue('${var.' + n + '}', { vars: b.vars, missing: [] }, 0)); }
      }
      return { val: val, sets: sets && !!snap, uses: !!cons };
    });
    return { name: n, cells: cells };
  });
}
function thirdPerson(name) {
  var w = String(name || '').trim().split(/\s+/);
  if (!w[0]) return '';
  var v = w[0];
  if (/[^aeiou]y$/i.test(v)) v = v.slice(0, -1) + 'ies';
  else if (/(s|sh|ch|x|z)$/i.test(v)) v = v + 'es';
  else v = v + 's';
  w[0] = v;
  return w.join(' ');
}
function reqToApi(req, found) {
  var api = clone(found.api);
  var base = found.file.baseUrl;
  if (String(req.urlTpl).indexOf(base) !== 0) return { error: 'The URL no longer starts with the file baseUrl ' + base + ', so it cannot be written back to ' + found.file.id + '.api.yaml.' };
  api.method = req.method;
  api.path = String(req.urlTpl).slice(base.length) || '/';
  var q = {}; (req.query || []).forEach(function (r) { if (r.on && r.k) q[r.k] = r.v; });
  if (Object.keys(q).length) api.query = q; else delete api.query;
  var h = {}; (req.headers || []).forEach(function (r) { if (r.on && r.k) h[r.k] = r.v; });
  if (Object.keys(h).length) api.headers = h; else delete api.headers;
  if (req.bodyMode === 'json') {
    var err = jsonError(req.bodyText);
    if (err) return { error: err + ': fix the body before saving.' };
    if (String(req.bodyText).trim()) api.body = JSON.parse(req.bodyText); else delete api.body;
  } else if (req.bodyMode === 'none') delete api.body;
  else if (req.bodyMode === 'text') api.body = req.bodyText;
  else if (req.bodyMode === 'form' || req.bodyMode === 'multipart') { var b = {}; (req.form || []).forEach(function (r) { if (r.on && r.k) b[r.k] = r.v; }); api.body = b; }
  var sv = {}; (req.saves || []).forEach(function (s) { if (s.name) sv[s.name] = s.expr; });
  if (Object.keys(sv).length) api.save = sv; else delete api.save;
  return { api: api };
}
function reqDirty(req, catalog) {
  if (!req.apiId) return !!String(req.urlTpl || '').trim();
  var found = findApi(catalog, req.apiId);
  if (!found) return true;
  var r = reqToApi(req, found);
  if (r.error) return true;
  return JSON.stringify(apiToObj(r.api)) !== JSON.stringify(apiToObj(found.api));
}
function postmanItems() {
  var v = function (n) { return LB + n + RB; };
  return [
    { folder: 'Users', name: 'Create user', method: 'POST', url: v('usersUrl') + '/api/v1/users' },
    { folder: 'Users', name: 'Get user', method: 'GET', url: v('usersUrl') + '/api/v1/users/' + v('userId') },
    { folder: 'Orders', name: 'Order history', method: 'GET', url: v('ordersUrl') + '/api/v1/orders?userId=' + v('userId') },
    { folder: 'Orders', name: 'Cancel order', method: 'POST', url: v('ordersUrl') + '/api/v1/orders/' + v('orderId') + '/cancel' },
    { folder: 'Catalog', name: 'Product by SKU', method: 'GET', url: v('catalogUrl') + '/api/v1/products/' + v('sku') },
    { folder: 'Catalog', name: 'Stock levels', method: 'GET', url: v('catalogUrl') + '/api/v1/products/' + v('sku') + '/stock-levels' }
  ];
}
function postmanMap(items, catalog) {
  var re = new RegExp('\\{\\{\\s*(\\w+)\\s*\\}\\}', 'g');
  return items.map(function (it) {
    var tpl = it.url.replace(re, function (a, n) { return '${var.' + n + '}'; });
    var m = /^\$\{var\.(\w+)Url\}(\/api\/v\d+)?([^?]*)(\?.*)?$/.exec(tpl);
    var service = m ? m[1] : 'misc';
    var path = m ? m[3] : tpl;
    var file = catalog.filter(function (f) { return f.id === service; })[0];
    var match = file ? file.apis.filter(function (a) { return (a.method || 'GET') === it.method && shapeOf(a.path) === shapeOf(path); })[0] : null;
    var query = {};
    if (m && m[4]) m[4].slice(1).split('&').forEach(function (kv) { var p = kv.split('='); query[p[0]] = p[1] || ''; });
    var id = match ? match.id : deriveApiId(it.method, path, service);
    return { folder: it.folder, name: it.name, method: it.method, url: it.url, tpl: tpl, service: service, path: path, query: query, id: id, match: match ? service + '.' + match.id : null };
  });
}

function curlFor(built) {
  var q = function (x) { return "'" + String(x).replace(/'/g, "'\\''") + "'"; };
  var lines = ['curl -X ' + built.method + ' ' + q(built.url)];
  built.headers.forEach(function (h) { var v = h.k === 'authorization' && /^Bearer/.test(h.v) ? 'Bearer $TOKEN' : h.v; lines.push('  -H ' + q(h.k + ': ' + v)); });
  if (built.bodyText) lines.push('  --data-raw ' + q(built.body ? JSON.stringify(built.body) : built.bodyText));
  return lines.join(' \\\n');
}

return {
  curlFor: curlFor, CHECKOUT_BEFORE: CHECKOUT_BEFORE, ENV_SCHEMA_BEFORE: ENV_SCHEMA_BEFORE,
  flowVarTable: flowVarTable, thirdPerson: thirdPerson, reqToApi: reqToApi, reqDirty: reqDirty, postmanItems: postmanItems, postmanMap: postmanMap, fmtShort: fmtShort, fmt: fmt,
  LB: LB, RB: RB, PROFILES: PROFILES, LEVEL_ORDER: LEVEL_ORDER, LEVEL_LABEL: LEVEL_LABEL, TEST_FILES: TEST_FILES,
  clone: clone, cap: cap, toCamel: toCamel, validId: validId, singular: singular, humanize: humanize,
  initialConfig: initialConfig, initialCatalog: initialCatalog, initialFlows: initialFlows, findApi: findApi, allApis: allApis,
  resolveValue: resolveValue, resolveDeep: resolveDeep, varRefs: varRefs, lookupPath: lookupPath,
  reqFromApi: reqFromApi, blankReq: blankReq, savedValues: savedValues, levelsFor: levelsFor, mergeLevels: mergeLevels, usedVars: usedVars, varRows: varRows,
  inheritedHeaders: inheritedHeaders, jsonError: jsonError, buildRequest: buildRequest, shapeOf: shapeOf, splitUrl: splitUrl, matchApiByUrl: matchApiByUrl,
  execRequest: execRequest, apiChecks: apiChecks, fmtSize: fmtSize, timingFor: timingFor,
  stageReq: stageReq, runStage: runStage, runFlow: runFlow, stageInfo: stageInfo, flowVarNames: flowVarNames, loopItems: loopItems,
  toYaml: toYaml, apiToObj: apiToObj, apiFileYaml: apiFileYaml, tokenizeJson: tokenizeJson, tokenizeYaml: tokenizeYaml, codeLines: codeLines,
  lineDiff: lineDiff, diffHunks: diffHunks, diffView: diffView,
  deriveTestId: deriveTestId, testIdWarning: testIdWarning, testFilePath: testFilePath, suggestExpectations: suggestExpectations, testFromRequest: testFromRequest,
  testBlockYaml: testBlockYaml, stageToStep: stageToStep, flowToTest: flowToTest, flowToLifecycle: flowToLifecycle,
  shellSplit: shellSplit, parseCurl: parseCurl, isIdLike: isIdLike, splitBase: splitBase, templatePath: templatePath, deriveApiId: deriveApiId, nameFromId: nameFromId,
  harEntries: harEntries, classifyEntry: classifyEntry, harCounts: harCounts, harVisible: harVisible, harSelected: harSelected, harMap: harMap,
  harSecrets: harSecrets, harFlowSteps: harFlowSteps, chainSummary: chainSummary, harApply: harApply, harFiles: harFiles, stepsToStages: stepsToStages,
  configYaml: configYaml, openapiOps: openapiOps, openapiMap: openapiMap
};
