// ===================== api, config, load, dataset forms =====================
function buildApiForm(B) {
  var m = B.m, apis = Array.isArray(m.apis) ? m.apis : [], nav = [], secs = [];
  if (!B.sel || (/^api:/.test(B.sel) && !apis[+B.sel.split(':')[1]])) B.sel = apis.length ? 'api:0' : 'file';
  nav.push(navItem(B, 'file', 'File settings', (m.id || 'no id') + ' · baseUrl, auth, vars', [['id'], ['name'], ['baseUrl'], ['auth'], ['headers'], ['vars'], ['source'], ['profiles']]));
  apis.forEach(function (a, i) {
    if (!isObj(a)) a = {};
    nav.push(navItem(B, 'api:' + i, a.id || 'no id', (a.path || '') + (a.deprecated ? ' · deprecated' : ''), [['apis', i]], { method: String(a.method || 'GET').toUpperCase(), monoSub: true, btns: moveBtns(B, ['apis'], i, apis.length, 'api') }));
  });
  if (B.sel === 'file') {
    secs.push(sec('File settings', { sub: 'Every api in ' + B.fpath + ' inherits these' }, [
      rText(B, 'File id', ['id'], { help: 'The service name; apis are called as ' + (m.id || 'service') + '.<apiId>' }),
      rText(B, 'Name', ['name'], { mono: false, span: 'w2' }),
      rText(B, 'Base URL', ['baseUrl'], { comp: 'tpl', span: 'w2', help: 'Usually a var, so profiles switch it' }),
      rSelect(B, 'Auth', ['auth'], [{ v: '', l: 'none set' }, { v: 'none', l: 'none: send no credentials' }].concat(B.ctx.authDefs.map(function (a) { return { v: a.id, l: a.id + ' · ' + a.type }; }))),
      rText(B, 'OpenAPI source', ['source', 'openapi'], { span: 'w2', help: 'vc sync compares the apis with this spec and writes a snapshot under openapi/' })
    ]));
    secs.push(sec('Headers and vars', {}, [rKv(B, 'Headers', ['headers'], { addLbl: 'Add header', newKey: 'x-header', raw: true }), rKv(B, 'File vars', ['vars'], { addLbl: 'Add var' })]));
    secs.push(sec('Profile overrides', { sub: 'Per-profile values for this file\'s vars' }, []));
    profileOverrideSecs(B, ['profiles']).forEach(function (s) { secs.push(s); });
  } else {
    var i = +B.sel.split(':')[1], ap = ['apis', i], a = isObj(apis[i]) ? apis[i] : {}, fid = m.id || '';
    var pathVars = varRoots([a.path]), provided = Object.keys(isObj(a.vars) ? a.vars : {}).concat(Object.keys(isObj(m.vars) ? m.vars : {})).concat(Object.keys(B.ctx.configVars));
    secs.push(sec(a.name || a.id || 'Api', { sub: fid + '.' + (a.id || '?'), fid: fidOf(ap) }, [
      rText(B, 'Id', ap.concat('id'), { help: 'camelCase, verb first, no service prefix: ' + fid + '.' + (a.id || 'create') }),
      rText(B, 'Name', ap.concat('name'), { mono: false, span: 'w2' }),
      rSelect(B, 'Method', ap.concat('method'), METHODS, { value: String(a.method || 'GET').toUpperCase(), set: function (v) { B.c.editModel(function (mm) { setIn(mm, ap.concat('method'), v === 'GET' ? undefined : v); }); }, help: a.method ? '' : 'GET when not set' }),
      rText(B, 'Path', ap.concat('path'), { comp: 'tpl', span: 'w2', ph: '/users/${var.userId}' }),
      rChips('Path vars', pathVars.length ? pathVars.map(function (nm) { var ok = provided.indexOf(nm) >= 0; return { t: '${var.' + nm + '}', cls: 'pill ' + (ok ? 'pass' : 'accent'), title: ok ? 'has a default here' : 'callers set it (a saved value or vars)' }; }) : [{ t: 'none', cls: 'pill', title: 'The path has no templates' }]),
      rText(B, 'operationId', ap.concat('operationId'), { ph: 'from the OpenAPI spec' }),
      rText(B, 'Timeout', ap.concat('timeout'), { ph: 'defaults.timeout' }),
      rCheck(B, 'Deprecated', a.deprecated === true, function (on) { B.c.editModel(function (mm) { setIn(mm, ap.concat('deprecated'), on ? true : undefined); }); }, { fid: fidOf(ap.concat('deprecated')), help: 'vc check warns where it is still called' })
    ]));
    secs.push(sec('Request', { fid: fidOf(ap.concat('vars')) }, [
      rKv(B, 'Vars (defaults)', ap.concat('vars'), { addLbl: 'Add var', sub: 'callers override them' }),
      rKv(B, 'Query', ap.concat('query'), { addLbl: 'Add parameter', newKey: 'param' }),
      rKv(B, 'Headers', ap.concat('headers'), { addLbl: 'Add header', newKey: 'x-header', raw: true })
    ].concat(bodyRows(B, ap.concat('body'), null)).concat(authRows(B, ap.concat('auth'), { inheritLbl: 'inherit from the file (' + (m.auth || 'none') + ')' }))));
    secs.push(sec('Expect', { sub: 'Checks that always hold; tests add to them', fid: fidOf(ap.concat('expect')) }, expectRows(B, ap.concat('expect'), { statusPh: 'any 2xx' })));
    var cu = isObj(a.cleanup) ? a.cleanup : null;
    var cl = [rKv(B, 'Save', ap.concat('save'), { addLbl: 'Add value', newKey: 'value', newVal: '${response.body.id}', vph: '${response.body.id}', empty: 'Nothing saved.' })];
    if (cu) {
      cl.push(rText(B, 'Cleanup calls', ap.concat('cleanup', 'call'), { comp: 'localApi', help: 'An api in this file; runs after the test, newest first', keepEmpty: true }));
      cl.push(rKv(B, 'Cleanup vars', ap.concat('cleanup', 'vars'), { addLbl: 'Add var', newVal: '${response.body.id}' }));
      cl.push(rAdds([{ l: 'Remove cleanup', on: function () { B.c.editModel(function (mm) { setIn(mm, ap.concat('cleanup'), undefined); }); } }]));
    } else cl.push(rAdds([{ l: '+ Cleanup', on: function () { B.c.editModel(function (mm) { setIn(mm, ap.concat('cleanup'), { call: apis.filter(function (x) { return isObj(x) && String(x.method).toUpperCase() === 'DELETE'; }).map(function (x) { return x.id; })[0] || '', vars: { id: '${response.body.id}' } }); }); } }]));
    secs.push(sec('Save and cleanup', { fid: fidOf(ap.concat('save')) }, cl));
  }
  return { nav: nav, secs: secs, add: { l: 'Add api', on: function () { B.c.openNew('api'); } } };
}
function buildConfigForm(B) {
  var m = B.m, nav = [], secs = [];
  var keys = { general: [['syntaxVersion'], ['project'], ['paths']], profiles: [['vars'], ['profiles']], auth: [['auth']], defaults: [['defaults']], output: [['secrets'], ['reports'], ['history']], hooks: [['beforeRun'], ['afterRun']] };
  if (!B.sel || !keys[B.sel]) B.sel = 'profiles';
  nav.push(navItem(B, 'general', 'Project', 'syntaxVersion, project, paths', keys.general));
  nav.push(navItem(B, 'profiles', 'Vars and profiles', (Array.isArray(m.profiles) ? m.profiles.length : 0) + ' profiles', keys.profiles));
  nav.push(navItem(B, 'auth', 'Auth', (Array.isArray(m.auth) ? m.auth.length : 0) + ' definitions', keys.auth));
  nav.push(navItem(B, 'defaults', 'Defaults', 'timeout, concurrency, retry', keys.defaults));
  nav.push(navItem(B, 'output', 'Secrets and reports', 'provider, formats', keys.output));
  nav.push(navItem(B, 'hooks', 'beforeRun and afterRun', 'run-level actions', keys.hooks));
  if (B.sel === 'general') {
    secs.push(sec('Project', {}, [
      rSelect(B, 'syntaxVersion', ['syntaxVersion'], [{ v: '2', l: '2 (current)' }], { parse: function (v) { return +v; } }),
      rText(B, 'Project', ['project'], { mono: false, help: 'Shown in reports and on the server' }),
      rText(B, 'Include paths', ['paths', 'include'], { span: 'w2', fmt: function (v) { return Array.isArray(v) ? v.join(', ') : String(v); }, parse: tagsParse, ph: 'everything except node_modules and .vibranium' }),
      rText(B, 'Exclude paths', ['paths', 'exclude'], { span: 'w2', fmt: function (v) { return Array.isArray(v) ? v.join(', ') : String(v); }, parse: tagsParse, ph: 'none' })
    ]));
  }
  if (B.sel === 'profiles') {
    var prs = Array.isArray(m.profiles) ? m.profiles : [], cvars = isObj(m.vars) ? m.vars : {}, names = Object.keys(cvars);
    prs.forEach(function (p) { if (isObj(p) && isObj(p.vars)) Object.keys(p.vars).forEach(function (k) { if (names.indexOf(k) < 0) names.push(k); }); });
    var cols = ['var', 'base (vars)'].concat(prs.map(function (p) { return isObj(p) ? p.id : '?'; }));
    var data = names.map(function (nm) { return [nm, cvars[nm]].concat(prs.map(function (p) { return isObj(p) && isObj(p.vars) ? p.vars[nm] : undefined; })); });
    secs.push(sec('Vars by profile', { sub: 'Rows are vars, columns are the base value and each profile; an empty cell inherits the base. Exactly one profile is the default.' }, [
      gridRow(B, 'Profiles matrix', ['profiles'], cols, data, {
        firstHead: null, noDraft: false,
        phOf: function (ri, ci) { return ci >= 2 ? (cvars[names[ri]] !== undefined ? cellText(cvars[names[ri]]) : 'not set') : ci === 1 ? 'not set' : 'name'; },
        rowName: function (ri) { return names[ri]; },
        setCell: function (ri, ci, raw, fid) {
          var nm = names[ri];
          if (ci === 0) {
            var to = String(raw).trim();
            if (!/^[A-Za-z_][\w]*$/.test(to) || (to !== nm && names.indexOf(to) >= 0)) { B.c.setDraft(fid, raw); return; }
            B.c.setDraft(fid, undefined);
            B.c.editModel(function (mm) { if (isObj(mm.vars) && mm.vars[nm] !== undefined) mm.vars = renameKey(mm.vars, nm, to); (mm.profiles || []).forEach(function (p) { if (isObj(p) && isObj(p.vars) && p.vars[nm] !== undefined) p.vars = renameKey(p.vars, nm, to); }); });
            return;
          }
          B.c.editModel(function (mm) {
            if (ci === 1) { if (raw === '') { if (isObj(mm.vars)) setIn(mm, ['vars', nm], undefined); } else setIn(mm, ['vars', nm], parseCell(raw)); }
            else { var pi = ci - 2; if (raw === '') setIn(mm, ['profiles', pi, 'vars', nm], undefined); else setIn(mm, ['profiles', pi, 'vars', nm], parseCell(raw)); }
          });
        },
        colEditable: function (ci) { return ci >= 2; },
        radio: { label: 'default', on: function (ci) { return ci >= 2; }, checked: function (ci) { return ci >= 2 && isObj(prs[ci - 2]) && prs[ci - 2]['default'] === true; }, set: function (ci) { if (ci < 2) return; B.c.editModel(function (mm) { mm.profiles.forEach(function (p, pi) { if (!isObj(p)) return; if (pi === ci - 2) mm.profiles[pi] = p['default'] === true ? p : insertKeyAfter(p, 'id', 'default', true); else delete p['default']; }); }); } },
        renameCol: function (ci, raw, fid) {
          if (ci < 2) return;
          var to = String(raw).trim();
          if (!/^[a-z][a-z0-9]*$/.test(to) || (to !== cols[ci] && cols.indexOf(to) >= 0)) { B.c.setDraft(fid, raw); return; }
          B.c.setDraft(fid, undefined); B.c.editModel(function (mm) { mm.profiles[ci - 2].id = to; });
        },
        colsEditable: true,
        delCol: function (ci) { if (ci < 2) return; B.c.editModel(function (mm) { mm.profiles.splice(ci - 2, 1); }); },
        addCol: function () { B.c.editModel(function (mm) { var ids = (mm.profiles || []).map(function (p) { return p.id; }); var nm = ['eu', 'qa', 'perf', 'local'].filter(function (x) { return ids.indexOf(x) < 0; })[0] || 'profile' + ids.length; if (!Array.isArray(mm.profiles)) mm.profiles = []; mm.profiles.push({ id: nm }); }); },
        addColLbl: 'Add profile',
        addRow: function () { B.c.editModel(function (mm) { if (!isObj(mm.vars)) mm.vars = {}; mm.vars[uniqueKey(mm.vars, 'newVar')] = ''; }); },
        addRowLbl: 'Add var',
        delRow: function (ri) { var nm = names[ri]; B.c.editModel(function (mm) { if (isObj(mm.vars)) setIn(mm, ['vars', nm], undefined); (mm.profiles || []).forEach(function (p, pi) { if (isObj(p) && isObj(p.vars) && p.vars[nm] !== undefined) setIn(mm, ['profiles', pi, 'vars', nm], undefined); }); }); },
        note: 'Profile ids are lowercase single words. Rename a profile in its column header.'
      })
    ]));
  }
  if (B.sel === 'auth') {
    var auths = Array.isArray(m.auth) ? m.auth : [];
    secs.push(sec('Auth definitions', { sub: 'Written once; use them by id on an api file, api, test, step or action' }, [rAdds([
      { l: '+ OAuth2 client credentials', on: function () { B.c.editModel(function (mm) { if (!Array.isArray(mm.auth)) mm.auth = []; mm.auth.push({ id: uniqueId('serviceOauth', mm.auth.map(function (a) { return a.id; })).replace(/2$/, ''), type: 'oauth2ClientCredentials', tokenUrl: '${var.identityUrl}/oauth/token', clientId: '${secret.CLIENT_ID}', clientSecret: '${secret.CLIENT_SECRET}', scopes: [] }); }); } },
      { l: '+ Login flow', on: function () { B.c.editModel(function (mm) { if (!Array.isArray(mm.auth)) mm.auth = []; mm.auth.push({ id: 'portalLogin', type: 'flow', steps: [{ id: 'login', method: 'POST', url: '${var.identityUrl}/login', save: { accessToken: '${response.body.token}' } }], apply: { headers: { authorization: 'Bearer ${var.accessToken}' } } }); }); } },
      { l: '+ API key', on: function () { B.c.editModel(function (mm) { if (!Array.isArray(mm.auth)) mm.auth = []; mm.auth.push({ id: 'partnerKey', type: 'apiKey', 'in': 'header', name: 'x-api-key', value: '${secret.PARTNER_API_KEY}' }); }); } }
    ])]));
    auths.forEach(function (a, i) {
      var p = ['auth', i], rows = [];
      if (!isObj(a)) return;
      rows.push(rText(B, 'Id', p.concat('id'), { help: 'camelCase noun, like shopOauth' }));
      rows.push(rSelect(B, 'Type', p.concat('type'), AUTH_TYPES));
      if (a.type !== 'apiKey') rows.push(rText(B, 'Name', p.concat('name'), { mono: false, span: 'w2' }));
      var F = { basic: ['username', 'password'], bearer: ['token'], apiKey: ['in', 'name', 'value'], oauth2ClientCredentials: ['tokenUrl', 'clientId', 'clientSecret', 'audience'], oauth2AuthorizationCode: ['authorizeUrl', 'tokenUrl', 'clientId', 'redirectPort'], oauth2Refresh: ['tokenUrl', 'clientId', 'refreshToken'], plugin: ['plugin'] }[a.type] || [];
      F.forEach(function (k) { if (k === 'in') rows.push(rSelect(B, 'In', p.concat('in'), ['header', 'query'])); else rows.push(rText(B, k, p.concat(k), { comp: 'tpl', span: /Url$|Secret|token|value/.test(k) ? 'w2' : '' })); });
      if (/^oauth2/.test(a.type)) rows.push(rText(B, 'Scopes', p.concat('scopes'), { span: 'w2', fmt: function (v) { return Array.isArray(v) ? v.join(', ') : String(v); }, parse: function (r) { var x = tagsParse(r); return x || []; }, keepEmpty: true }));
      if (a.type === 'flow') {
        rows.push(rCheck(B, 'Cache credentials across runs', a.cache === true, function (on) { B.c.editModel(function (mm) { setIn(mm, p.concat('cache'), on ? true : undefined); }); }, { fid: fidOf(p.concat('cache')) }));
        rows.push(rText(B, 'Expires in', p.concat('expiresIn'), { comp: 'tpl', help: 'A duration; a template is fine' }));
        rows.push(rKv(B, 'Default vars', p.concat('vars'), { addLbl: 'Add var', newKey: 'username' }));
        rows.push(rKv(B, 'Apply headers', p.concat('apply', 'headers'), { raw: true, addLbl: 'Add header', newKey: 'authorization', newVal: 'Bearer ${var.accessToken}' }));
      }
      secs.push(sec(a.id || 'auth', { lvl: 1, sub: a.type, fid: fidOf(p), btns: moveBtns(B, ['auth'], i, auths.length, 'auth ' + (a.id || '')) }, rows));
      if (a.type === 'flow') {
        var steps = Array.isArray(a.steps) ? a.steps : [];
        steps.forEach(function (s, si) {
          var sp2 = p.concat('steps', si);
          if (!isObj(s)) return;
          secs.push(sec((a.id || 'flow') + ' · step ' + (si + 1), { lvl: 1, sub: s.call ? 'calls ' + s.call : (s.method || 'GET') + ' ' + (s.url || ''), fid: fidOf(sp2), btns: moveBtns(B, p.concat('steps'), si, steps.length, 'flow step') }, [
            rText(B, 'Id', sp2.concat('id')),
            rSelect(B, 'Method', sp2.concat('method'), METHODS),
            rText(B, 'URL', sp2.concat('url'), { comp: 'tpl', span: 'w2' }),
            rKv(B, 'Headers', sp2.concat('headers'), { raw: true, addLbl: 'Add header', newKey: 'x-header' }),
            rKv(B, 'Body fields', sp2.concat('body'), { addLbl: 'Add field', newKey: 'field' }),
            rText(B, 'Expect status', sp2.concat('expect', 'status'), { fmt: fmtStatus, parse: parseStatusInput }),
            rKv(B, 'Save', sp2.concat('save'), { addLbl: 'Add value', newKey: 'value', newVal: '${response.body.id}' })
          ]));
        });
        secs.push(sec('', {}, [rAdds([{ l: '+ Flow step', on: function () { B.c.editModel(function (mm) { var st = getIn(mm, p.concat('steps')); if (!Array.isArray(st)) { st = []; setIn(mm, p.concat('steps'), st); } st.push({ id: 'step' + (st.length + 1), method: 'POST', url: '${var.identityUrl}/' }); }); } }])]));
      }
    });
  }
  if (B.sel === 'defaults') {
    secs.push(sec('Defaults', { sub: 'Applied to every request and test unless overridden' }, [
      rText(B, 'Timeout', ['defaults', 'timeout'], { ph: '30s', help: 'Per request' }),
      rText(B, 'Concurrency', ['defaults', 'concurrency'], { type: 'number', ph: '16', parse: function (r) { return r === '' ? undefined : +r; }, help: 'Test instances in flight' }),
      rText(B, 'Per host', ['defaults', 'perHost'], { type: 'number', ph: '8', parse: function (r) { return r === '' ? undefined : +r; }, help: 'Connections per host' }),
      rText(B, 'Retry attempts', ['defaults', 'retry', 'attempts'], { type: 'number', ph: 'none', parse: function (r) { return r === '' ? undefined : +r; } }),
      rText(B, 'Retry delay', ['defaults', 'retry', 'delay'], { ph: '1s' }),
      rKv(B, 'Headers on every request', ['defaults', 'headers'], { raw: true, addLbl: 'Add header', newKey: 'x-header' })
    ]));
  }
  if (B.sel === 'output') {
    var fm = isObj(m.reports) && Array.isArray(m.reports.formats) ? m.reports.formats : [];
    secs.push(sec('Secrets', { sub: '${secret.*} values are redacted everywhere' }, [
      rSelect(B, 'Provider', ['secrets', 'provider'], [{ v: 'varlock', l: 'varlock (1Password, Vault, cloud secret stores)' }, { v: 'env', l: 'env (plain environment variables)' }]),
      rText(B, 'Schema file', ['secrets', 'schema'], { ph: '.env.schema' })
    ]));
    secs.push(sec('Reports', { sub: 'Written after every run; CTRF is always produced' }, [rText(B, 'Folder', ['reports', 'dir'], { ph: '.vibranium/reports', span: 'w2' })].concat(REPORT_FORMATS.map(function (f) {
      return rCheck(B, f, fm.indexOf(f) >= 0, function (on) { B.c.editModel(function (mm) { var cur = getIn(mm, ['reports', 'formats']); cur = Array.isArray(cur) ? cur.slice() : []; if (on && cur.indexOf(f) < 0) cur.push(f); if (!on) cur = cur.filter(function (x) { return x !== f; }); setIn(mm, ['reports', 'formats'], cur.length ? REPORT_FORMATS.filter(function (x) { return cur.indexOf(x) >= 0; }) : undefined); }); }, { fid: fidOf(['reports', 'formats', f]) });
    }))));
  }
  if (B.sel === 'hooks') lifecycleSecs(B, ['beforeRun', 'afterRun']).forEach(function (s) { secs.push(s); });
  return { nav: nav, secs: secs, add: null };
}
function buildLoadForm(B) {
  var m = B.m, nav = [], secs = [];
  var keys = { scenario: [['id'], ['name'], ['checks'], ['thinkTime'], ['abortOn']], mix: [['mix']], stages: [['stages']], thresholds: [['thresholds']] };
  if (!B.sel || !keys[B.sel]) B.sel = 'mix';
  var mix = Array.isArray(m.mix) ? m.mix : [], stages = Array.isArray(m.stages) ? m.stages : [];
  nav.push(navItem(B, 'scenario', 'Scenario', (m.id || 'no id') + ' · checks ' + (m.checks || 'api'), keys.scenario));
  nav.push(navItem(B, 'mix', 'Traffic mix', mix.length + ' tests', keys.mix));
  nav.push(navItem(B, 'stages', 'Stages', stages.length + ' stages', keys.stages));
  nav.push(navItem(B, 'thresholds', 'Thresholds', Object.keys(isObj(m.thresholds) ? m.thresholds : {}).join(', ') || 'none', keys.thresholds));
  if (B.sel === 'scenario') {
    var tt = m.thinkTime;
    var rows = [
      rText(B, 'Id', ['id'], { help: '<area>.<scenario>, run with vc load ' + (m.id || 'checkout.peak') }),
      rText(B, 'Name', ['name'], { mono: false, span: 'w2' }),
      rSelect(B, 'Checks', ['checks'], [{ v: 'api', l: 'api: only the apis\' own expect (fast)' }, { v: 'full', l: 'full: every test\'s expect' }], { span: 'w2' })
    ];
    if (isObj(tt)) { rows.push(rText(B, 'Think time min', ['thinkTime', 'min'], { ph: '500ms' })); rows.push(rText(B, 'Think time max', ['thinkTime', 'max'], { ph: '2s' })); rows.push(rAdds([{ l: 'Use a fixed think time', on: function () { B.c.editModel(function (mm) { mm.thinkTime = tt.min || '1s'; }); } }])); }
    else { rows.push(rText(B, 'Think time', ['thinkTime'], { ph: '0s' })); rows.push(rAdds([{ l: 'Use a random range', on: function () { B.c.editModel(function (mm) { mm.thinkTime = { min: '500ms', max: '2s' }; }); } }])); }
    rows.push(rText(B, 'Abort on', ['abortOn'], { span: 'w2', fmt: function (v) { return Array.isArray(v) ? v.join(', ') : String(v); }, parse: tagsParse, help: 'Stop early when one of these thresholds is breached' }));
    secs.push(sec('Scenario', {}, rows));
  }
  if (B.sel === 'mix') {
    var total = mix.reduce(function (s, x) { return s + (isObj(x) && typeof x.weight === 'number' ? x.weight : 0); }, 0) || 1;
    var items = mix.map(function (x, i) {
      var p = ['mix', i], w = isObj(x) && typeof x.weight === 'number' ? x.weight : 0, dT = diagAt(B, p.concat('test'));
      return { cells: [
        inCell(B, fidOf(p.concat('test')), isObj(x) ? x.test : '', function (raw) { B.c.editModel(function (mm) { setIn(mm, p.concat('test'), raw); }); }, { comp: 'test', aria: 'Test', diag: dT, ph: 'file.testId' }),
        rangeCell(fidOf(p.concat('weight')), w, 1, 10, function (v) { B.c.editModel(function (mm) { setIn(mm, p.concat('weight'), v); }); }, 'Weight of ' + (x && x.test), String(w)),
        stCell(Math.round(w / total * 100) + '%', 'num'),
        inCell(B, fidOf(p.concat('thresholds')), isObj(x) && x.thresholds ? flowOf(x.thresholds) : '', function (raw) { var v = parseCell(raw); B.c.editModel(function (mm) { setIn(mm, p.concat('thresholds'), isObj(v) ? v : undefined); }); }, { ph: '{p95: {lt: 1200ms}}', aria: 'Thresholds for this test' })
      ], del: function () { B.c.delItem(['mix'], i); }, delAria: 'Remove from mix' };
    });
    var errs = [];
    mix.forEach(function (x, i) { var d = subDiag(B, ['mix', i]); if (d) errs.push({ txt: 'Row ' + (i + 1) + ': ' + d.msg, cls: hcls(d) }); });
    secs.push(sec('Traffic mix', { sub: 'Existing tests become traffic; the share is weight over the total' }, [rTable('Mix', { gcols: 'minmax(180px,2fr) minmax(110px,1fr) 52px minmax(120px,1.2fr) 32px', heads: [{ t: 'Test' }, { t: 'Weight' }, { t: 'Share' }, { t: 'Own thresholds' }, { t: '' }], items: items, errs: errs, addLbl: 'Add test',
      add: function () { var used = mix.map(function (x) { return x && x.test; }), next = B.ctx.tests.filter(function (t) { return used.indexOf(t.full) < 0; })[0]; B.c.editModel(function (mm) { if (!Array.isArray(mm.mix)) mm.mix = []; mm.mix.push({ test: next ? next.full : '', weight: 1 }); }); } })]));
  }
  if (B.sel === 'stages') {
    var W = 640, H = 132, rp = rampPoints(stages, W, H);
    var sitems = stages.map(function (s, i) {
      var p = ['stages', i];
      return { cells: [
        inCell(B, fidOf(p.concat('duration')), isObj(s) ? s.duration : '', function (raw) { B.c.editModel(function (mm) { setIn(mm, p.concat('duration'), raw); }); }, { aria: 'Duration of stage ' + (i + 1), ph: '1m', diag: diagAt(B, p.concat('duration')) }),
        inCell(B, fidOf(p.concat('users')), isObj(s) ? s.users : '', function (raw) { B.c.editModel(function (mm) { setIn(mm, p.concat('users'), raw === '' ? 0 : parseCell(raw)); }); }, { aria: 'Users at the end of stage ' + (i + 1), ph: '100', diag: diagAt(B, p.concat('users')) })
      ], del: function () { B.c.delItem(['stages'], i); }, delAria: 'Remove stage ' + (i + 1) };
    });
    secs.push(sec('Stages', { sub: 'Users ramp from the previous stage\'s count to this one over the duration' }, [
      { isRamp: true, cls: 'fld full', pts: rp.pts, area: rp.area, vb: '0 0 ' + W + ' ' + H, labels: rp.peaks.map(function (p) { return { x: Math.min(W - 40, Math.max(4, p.x - 18)), y: Math.max(12, p.y - 6), t: p.u.toLocaleString('en-US') }; }), total: 'Total ' + fmtDur(rp.total) + ', peak ' + rp.maxU.toLocaleString('en-US') + ' users', desc: 'Users over time: ' + stages.map(function (s) { return isObj(s) ? s.users + ' after ' + s.duration : ''; }).join(', ') },
      rTable('Stages', { gcols: 'minmax(90px,1fr) minmax(90px,1fr) 32px', heads: [{ t: 'Duration' }, { t: 'Users' }, { t: '' }], items: sitems, addLbl: 'Add stage', add: function () { B.c.editModel(function (mm) { if (!Array.isArray(mm.stages)) mm.stages = []; mm.stages.push({ duration: '1m', users: 0 }); }); } })
    ]));
  }
  if (B.sel === 'thresholds') {
    secs.push(sec('Thresholds', { sub: 'The run fails when a metric breaks its threshold' }, [
      rMatch(B, 'Thresholds', ['thresholds'], { pathOpts: LOAD_METRICS, pathHead: 'Metric', newM: 'lt', newV: '1s', mopts: ['lt', 'lte', 'gt', 'gte'].map(function (x) { return { v: x, l: x }; }) })
    ]));
  }
  return { nav: nav, secs: secs, add: null };
}
function buildDatasetForm(B, P, usedBy) {
  var data = P.ok ? P.value : { cols: [], rows: [] }, secs = [];
  secs.push(sec(B.fpath, { sub: 'Read as ${dataset.' + camelOf(baseOf(B.fpath).replace(/\.csv$/, '')) + '} or with forEach: {$file: …}', mono: true }, [
    gridRow(B, 'Rows', ['csv'], data.cols, data.rows.map(function (r) { return data.cols.map(function (c, ci) { return r[ci] === undefined ? '' : r[ci]; }); }), {
      colsEditable: true, noDraft: true,
      diagOf: function (ri) { var r = data.rows[ri]; return r && r.length !== data.cols.length ? 'This row has ' + r.length + ' values; the header has ' + data.cols.length : null; },
      setCell: function (ri, ci, raw) { B.c.editModel(function (m) { var r = m.rows[ri]; while (r.length < m.cols.length) r.push(''); r[ci] = raw; }); },
      renameCol: function (ci, raw) { B.c.editModel(function (m) { m.cols[ci] = raw; }); },
      delCol: function (ci) { B.c.editModel(function (m) { m.cols.splice(ci, 1); m.rows.forEach(function (r) { r.splice(ci, 1); }); }); },
      addCol: function () { B.c.editModel(function (m) { m.cols.push(uniqueId('column', m.cols).replace(/^column2$/, m.cols.indexOf('column') < 0 ? 'column' : 'column2')); m.rows.forEach(function (r) { r.push(''); }); }); },
      addRow: function () { B.c.editModel(function (m) { m.rows.push(m.cols.map(function () { return ''; })); }); },
      delRow: function (ri) { B.c.editModel(function (m) { m.rows.splice(ri, 1); }); }
    })
  ]));
  secs.push(sec('Used by', { sub: usedBy.length ? usedBy.length + ' reference' + (usedBy.length > 1 ? 's' : '') : 'Nothing references this dataset yet' }, [rChips('', usedBy.map(function (u) { return { t: u, cls: 'pill', title: u }; }))]));
  return { nav: [], secs: secs, add: null };
}
function usedByOf(target, texts) {
  var out = [], base = baseOf(target), id = /\.csv$/.test(target) ? camelOf(base.replace(/\.csv$/, '')) : null;
  Object.keys(texts).forEach(function (p) {
    if (p === target || !isYamlKind(fileKind(p))) return;
    var lines = texts[p].split('\n');
    lines.forEach(function (l, i) {
      var hit = false, m = /(\.\.\/)*[\w.\/-]*?([\w-]+\.(?:json|csv|ts))(#[\w$\/]+)?/g, mm;
      while ((mm = m.exec(l))) { if (resolvePath(p, mm[0].split('#')[0]) === target || (fileKind(p) === 'config' && mm[0].split('#')[0] === target)) hit = true; }
      if (id && l.indexOf('${dataset.' + id + '}') >= 0) hit = true;
      if (hit) out.push(p + ':' + (i + 1));
    });
  });
  return out;
}
