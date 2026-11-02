class Component extends DCLogic {
  componentWillUnmount() { clearTimeout(this._tt); clearTimeout(this._ct); clearTimeout(this._at); }
  renderVals() {
    const clone = (o) => JSON.parse(JSON.stringify(o));
    const METHODS = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE'];
    const INIT = {
      project: 'shop',
      include: ['services/**/*.api.yaml', 'services/**/*.test.yaml', 'load/*.load.yaml'],
      exclude: ['services/**/drafts/**'],
      timeout: '10s', concurrency: '32', perHost: '16', attempts: '2', delay: '500ms',
      headers: [['accept', 'application/json'], ['user-agent', 'vibranium/2 (${run.id})']],
      vars: [['usersUrl', 'http://localhost:8080'], ['ordersUrl', 'http://localhost:8081'], ['catalogUrl', 'http://localhost:8082'], ['identityUrl', 'http://localhost:8090'], ['partnerTenant', 'shop-dev']],
      profiles: [
        { id: 'dev', name: 'Laptop and local stack', def: true, vars: {}, agents: { run: true, call: ['GET', 'POST', 'PUT', 'PATCH', 'DELETE'], approve: [], write: 'yes' } },
        { id: 'staging', name: 'Shared staging', def: false, vars: { usersUrl: 'https://users.staging.shop.example', ordersUrl: 'https://orders.staging.shop.example', catalogUrl: 'https://catalog.staging.shop.example', identityUrl: 'https://id.staging.shop.example', partnerTenant: 'shop-staging' }, agents: { run: true, call: ['GET', 'POST'], approve: ['PUT', 'PATCH', 'DELETE'], write: 'ask' } },
        { id: 'prod', name: 'Production, smoke tests only', def: false, vars: { usersUrl: 'https://users.shop.example', ordersUrl: 'https://orders.shop.example', catalogUrl: 'https://catalog.shop.example', identityUrl: 'https://id.shop.example', partnerTenant: 'shop' }, agents: { run: true, call: ['GET'], approve: ['POST', 'PUT', 'PATCH', 'DELETE'], write: 'no' } }
      ],
      auth: [
        { id: 'shopOauth', type: 'oauth2ClientCredentials', f: { tokenUrl: '${var.identityUrl}/oauth/token', clientId: '${secret.SHOP_CLIENT_ID}', clientSecret: '${secret.SHOP_CLIENT_SECRET}', scopes: 'users.read users.write orders.write', audience: '', refreshBefore: '1m' }, steps: [], headers: [] },
        { id: 'partnerLogin', type: 'flow', f: {}, steps: [
          { id: 'login', method: 'POST', url: '${var.identityUrl}/login', body: 'tenant: ${var.partnerTenant}\nusername: ${secret.PARTNER_USER}\npassword: ${secret.PARTNER_PASSWORD}', save: [['sessionId', '${response.body.session.id}']] },
          { id: 'token', method: 'POST', url: '${var.identityUrl}/token', body: 'grant_type: session\nsession_id: ${var.sessionId}', save: [['accessToken', '${response.body.access_token}'], ['expiresIn', '${response.body.expires_in | int}']] }
        ], headers: [['authorization', 'Bearer ${var.accessToken}']] }
      ],
      reports: { ctrf: true, junit: true, html: true, layout: 'folder', bodies: 'failures', maxBody: '64kb', runs: '500', days: '90' },
      alerts: { channels: [{ id: 'apiTests', type: 'slack', target: '${secret.SLACK_WEBHOOK_URL}', note: '#shop-api-tests' }, { id: 'oncall', type: 'webhook', target: 'https://hooks.shop.example/vibranium', note: '' }], on: { failed: true, blocked: true, flaky: false, recovered: true }, profiles: ['staging', 'prod'] },
      plugins: [
        { id: '@vibranium/plugin-aws-sigv4', version: '1.4.0', what: 'Signs requests with AWS Signature V4; used through auth type plugin.', net: ['*.amazonaws.com'], sec: ['AWS_ACCESS_KEY_ID', 'AWS_SECRET_ACCESS_KEY'], files: [] },
        { id: '@shop/vc-fixtures', version: '0.6.2', what: 'Generators for shop test data: skus, carts and addresses.', net: [], sec: [], files: ['datasets/** (read)', '.vibranium/cache (write)'] }
      ]
    };
    const ENV0 = [
      { name: 'SHOP_CLIENT_ID', desc: 'OAuth client id for the shop APIs', src: 'op', ref: 'op://shop-$VC_PROFILE/oauth/client_id', st: { dev: 'ok', staging: 'ok', prod: 'ok' } },
      { name: 'SHOP_CLIENT_SECRET', desc: 'OAuth client secret for the shop APIs', src: 'op', ref: 'op://shop-$VC_PROFILE/oauth/client_secret', st: { dev: 'ok', staging: 'ok', prod: 'missing' } },
      { name: 'PARTNER_USER', desc: 'Partner portal test account, user name', src: 'vault', ref: 'secret/shop/$VC_PROFILE/partner#username', st: { dev: 'ok', staging: 'ok', prod: 'ok' } },
      { name: 'PARTNER_PASSWORD', desc: 'Partner portal test account, password', src: 'vault', ref: 'secret/shop/$VC_PROFILE/partner#password', st: { dev: 'ok', staging: 'ok', prod: 'ok' } },
      { name: 'SLACK_WEBHOOK_URL', desc: 'Incoming webhook for #shop-api-tests alerts', src: 'env', ref: 'set as a CI secret', st: { dev: 'missing', staging: 'ok', prod: 'ok' } },
      { name: 'AWS_ACCESS_KEY_ID', desc: 'Read-only key for the export bucket checks', src: 'vault', ref: 'secret/shop/$VC_PROFILE/aws#access_key_id', st: { dev: 'ok', staging: 'ok', prod: 'ok' } },
      { name: 'AWS_SECRET_ACCESS_KEY', desc: 'Secret for AWS_ACCESS_KEY_ID', src: 'vault', ref: 'secret/shop/$VC_PROFILE/aws#secret_access_key', st: { dev: 'ok', staging: 'ok', prod: 'ok' } }
    ];
    const USED = { shopOauth: ['users', 'orders', 'carts', 'catalog'], partnerLogin: ['payments'] };
    const TYPES = [
      ['basic', 'Basic', 'User name and password in an Authorization: Basic header.'],
      ['bearer', 'Bearer token', 'A fixed token sent as Authorization: Bearer.'],
      ['apiKey', 'API key', 'A key in a header or query parameter.'],
      ['oauth2ClientCredentials', 'OAuth 2 client credentials', 'Service-to-service token from a token URL; refreshed before it expires.'],
      ['oauth2AuthorizationCode', 'OAuth 2 authorization code', 'A person signs in once in the browser; vc keeps the refresh token in .vibranium/cache.db.'],
      ['oauth2Refresh', 'OAuth 2 refresh token', 'Exchanges a long-lived refresh token for access tokens.'],
      ['flow', 'Flow', 'Your own login steps: requests that save values, then headers built from them.'],
      ['plugin', 'Plugin', 'An installed plugin signs or decorates each request (for example AWS SigV4).']
    ];
    const TYPE_LABEL = {}; TYPES.forEach((t) => { TYPE_LABEL[t[0]] = t[1]; });
    const FIELDS = {
      basic: [['username', 'User name', 'text'], ['password', 'Password', 'secret']],
      bearer: [['token', 'Token', 'secret']],
      apiKey: [['in', 'Send the key in', 'choice'], ['name', 'Header or query parameter name', 'text'], ['value', 'Key', 'secret']],
      oauth2ClientCredentials: [['tokenUrl', 'Token URL', 'url'], ['clientId', 'Client id', 'text'], ['clientSecret', 'Client secret', 'secret'], ['clientAuth', 'Send client credentials in', 'choice'], ['scopes', 'Scopes, separated by spaces', 'opt'], ['audience', 'Audience (optional)', 'opt'], ['refreshBefore', 'Refresh before expiry', 'dur']],
      oauth2AuthorizationCode: [['authorizeUrl', 'Authorize URL', 'url'], ['tokenUrl', 'Token URL', 'url'], ['clientId', 'Client id', 'text'], ['clientSecret', 'Client secret', 'secret'], ['redirectUri', 'Redirect URI', 'url'], ['scopes', 'Scopes, separated by spaces', 'opt']],
      oauth2Refresh: [['tokenUrl', 'Token URL', 'url'], ['clientId', 'Client id', 'text'], ['refreshToken', 'Refresh token', 'secret'], ['refreshBefore', 'Refresh before expiry', 'dur']],
      plugin: [['plugin', 'Plugin', 'plugin'], ['with', 'Options for the plugin, one key: value per line', 'area']],
      flow: []
    };
    const FDEF = { tokenUrl: '${var.identityUrl}/oauth/token', authorizeUrl: '${var.identityUrl}/oauth/authorize', redirectUri: 'http://localhost:7420/oauth/callback', refreshBefore: '1m', in: 'header', clientAuth: 'body', name: 'x-api-key', with: 'region: eu-west-1\nservice: execute-api' };
    const W0 = { step: 1, type: 'oauth2ClientCredentials', name: '', id: '', idEdited: false, f: {}, steps: [{ id: 'login', method: 'POST', url: '${var.identityUrl}/login', body: 'username: ${secret.PARTNER_USER}\npassword: ${secret.PARTNER_PASSWORD}', save: [['accessToken', '${response.body.access_token}']] }], headers: [['authorization', 'Bearer ${var.accessToken}']] };
    const D = { sideCollapsed: false, profile: 'staging', sec: 'general', cfg: null, base: null, env: null, envBase: null, dlg: '', diffFile: 'cfg', np: { name: '', title: '', copy: '', def: false }, delP: '', profMsg: '', authSel: 'shopOauth', authMsg: '', tprof: '', test: null, wiz: null, chk: { running: false, at: '09:12', note: '' }, ns: { name: '', desc: '', src: 'op', ref: '' }, nsOpen: false, alert: { state: 'idle', prof: '' }, nc: { type: 'slack', id: '', target: '', note: '' }, ncOpen: false, agentProf: '', toast: '', newVar: '', plMsg: '' };
    const s = Object.assign({}, D, this.state || {});
    const base = s.base || INIT; const cfg = s.cfg || base;
    const envBase = s.envBase || ENV0; const env = s.env || envBase;
    const set = (o) => this.setState(o);
    const keep = { base, cfg, envBase, env };
    const upd = (fn, extra) => { const c = clone(cfg); fn(c); set(Object.assign({}, keep, { cfg: c, toast: '' }, extra || {})); };
    const updEnv = (fn, extra) => { const e = clone(env); fn(e); set(Object.assign({}, keep, { env: e, toast: '' }, extra || {})); };
    const val = (e) => (e && e.target ? e.target.value : '');
    const opt = (label, on, pick, extra) => Object.assign({ label, on, cls: on ? 'on' : '', pick }, extra || {});
    const PROFS = cfg.profiles.map((p) => p.id);
    const defProf = (cfg.profiles.find((p) => p.def) || cfg.profiles[0] || { id: 'dev' }).id;
    const profOk = (p) => PROFS.indexOf(p) >= 0;

    // ---------- YAML ----------
    const sc = (v) => {
      if (v === true || v === false) return String(v);
      v = String(v);
      if (v === '') return "''";
      if (/^[*@&!%`'"#|>{}\[\],?]/.test(v) || /: |\s#|:$/.test(v) || /^(true|false|null|yes|no|~)$/i.test(v)) return "'" + v.replace(/'/g, "''") + "'";
      return v;
    };
    const flowSeq = (arr) => '[' + arr.map(sc).join(', ') + ']';
    const camelToSnake = (x) => String(x).replace(/([a-z0-9])([A-Z])/g, '$1_$2').replace(/[^A-Za-z0-9]+/g, '_').toUpperCase();
    const authLines = (a, ind) => {
      const L = []; const p = ind || '';
      L.push(p + '- id: ' + a.id);
      L.push(p + '  type: ' + a.type);
      (FIELDS[a.type] || []).forEach((fd) => {
        const v = (a.f || {})[fd[0]];
        if (v == null || v === '') return;
        if (fd[0] === 'clientAuth' && v === 'body') return;
        if (fd[0] === 'scopes') { L.push(p + '  scopes: ' + flowSeq(String(v).trim().split(/\s+/))); return; }
        if (fd[2] === 'area') { L.push(p + '  ' + fd[0] + ':'); String(v).split('\n').filter((x) => x.trim()).forEach((x) => L.push(p + '    ' + x.trim())); return; }
        L.push(p + '  ' + fd[0] + ': ' + sc(v));
      });
      if (a.type === 'flow') {
        L.push(p + '  steps:');
        (a.steps || []).forEach((st, i) => {
          L.push(p + '    - id: ' + (st.id || 'step' + (i + 1)));
          L.push(p + '      method: ' + st.method);
          L.push(p + '      url: ' + sc(st.url));
          const bl = String(st.body || '').split('\n').filter((x) => x.trim());
          if (bl.length) { L.push(p + '      body:'); bl.forEach((x) => L.push(p + '        ' + x.trim())); }
          const sv = (st.save || []).filter((x) => x[0]);
          if (sv.length) { L.push(p + '      save:'); sv.forEach((x) => L.push(p + '        ' + x[0] + ': ' + sc(x[1]))); }
        });
        const hs = (a.headers || []).filter((x) => x[0]);
        if (hs.length) { L.push(p + '  apply:'); L.push(p + '    headers:'); hs.forEach((x) => L.push(p + '      ' + x[0] + ': ' + sc(x[1]))); }
        const ex = (a.steps || []).some((st) => (st.save || []).some((x) => x[0] === 'expiresIn'));
        if (ex) L.push(p + '  expiresIn: ${var.expiresIn}s');
      }
      return L;
    };
    const profLines = (pr, ind) => {
      const p = ind || ''; const L = [p + '- id: ' + pr.id];
      if (pr.name) L.push(p + '  name: ' + sc(pr.name));
      if (pr.def) L.push(p + '  default: true');
      const ov = cfg.vars.filter((v) => pr.vars && pr.vars[v[0]] != null && pr.vars[v[0]] !== '');
      if (ov.length) { L.push(p + '  vars:'); ov.forEach((v) => L.push(p + '    ' + v[0] + ': ' + sc(pr.vars[v[0]]))); }
      const ag = pr.agents;
      L.push(p + '  agents:');
      L.push(p + '    run: ' + (ag.run ? 'true' : 'false'));
      L.push(p + '    call: ' + flowSeq(ag.call));
      L.push(p + '    approve: ' + flowSeq(ag.approve));
      L.push(p + '    writeDefinitions: ' + (ag.write === 'yes' ? 'true' : ag.write === 'no' ? 'false' : 'ask'));
      return L;
    };
    const cfgLines = (c) => {
      const L = ['syntaxVersion: 2', 'project: ' + sc(c.project), ''];
      L.push('paths:'); L.push('  include:'); c.include.forEach((x) => L.push('    - ' + sc(x)));
      if (c.exclude.length) { L.push('  exclude:'); c.exclude.forEach((x) => L.push('    - ' + sc(x))); }
      L.push('', 'defaults:', '  timeout: ' + sc(c.timeout), '  concurrency: ' + sc(c.concurrency), '  perHost: ' + sc(c.perHost), '  retry:', '    attempts: ' + sc(c.attempts), '    delay: ' + sc(c.delay));
      if (c.headers.length) { L.push('  headers:'); c.headers.forEach((h) => L.push('    ' + (h[0] || '?') + ': ' + sc(h[1]))); }
      L.push('', 'vars:'); c.vars.forEach((v) => L.push('  ' + v[0] + ': ' + sc(v[1])));
      L.push('', 'profiles:'); c.profiles.forEach((pr) => { profLines.call(null, pr, '  ').forEach((x) => L.push(x)); });
      L.push('', 'auth:'); c.auth.forEach((a) => authLines(a, '  ').forEach((x) => L.push(x)));
      L.push('', 'secrets:', '  provider: varlock', '  schema: .env.schema');
      L.push('', 'datasets:', '  capitals: datasets/capitals.csv', '', 'redact:', '  headers: [authorization, cookie, x-api-key]');
      const r = c.reports; const fm = ['ctrf', 'junit', 'html'].filter((k) => r[k]);
      L.push('', 'reports:', '  dir: .vibranium/reports', '  formats: ' + flowSeq(fm));
      if (r.html) { L.push('  html:'); L.push('    layout: ' + (r.layout === 'folder' ? 'folder' : 'singleFile')); }
      L.push('  bodies: ' + r.bodies);
      if (r.bodies !== 'none') L.push('  maxBodySize: ' + sc(r.maxBody));
      L.push('', 'history:', '  keep:', '    runs: ' + sc(r.runs), '    days: ' + sc(r.days));
      const al = c.alerts;
      L.push('', 'alerts:', '  channels:');
      al.channels.forEach((ch) => { L.push('    - id: ' + ch.id); L.push('      type: ' + ch.type); L.push('      ' + (ch.type === 'slack' ? 'webhook' : ch.type === 'email' ? 'to' : 'url') + ': ' + sc(ch.target)); if (ch.note) L.push('      channel: ' + sc(ch.note)); });
      L.push('  on: ' + flowSeq(['failed', 'blocked', 'flaky', 'recovered'].filter((k) => al.on[k])));
      L.push('  profiles: ' + flowSeq(al.profiles.filter((p) => c.profiles.some((x) => x.id === p))));
      if (c.plugins.length) {
        L.push('', 'plugins:');
        c.plugins.forEach((pl) => {
          L.push('  - id: ' + sc(pl.id)); L.push('    version: ' + sc(pl.version)); L.push('    permissions:');
          L.push('      network: ' + flowSeq(pl.net)); L.push('      secrets: ' + flowSeq(pl.sec)); L.push('      files: ' + flowSeq(pl.files));
        });
      }
      L.push('', 'beforeRun:', '  - id: admin', '    call: users.create', '    vars:', '      role: admin', '    cache:', '      for: 12h');
      L.push('', 'afterRun:', '  - function: hooks/notify.ts#postSummary');
      return L;
    };
    const srcExpr = (e) => {
      if (e.src === 'op') return "exec('op read \"" + e.ref + "\"')";
      if (e.src === 'vault') { const parts = String(e.ref).split('#'); return "exec('vault kv get -field=" + (parts[1] || 'value') + ' ' + parts[0] + "')"; }
      return '';
    };
    const envLines = (E) => {
      const L = ['# Secrets for the shop project. Varlock resolves them per profile;', '# values never live in git or in vibranium.config.yaml.', '# @defaultSensitive=true @defaultRequired=true', '# @envFlag=VC_PROFILE', '# ---'];
      E.forEach((e) => { L.push(''); L.push('# ' + e.desc + (e.src === 'env' ? ' (from the environment: ' + e.ref + ')' : '')); L.push(e.name + '=' + srcExpr(e)); });
      return L;
    };
    // highlighting
    const valSegs = (v, out) => {
      if (!v) return;
      const lead = v.match(/^\s*/)[0]; if (lead) out.push({ c: '', t: lead });
      const body = v.slice(lead.length);
      if (/^#/.test(body)) { out.push({ c: 'y-com', t: body }); return; }
      const cm = body.match(/^(.*?)(\s+#.*)$/);
      const main = cm ? cm[1] : body;
      if (/^-?\d+(\.\d+)?$/.test(main) || /^(true|false)$/.test(main)) out.push({ c: 'y-num', t: main });
      else main.split(/(\$\{[^}]*\}|^\[|\]$|, )/).forEach((p) => { if (!p) return; out.push({ c: /^\$\{/.test(p) ? 'y-tpl' : (/^(\[|\]|, )$/.test(p) ? 'y-punc' : 'y-str'), t: p }); });
      if (cm) out.push({ c: 'y-com', t: cm[2] });
    };
    const tok = (line) => {
      const out = [];
      if (/^\s*#/.test(line)) { out.push({ c: 'y-com', t: line }); return out; }
      const em = line.match(/^([A-Z][A-Z0-9_]*)(=)(.*)$/);
      if (em) { out.push({ c: 'y-key', t: em[1] }); out.push({ c: 'y-punc', t: '=' }); if (em[3]) out.push({ c: 'y-str', t: em[3] }); return out; }
      const m = line.match(/^(\s*)(- )?([A-Za-z0-9_$.@'-][A-Za-z0-9_$.@/' -]*?)(:)(\s.*)?$/);
      if (m) {
        if (m[1]) out.push({ c: '', t: m[1] });
        if (m[2]) out.push({ c: 'y-punc', t: '- ' });
        out.push({ c: 'y-key', t: m[3] }); out.push({ c: 'y-punc', t: ':' });
        valSegs(m[5] || '', out); return out;
      }
      const m2 = line.match(/^(\s*)(- )(.*)$/);
      if (m2) { if (m2[1]) out.push({ c: '', t: m2[1] }); out.push({ c: 'y-punc', t: '- ' }); valSegs(m2[3], out); return out; }
      if (!line) { out.push({ c: '', t: ' ' }); return out; }
      valSegs(line, out); return out;
    };
    const codeOf = (lines, start) => lines.map((l, i) => ({ no: (start || 1) + i, segs: tok(l) }));
    // diff
    const diffOps = (A, B) => {
      const n = A.length, m = B.length;
      const dp = []; for (let i = 0; i <= n; i++) dp.push(new Array(m + 1).fill(0));
      for (let i = n - 1; i >= 0; i--) for (let j = m - 1; j >= 0; j--) dp[i][j] = A[i] === B[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
      const ops = []; let i = 0, j = 0;
      while (i < n && j < m) {
        if (A[i] === B[j]) { ops.push({ t: ' ', a: i + 1, b: j + 1, x: A[i] }); i++; j++; }
        else if (dp[i + 1][j] >= dp[i][j + 1]) { ops.push({ t: '-', a: i + 1, b: null, x: A[i] }); i++; }
        else { ops.push({ t: '+', a: null, b: j + 1, x: B[j] }); j++; }
      }
      while (i < n) { ops.push({ t: '-', a: i + 1, b: null, x: A[i] }); i++; }
      while (j < m) { ops.push({ t: '+', a: null, b: j + 1, x: B[j] }); j++; }
      return ops;
    };
    const hunksOf = (ops) => {
      const ch = []; ops.forEach((o, k) => { if (o.t !== ' ') ch.push(k); });
      const H = [];
      ch.forEach((k) => { const st = Math.max(0, k - 3), en = Math.min(ops.length - 1, k + 3); const last = H[H.length - 1]; if (last && st <= last.en + 1) last.en = Math.max(last.en, en); else H.push({ st, en }); });
      return H.map((h) => {
        const seg = ops.slice(h.st, h.en + 1);
        const a0 = (seg.find((o) => o.a) || {}).a || 0; const b0 = (seg.find((o) => o.b) || {}).b || 0;
        const na = seg.filter((o) => o.t !== '+').length; const nb = seg.filter((o) => o.t !== '-').length;
        return { head: '@@ -' + a0 + ',' + na + ' +' + b0 + ',' + nb + ' @@', ops: seg };
      });
    };
    // hunks counted per change group (adjacent edits are one change)
    const changeCount = (ops) => { let n = 0, inRun = false; ops.forEach((o) => { if (o.t !== ' ') { if (!inRun) n++; inRun = true; } else inRun = false; }); return n; };
    const baseLines = cfgLines(base), curLines = cfgLines(cfg);
    const cfgOps = diffOps(baseLines, curLines);
    const envBaseLines = envLines(envBase), envCurLines = envLines(env);
    const envOps = diffOps(envBaseLines, envCurLines);
    const nCfg = changeCount(cfgOps), nEnv = changeCount(envOps);
    const dirty = nCfg + nEnv > 0;
    const plural = (n, w) => n + ' ' + w + (n === 1 ? '' : 's');

    // ---------- resolution helpers ----------
    const varNames = cfg.vars.map((v) => v[0]);
    const varVal = (k, prof) => { const b = cfg.vars.find((v) => v[0] === k); if (!b) return null; const pr = cfg.profiles.find((p) => p.id === prof); const o = pr && pr.vars ? pr.vars[k] : null; return o != null && o !== '' ? o : b[1]; };
    const SEC_RE = /^\$\{secret\.([A-Z][A-Z0-9_]*)\}$/;
    const envNames = env.map((e) => e.name);
    const refsOf = (str, kind) => { const out = []; const re = new RegExp('\\$\\{' + kind + '\\.([A-Za-z0-9_]+)', 'g'); let m; while ((m = re.exec(String(str || '')))) out.push(m[1]); return out; };
    const stOf = (e, p) => (e.st && e.st[p]) || 'unchecked';
    const savedNames = (a) => { const out = []; (a.steps || []).forEach((st) => (st.save || []).forEach((x) => { if (x[0]) out.push(x[0]); })); return out; };
    const checkField = (kind, v, a, key) => {
      v = String(v == null ? '' : v);
      const extraVars = a ? savedNames(a) : [];
      const badVars = refsOf(v, 'var').filter((k) => varNames.indexOf(k) < 0 && extraVars.indexOf(k) < 0);
      const sugg = '${secret.' + camelToSnake((a && a.id) || 'auth') + '_' + camelToSnake(key) + '}';
      if (kind === 'secret') {
        if (!v.trim()) return { err: 'Required: a ${secret.NAME} reference.' };
        const m = v.trim().match(SEC_RE);
        if (!m) return { err: 'Secret fields only accept a ${secret.NAME} reference. A literal value would be written to vibranium.config.yaml and committed to git.', sugg };
        if (envNames.indexOf(m[1]) < 0) return { warn: m[1] + ' is not declared in .env.schema yet.' , missing: m[1] };
        return { help: m[1] + ' · resolved by Varlock from .env.schema' };
      }
      if (badVars.length) return { err: '${var.' + badVars[0] + '} is not defined. Add it under Profiles → Variables.' };
      const secs = refsOf(v, 'secret').filter((k) => envNames.indexOf(k) < 0);
      if (kind === 'url') {
        if (!v.trim()) return { err: 'Required.' };
        if (!/^(\$\{var\.|https?:\/\/)/.test(v.trim())) return { err: 'Start with ${var.…} or https://' };
        if (/^https?:\/\//.test(v.trim()) && !/localhost/.test(v)) return { warn: 'A literal host is the same for every profile. Prefer ${var.…}, for example ${var.identityUrl}/oauth/token.' };
        return { help: 'Resolves per profile, e.g. ' + v.replace(/\$\{var\.([A-Za-z0-9_]+)\}/g, (x, k) => varVal(k, s.profile) || x) };
      }
      if (kind === 'text' && !v.trim()) return { err: 'Required.' };
      if (secs.length) return { warn: secs[0] + ' is not declared in .env.schema yet.', missing: secs[0] };
      if (kind === 'dur' && !/^\d+(s|m|h)$/.test(v.trim())) return { err: 'Use a duration like 30s, 1m or 1h.' };
      if (kind === 'text') return { help: 'A literal, ${var.…} or ${secret.…}' };
      if (kind === 'dur') return { help: 'Tokens are refreshed this long before they expire.' };
      return { help: '' };
    };

    // ---------- problems ----------
    const problems = [];
    const intIn = (v, lo, hi) => /^\d+$/.test(String(v).trim()) && +v >= lo && +v <= hi;
    const gErr = {
      project: !String(cfg.project).trim() ? 'Required.' : (!/^[a-z][a-z0-9-]*$/.test(cfg.project.trim()) ? 'Lowercase letters, digits and dashes, like shop.' : ''),
      timeout: /^\d+(ms|s|m)$/.test(cfg.timeout.trim()) ? '' : 'Use a duration like 10s or 500ms.',
      concurrency: intIn(cfg.concurrency, 1, 512) ? '' : 'A whole number from 1 to 512.',
      perHost: !intIn(cfg.perHost, 1, 512) ? 'A whole number from 1 to 512.' : (+cfg.perHost > +cfg.concurrency ? 'Cannot be higher than concurrency (' + cfg.concurrency + ').' : ''),
      attempts: intIn(cfg.attempts, 0, 10) ? '' : 'A whole number from 0 to 10.',
      delay: /^\d+(ms|s)$/.test(cfg.delay.trim()) ? '' : 'Use a duration like 500ms or 2s.'
    };
    Object.keys(gErr).forEach((k) => { if (gErr[k]) problems.push({ sec: 'general', text: 'General · ' + k + ': ' + gErr[k] }); });
    cfg.include.forEach((x) => { if (!x.trim()) problems.push({ sec: 'general', text: 'General · an include pattern is empty' }); });
    cfg.headers.forEach((h) => { if (!/^[a-z0-9-]+$/.test(h[0])) problems.push({ sec: 'general', text: 'General · header name "' + h[0] + '" must be lowercase letters, digits and dashes' }); });
    const authProblems = {};
    cfg.auth.forEach((a) => {
      const list = [];
      (FIELDS[a.type] || []).forEach((fd) => { const r = checkField(fd[2], (a.f || {})[fd[0]], a, fd[0]); if (r.err) list.push(a.id + '.' + fd[0] + ': ' + r.err); });
      (a.steps || []).forEach((st, i) => { const r = checkField('url', st.url, a, 'url'); if (r.err) list.push(a.id + ' step ' + (i + 1) + ' url: ' + r.err); });
      authProblems[a.id] = list;
      list.forEach((t) => problems.push({ sec: 'auth', text: 'Authentication · ' + t }));
    });
    const R = cfg.reports;
    if (!R.ctrf && !R.junit && !R.html) problems.push({ sec: 'reports', text: 'Reports · pick at least one report format' });
    if (R.bodies !== 'none' && !/^\d+(kb|mb)$/.test(String(R.maxBody).trim())) problems.push({ sec: 'reports', text: 'Reports · max body size: use a size like 64kb or 1mb' });
    if (!intIn(R.runs, 1, 100000)) problems.push({ sec: 'reports', text: 'Reports · keep runs: a whole number' });
    if (!intIn(R.days, 1, 3650)) problems.push({ sec: 'reports', text: 'Reports · keep days: a whole number' });

    // ---------- sections ----------
    const lineOf = (key) => curLines.indexOf(key + ':') + 1;
    const span = (key) => { const a = lineOf(key); let b = curLines.length; for (let i = a; i < curLines.length; i++) if (/^[a-zA-Z]/.test(curLines[i])) { b = i; break; } while (b > a && !curLines[b - 1]) b--; return 'lines ' + a + '–' + b; };
    const jsonEq = (x, y) => JSON.stringify(x) === JSON.stringify(y);
    const SECS = [
      ['general', 'General', 'M3 4.5h10M3 8h10M3 11.5h6', '', !jsonEq([base.project, base.include, base.exclude, base.timeout, base.concurrency, base.perHost, base.attempts, base.delay, base.headers], [cfg.project, cfg.include, cfg.exclude, cfg.timeout, cfg.concurrency, cfg.perHost, cfg.attempts, cfg.delay, cfg.headers])],
      ['profiles', 'Profiles', 'M8 2.5 14 5.5 8 8.5 2 5.5zM2 8.5l6 3 6-3M2 11.5l6 3 6-3', String(cfg.profiles.length), !jsonEq([base.vars, base.profiles.map((p) => [p.id, p.name, p.def, p.vars])], [cfg.vars, cfg.profiles.map((p) => [p.id, p.name, p.def, p.vars])])],
      ['auth', 'Authentication', 'M4.5 7.5h7v6h-7zM6 7.5V5.5a2 2 0 0 1 4 0v2', String(cfg.auth.length), !jsonEq(base.auth, cfg.auth)],
      ['secrets', 'Secrets', 'M8 2 13 4v4c0 3-2.2 5.2-5 6-2.8-.8-5-3-5-6V4z', String(env.length), nEnv > 0],
      ['reports', 'Reports and history', 'M4 2.5h5.5L12 5v8.5H4zM9.5 2.5V5H12M6 8.5h4M6 11h4', '', !jsonEq(base.reports, cfg.reports)],
      ['alerts', 'Alerts', 'M4 11V7.5a4 4 0 0 1 8 0V11l1 1.5H3zM6.5 14h3', String(cfg.alerts.channels.length), !jsonEq(base.alerts, cfg.alerts)],
      ['agents', 'Agent access', 'M3 5h10v8H3zM8 2.5V5M6 8.5v1.5M10 8.5v1.5', '', !jsonEq(base.profiles.map((p) => p.agents), cfg.profiles.map((p) => p.agents))],
      ['plugins', 'Plugins', 'M6 2v3M10 2v3M4.5 5h7v3a3.5 3.5 0 0 1-7 0zM8 11.5V14', String(cfg.plugins.length), !jsonEq(base.plugins, cfg.plugins)]
    ];
    const sec = SECS.some((x) => x[0] === s.sec) ? s.sec : 'general';
    const go = (k) => () => set({ sec: k, profMsg: '', authMsg: '', plMsg: '' });
    const secs = SECS.map((x) => ({ key: x[0], label: x[1], icon: x[2], count: x[3], dirty: !!x[4], on: sec === x[0], cls: sec === x[0] ? 'on' : '', pick: go(x[0]) }));
    const LOCKEY = { general: 'paths', profiles: 'profiles', auth: 'auth', secrets: 'secrets', reports: 'reports', alerts: 'alerts', agents: 'profiles', plugins: 'plugins' };
    const locs = {};
    SECS.forEach((x) => { const k = LOCKEY[x[0]]; locs[x[0]] = lineOf(k) ? 'vibranium.config.yaml › ' + (x[0] === 'agents' ? 'profiles[].agents' : x[0] === 'general' ? 'project, paths, defaults' : x[0] === 'reports' ? 'reports, history' : k) + ' · ' + (x[0] === 'general' ? 'lines 1–' + (lineOf('vars') - 2) : span(k)) : 'vibranium.config.yaml'; });
    locs.secrets = 'vibranium.config.yaml › secrets · ' + span('secrets') + ' and .env.schema';

    // ---------- general ----------
    const fieldOf = (key, err, help) => ({ v: String(cfg[key]), set: (e) => { const v = val(e); upd((c) => { c[key] = v; }); }, cls: err ? 'err' : '', help: err || help, helpCls: err ? 'err' : '' });
    const listRows = (key, minOne) => cfg[key].map((x, i) => ({
      v: x, label: (key === 'include' ? 'Include pattern ' : 'Exclude pattern ') + (i + 1),
      set: (e) => { const v = val(e); upd((c) => { c[key][i] = v; }); },
      remove: () => { if (minOne && cfg[key].length <= 1) return; upd((c) => { c[key].splice(i, 1); }); },
      rmDisabled: !!minOne && cfg[key].length <= 1, rmTitle: !!minOne && cfg[key].length <= 1 ? 'At least one include pattern is needed' : 'Remove this pattern'
    }));
    const g = {
      project: fieldOf('project', gErr.project, 'Shown in reports, alerts and the top bar.'),
      include: listRows('include', true), exclude: listRows('exclude', false),
      addInclude: () => upd((c) => { c.include.push('services/**/*.yaml'); }), addExclude: () => upd((c) => { c.exclude.push('**/*.draft.yaml'); }),
      timeout: fieldOf('timeout', gErr.timeout, 'Per request, unless an api or test sets its own.'),
      concurrency: fieldOf('concurrency', gErr.concurrency, 'Tests running at the same time.'),
      perHost: fieldOf('perHost', gErr.perHost, 'Open requests per host, so one service is not flooded.'),
      attempts: fieldOf('attempts', gErr.attempts, 'Retries after a failed attempt. A pass on retry is reported as flaky.'),
      delay: fieldOf('delay', gErr.delay, 'Wait between attempts.'),
      headers: cfg.headers.map((h, i) => ({ k: h[0], v: h[1], kCls: /^[a-z0-9-]+$/.test(h[0]) ? '' : 'err', aria: 'Header ' + (i + 1),
        setK: (e) => { const v = val(e); upd((c) => { c.headers[i][0] = v; }); }, setV: (e) => { const v = val(e); upd((c) => { c.headers[i][1] = v; }); },
        remove: () => upd((c) => { c.headers.splice(i, 1); }) })),
      addHeader: () => upd((c) => { c.headers.push(['x-request-source', 'vibranium']); }),
      noHeaders: cfg.headers.length === 0
    };

    // ---------- profiles ----------
    const pr = {
      summary: plural(cfg.profiles.length, 'profile') + ' · default ' + defProf,
      rows: cfg.profiles.map((p) => {
        const n = Object.keys(p.vars || {}).filter((k) => p.vars[k] !== '' && p.vars[k] != null).length;
        const ag = p.agents;
        return {
          id: p.id, name: p.name || 'no display name', isDef: !!p.def, notDef: !p.def,
          overrides: n ? plural(n, 'variable override') : 'inherits every variable',
          agents: 'agents: ' + (ag.run ? 'run tests' : 'no runs') + ' · ' + (ag.call.length ? ag.call.join(' ') + ' freely' : 'no calls'),
          makeDef: () => upd((c) => { c.profiles.forEach((x) => { x.def = x.id === p.id; }); }, { profMsg: '' }),
          delAria: 'Delete profile ' + p.id,
          del: () => {
            if (cfg.profiles.length <= 1) { set({ profMsg: 'At least one profile is required: every run resolves its variables, secrets and agent rules through a profile. Add another profile before deleting ' + p.id + '.' }); return; }
            if (p.def) { set({ profMsg: p.id + ' is the default profile: vc run, vc ui and agents use it when no --profile is given. Make another profile the default first, then delete ' + p.id + '.' }); return; }
            set({ dlg: 'delProfile', delP: p.id, profMsg: '' });
          }
        };
      }),
      hasMsg: !!s.profMsg, msg: s.profMsg, closeMsg: () => set({ profMsg: '' }),
      openNew: () => set({ dlg: 'newProfile', np: D.np })
    };
    const mxCols = 'minmax(150px, 0.9fr) repeat(' + (cfg.profiles.length + 1) + ', minmax(200px, 1fr))';
    const mx = {
      cols: mxCols,
      head: [{ label: 'Base value', sub: 'vars:' }].concat(cfg.profiles.map((p) => ({ label: p.id, sub: p.def ? 'default · overrides' : 'overrides' }))),
      rows: cfg.vars.map((v, vi) => ({
        name: v[0],
        cells: [{ v: v[1], ph: 'required', cls: 'basev', aria: v[0] + ' base value', set: (e) => { const x = val(e); upd((c) => { c.vars[vi][1] = x; }); } }].concat(cfg.profiles.map((p, pi) => {
          const o = p.vars && p.vars[v[0]] != null ? p.vars[v[0]] : '';
          return { v: o, ph: v[1] ? v[1] + '  (inherits)' : 'inherits base', cls: o === '' ? 'inherit' : 'over', aria: v[0] + ' for profile ' + p.id + (o === '' ? ', inherits the base value' : ''),
            set: (e) => { const x = val(e); upd((c) => { if (!c.profiles[pi].vars) c.profiles[pi].vars = {}; if (x === '') delete c.profiles[pi].vars[v[0]]; else c.profiles[pi].vars[v[0]] = x; }); } };
        }))
      }))
    };
    const nv = s.newVar.trim();
    const nvOk = /^[a-z][a-zA-Z0-9]*$/.test(nv) && varNames.indexOf(nv) < 0;
    const nvSugg = nv ? (nv.replace(/[^A-Za-z0-9]+(.)?/g, (m2, ch) => (ch ? ch.toUpperCase() : '')).replace(/^[A-Z]/, (x) => x.toLowerCase()).replace(/^[0-9]+/, '')) : '';
    mx.newVar = s.newVar; mx.setNewVar = (e) => set({ newVar: val(e) });
    mx.newCls = nv && !nvOk ? 'err' : '';
    mx.newHelp = !nv ? 'camelCase noun; URLs end in Url, ids in Id: paymentsUrl, partnerId' : (varNames.indexOf(nv) >= 0 ? nv + ' already exists.' : (nvOk ? (/url$/i.test(nv) && !/Url$/.test(nv) ? 'Tip: URLs end in Url (capital U).' : 'Adds ' + nv + ' with an empty base value.') : 'Use camelCase, like ' + (nvSugg || 'paymentsUrl') + '.'));
    mx.newHelpCls = nv && !nvOk ? 'err' : '';
    mx.addDisabled = !nvOk;
    mx.add = () => { if (!nvOk) return; upd((c) => { c.vars.push([nv, '']); }, { newVar: '' }); };

    // new profile dialog
    const np = Object.assign({}, D.np, s.np || {});
    const npName = np.name.trim();
    const npSugg = npName.toLowerCase().replace(/[^a-z0-9]/g, '').replace(/^[0-9]+/, '');
    let npErr = '';
    if (!npName) npErr = '';
    else if (!/^[a-z][a-z0-9]*$/.test(npName)) npErr = 'Profiles are lowercase single words: dev, staging, prod, eu.';
    else if (PROFS.indexOf(npName) >= 0) npErr = npName + ' already exists.';
    const npValid = !!npName && !npErr;
    const setNp = (o) => set({ np: Object.assign({}, np, o) });
    const npSrc = cfg.profiles.find((p) => p.id === np.copy);
    const npObj = { id: npName || '…', name: np.title.trim(), def: !!np.def, vars: npSrc ? clone(npSrc.vars || {}) : {}, agents: npSrc ? clone(npSrc.agents) : { run: true, call: ['GET'], approve: ['POST', 'PUT', 'PATCH', 'DELETE'], write: 'ask' } };
    const npV = {
      name: np.name, setName: (e) => setNp({ name: val(e) }), cls: npErr ? 'err' : '',
      help: npErr || (npName ? 'Run it with vc run --profile ' + npName : 'Lowercase single word, like eu or perf.'), helpCls: npErr ? 'err' : '',
      hasSugg: !!npErr && !!npSugg && npSugg !== npName && PROFS.indexOf(npSugg) < 0, sugg: npSugg, useSugg: () => setNp({ name: npSugg }),
      title: np.title, setTitle: (e) => setNp({ title: val(e) }),
      copyOpts: [opt('Nothing: every variable inherits the base value', !np.copy, () => setNp({ copy: '' }), { sub: 'agents start read-only: GET without asking' })].concat(cfg.profiles.map((p) => {
        const n = Object.keys(p.vars || {}).length;
        return opt('Copy from ' + p.id, np.copy === p.id, () => setNp({ copy: p.id }), { sub: plural(n, 'variable override') + ' and its agent access rules' });
      })),
      def: !!np.def, toggleDef: (e) => setNp({ def: e && e.target ? !!e.target.checked : !np.def }),
      defNote: np.def ? defProf + ' stops being the default. vc run without --profile will use ' + (npName || 'the new profile') + '.' : 'Only one profile is the default; ' + defProf + ' stays the default.',
      yaml: codeOf(profLines(npObj, '  '), 0).map((l) => Object.assign(l, { no: '+' })),
      valid: npValid, invalid: !npValid,
      create: () => { if (!npValid) return; upd((c) => { const o = clone(npObj); o.id = npName; if (o.def) c.profiles.forEach((x) => { x.def = false; }); c.profiles.push(o); }, { dlg: '', np: D.np, sec: 'profiles' }); }
    };
    // delete profile dialog
    const dp = cfg.profiles.find((p) => p.id === s.delP) || cfg.profiles[cfg.profiles.length - 1];
    const dpV = {
      id: dp.id,
      lines: [
        { t: 'Removes profiles › ' + dp.id + ' with ' + plural(Object.keys(dp.vars || {}).length, 'variable override') + ' and its agent access rules.' },
        { t: cfg.alerts.profiles.indexOf(dp.id) >= 0 ? 'Drops ' + dp.id + ' from alerts › profiles.' : 'No alert rule mentions ' + dp.id + '.' },
        { t: '.env.schema is unchanged: entries are per profile through $VC_PROFILE, so nothing is stored for ' + dp.id + ' there.' },
        { t: 'Run history for ' + dp.id + ' stays in .vibranium/history.db. vc run --profile ' + dp.id + ' will fail with “unknown profile”.' }
      ],
      confirm: () => upd((c) => { c.profiles = c.profiles.filter((p) => p.id !== dp.id); c.alerts.profiles = c.alerts.profiles.filter((p) => p !== dp.id); }, { dlg: '', delP: '' })
    };

    // ---------- auth ----------
    const selA = cfg.auth.find((a) => a.id === s.authSel) || cfg.auth[0];
    const usedOf = (id) => USED[id] || [];
    const au = {
      summary: plural(cfg.auth.length, 'definition') + ' · an api file picks one with auth: <id>',
      rows: cfg.auth.map((a) => {
        const u = usedOf(a.id);
        const first = a.type === 'flow' ? plural((a.steps || []).length, 'step') + ' → ' + (a.headers || []).map((h) => h[0]).join(', ') : ((a.f || {}).tokenUrl || (a.f || {}).name || (a.f || {}).plugin || (a.f || {}).username || 'header');
        return { id: a.id, typeLabel: TYPE_LABEL[a.type] || a.type, used: u.length ? 'used by ' + plural(u.length, 'api file') : 'not used yet', usedTitle: u.length ? u.map((x) => 'services/' + x + '/' + x + '.api.yaml').join(', ') : 'No api file sets auth: ' + a.id, summary: first, on: a.id === selA.id, cls: a.id === selA.id ? 'on' : '', pick: () => set({ authSel: a.id, authMsg: '', test: null }), probs: (authProblems[a.id] || []).length ? plural(authProblems[a.id].length, 'problem') : '', hasProbs: (authProblems[a.id] || []).length > 0 };
      }),
      openWizard: () => set({ dlg: 'wizard', wiz: W0 })
    };
    const fieldsFor = (a, writeF, ctxEnvAdd) => (FIELDS[a.type] || []).map((fd) => {
      const key = fd[0], kind = fd[2];
      const v = (a.f || {})[key] != null && a.f[key] !== '' ? String(a.f[key]) : (kind === 'choice' ? (FDEF[key] || '') : '');
      const r = checkField(kind, v, a, key);
      const choices = kind === 'choice' ? (key === 'clientAuth' ? ['body', 'header'] : ['header', 'query']) : kind === 'plugin' ? cfg.plugins.map((p) => p.id) : [];
      return {
        key, label: fd[1], v, isText: ['text', 'url', 'secret', 'opt', 'dur'].indexOf(kind) >= 0, isChoice: kind === 'choice' || kind === 'plugin', isArea: kind === 'area', isSecret: kind === 'secret',
        mono: 'mono', ph: kind === 'secret' ? '${secret.' + camelToSnake(a.id || 'auth') + '_' + camelToSnake(key) + '}' : (FDEF[key] || ''),
        set: (e) => { const x = val(e); writeF((f) => { f[key] = x; }); },
        opts: choices.map((ch) => opt(ch, v === ch, () => writeF((f) => { f[key] = ch; }))),
        noOpts: (kind === 'choice' || kind === 'plugin') && choices.length === 0,
        cls: r.err ? 'err' : (r.warn ? 'warnf' : ''), help: r.err || r.warn || r.help || '', helpCls: r.err ? 'err' : (r.warn ? 'warn' : ''),
        hasSugg: !!r.sugg, sugg: r.sugg || '', useSugg: () => writeF((f) => { f[key] = r.sugg; }),
        hasAddEnv: !!r.missing && !!ctxEnvAdd, addEnvLabel: 'Add ' + (r.missing || '') + ' to .env.schema', addEnv: () => { if (r.missing && ctxEnvAdd) ctxEnvAdd(r.missing, a.id); },
        span: kind === 'area' || key === 'scopes' ? 'span-all' : ''
      };
    });
    const newEnvEntry = (name, authId) => ({ name, desc: 'Used by auth ' + authId, src: 'op', ref: 'op://shop-$VC_PROFILE/' + String(authId).replace(/([a-z0-9])([A-Z])/g, '$1-$2').toLowerCase() + '/' + name.toLowerCase(), st: {} });
    const addEnvFor = (name, authId) => updEnv((e) => { if (!e.some((x) => x.name === name)) e.push(newEnvEntry(name, authId)); });
    const selIdx = cfg.auth.indexOf(selA);
    const writeSel = (fn) => upd((c) => { const a = c.auth[selIdx]; if (!a.f) a.f = {}; fn(a.f); });
    const fieldSrc = selA.type !== 'flow' ? selA : (cfg.auth.find((a) => a.type !== 'flow') || INIT.auth[0]);
    const flowSrc = selA.type === 'flow' ? selA : (cfg.auth.find((a) => a.type === 'flow') || INIT.auth[1]);
    const flowIdx = cfg.auth.indexOf(flowSrc);
    const flowEditor = (model, write, ownerId) => ({
      steps: (model.steps || []).map((st, i) => {
        const ur = checkField('url', st.url, model, 'url');
        return {
          n: i + 1, label: 'Step ' + (i + 1), id: st.id || '', method: st.method, url: st.url, body: st.body || '',
          setId: (e) => { const x = val(e); write((m) => { m.steps[i].id = x; }); },
          setMethod: (e) => { const x = val(e); write((m) => { m.steps[i].method = x; }); },
          setUrl: (e) => { const x = val(e); write((m) => { m.steps[i].url = x; }); },
          urlCls: ur.err ? 'err' : (ur.warn ? 'warnf' : ''), urlHelp: ur.err || ur.warn || ur.help || '', urlHelpCls: ur.err ? 'err' : (ur.warn ? 'warn' : ''),
          setBody: (e) => { const x = val(e); write((m) => { m.steps[i].body = x; }); },
          save: (st.save || []).map((x, j) => ({ k: x[0], v: x[1], aria: 'Step ' + (i + 1) + ' saved value ' + (j + 1),
            setK: (e) => { const y = val(e); write((m) => { m.steps[i].save[j][0] = y; }); }, setV: (e) => { const y = val(e); write((m) => { m.steps[i].save[j][1] = y; }); },
            remove: () => write((m) => { m.steps[i].save.splice(j, 1); }) })),
          addSave: () => write((m) => { m.steps[i].save = (m.steps[i].save || []).concat([['', '${response.body.']]); }),
          remove: () => { if ((model.steps || []).length <= 1) return; write((m) => { m.steps.splice(i, 1); }); },
          rmDisabled: (model.steps || []).length <= 1, rmTitle: (model.steps || []).length <= 1 ? 'A flow needs at least one step' : 'Remove step ' + (i + 1)
        };
      }),
      addStep: () => write((m) => { m.steps = (m.steps || []).concat([{ id: 'step' + ((m.steps || []).length + 1), method: 'POST', url: '${var.identityUrl}/', body: '', save: [['', '${response.body.']] }]); }),
      headers: (model.headers || []).map((h, j) => ({ k: h[0], v: h[1], aria: 'Header ' + (j + 1),
        setK: (e) => { const y = val(e); write((m) => { m.headers[j][0] = y; }); }, setV: (e) => { const y = val(e); write((m) => { m.headers[j][1] = y; }); },
        remove: () => write((m) => { m.headers.splice(j, 1); }) })),
      addHeader: () => write((m) => { m.headers = (m.headers || []).concat([['x-partner-tenant', '${var.partnerTenant}']]); }),
      saved: savedNames(model).length ? 'Saved values become ${var.' + savedNames(model).join('}, ${var.') + '} for later steps and the headers.' : 'Save values from a response to use them in later steps and the headers.'
    });
    const ad = {
      id: selA.id, typeLabel: TYPE_LABEL[selA.type], isFlow: selA.type === 'flow', notFlow: selA.type !== 'flow',
      used: usedOf(selA.id).length ? 'Used by ' + usedOf(selA.id).map((x) => x + '.api.yaml').join(', ') : 'Not used by any api file yet. Add auth: ' + selA.id + ' to an api file or a single api.',
      fields: fieldsFor(fieldSrc, (fn) => { if (selA.type !== 'flow') writeSel(fn); }, addEnvFor),
      hasMsg: !!s.authMsg, msg: s.authMsg, closeMsg: () => set({ authMsg: '' }),
      del: () => {
        const u = usedOf(selA.id);
        if (u.length) { set({ authMsg: selA.id + ' is used by ' + plural(u.length, 'api file') + ' (' + u.join(', ') + '). Point their auth: at another definition first, or rename it everywhere with vc rename ' + selA.id + ' <newId>.' }); return; }
        upd((c) => { c.auth = c.auth.filter((a) => a.id !== selA.id); }, { authSel: '', test: null, authMsg: '' });
      },
      problems: (authProblems[selA.id] || []).length ? plural(authProblems[selA.id].length, 'problem') + ' to fix before saving' : 'No problems',
      problemsCls: (authProblems[selA.id] || []).length ? 'fail' : 'pass'
    };
    const fe = flowEditor(flowSrc, (fn) => upd((c) => { fn(c.auth[flowIdx]); }), flowSrc.id);

    // ---------- auth test (simulated) ----------
    const tprof = profOk(s.tprof) ? s.tprof : (profOk(s.profile) ? s.profile : defProf);
    const red = (x) => String(x).replace(/\$\{secret\.[A-Z0-9_]+\}/g, '••••(redacted)');
    const resolve = (x, prof, saved) => red(String(x || '').replace(/\$\{var\.([A-Za-z0-9_]+)(?:\s*\|[^}]*)?\}/g, (m, k) => (saved && saved[k] != null ? saved[k] : (varVal(k, prof) != null ? varVal(k, prof) : m))));
    const allStrings = (a) => { const out = []; Object.keys(a.f || {}).forEach((k) => out.push(a.f[k])); (a.steps || []).forEach((st) => { out.push(st.url, st.body); (st.save || []).forEach((x) => out.push(x[1])); }); (a.headers || []).forEach((h) => out.push(h[1])); return out; };
    const plan = (a, prof) => {
      const steps = []; let fix = null;
      const lits = (FIELDS[a.type] || []).filter((fd) => fd[2] === 'secret' && (a.f || {})[fd[0]] && !SEC_RE.test(String(a.f[fd[0]]).trim()));
      const names = []; allStrings(a).forEach((x) => refsOf(x, 'secret').forEach((n) => { if (names.indexOf(n) < 0) names.push(n); }));
      if (names.length || lits.length) {
        const lines = []; let bad = null;
        lits.forEach((fd) => { lines.push({ k: fd[0], v: 'literal value in the config · refused', cls: 'fail' }); if (!bad) bad = { kind: 'literal', field: fd[0] }; });
        names.forEach((n) => {
          const e = env.find((x) => x.name === n);
          if (!e) { lines.push({ k: n, v: 'not declared in .env.schema', cls: 'fail' }); if (!bad) bad = { kind: 'undeclared', name: n }; return; }
          const st = stOf(e, prof);
          const where = e.src === 'op' ? '1Password ' + e.ref.replace('$VC_PROFILE', prof) : e.src === 'vault' ? 'Vault ' + e.ref.replace('$VC_PROFILE', prof) : 'environment';
          lines.push({ k: n, v: where + ' · ' + (st === 'ok' ? 'resolved' : st === 'missing' ? 'not set' : 'not checked yet'), cls: st === 'ok' ? '' : 'fail' });
          if (st !== 'ok' && !bad) bad = { kind: st === 'missing' ? 'missing' : 'unchecked', name: n, e };
        });
        steps.push({ title: 'Resolve secrets with Varlock for ' + prof, ok: !bad, ms: 38, lines });
        if (bad) {
          if (bad.kind === 'literal') fix = { title: a.id + '.' + bad.field + ' holds a literal value', text: 'Secret fields only take ${secret.NAME}. Replace the value, add NAME to .env.schema, and store the value in 1Password, Vault or the environment.', steps: ['Use the suggestion under the field, or type ${secret.NAME} yourself.'], add: '' };
          else if (bad.kind === 'undeclared') fix = { title: bad.name + ' is not declared in .env.schema', text: 'vc only resolves secrets that .env.schema declares, so a typo or a new secret fails here, before any request is sent.', steps: ['Add it to .env.schema below (source 1Password by default), then set the value in that item.', 'Run Check secrets, then test again.'], add: bad.name };
          else if (bad.kind === 'missing') {
            const e = bad.e; const ref = e.ref.replace('$VC_PROFILE', prof);
            fix = { title: bad.name + ' is not set for profile ' + prof, text: '.env.schema declares ' + bad.name + (e.src === 'op' ? ' from 1Password (' + ref + '), but that item has no value for ' + prof + '.' : e.src === 'vault' ? ' from Vault (' + ref + '), but the field is empty for ' + prof + '.' : ' from the environment, and it is not set in this shell for ' + prof + '.'),
              steps: e.src === 'op' ? ['Set it in 1Password: op item edit ' + ref.split('/')[3] + ' --vault ' + ref.split('/')[2] + ' ' + ref.split('/').pop() + '=<value>', 'Or point ' + bad.name + ' at another source in .env.schema (Secrets section).', 'Run Check secrets, then test again.'] : e.src === 'vault' ? ['Set it in Vault: vault kv patch ' + ref.split('#')[0] + ' ' + (ref.split('#')[1] || 'value') + '=<value>', 'Run Check secrets, then test again.'] : ['Set it in the shell or CI job for ' + prof + ': export ' + bad.name + '=<value>', 'Run Check secrets, then test again.'], add: '' };
          } else fix = { title: bad.name + ' has not been checked for ' + prof, text: 'It was added to .env.schema but Varlock has not resolved it yet.', steps: ['Set the value in its source, then run Check secrets.'], add: '' };
        }
      }
      const failed = !!fix;
      const hdrs = []; let expiry = '';
      const saved = {};
      const ms = [184, 142, 96, 210];
      if (a.type === 'oauth2ClientCredentials' || a.type === 'oauth2Refresh' || a.type === 'oauth2AuthorizationCode') {
        const f = a.f || {};
        if (a.type === 'oauth2AuthorizationCode') steps.push({ title: 'Use the stored sign-in', ok: true, ms: 4, lines: [{ k: 'Refresh token', v: 'from .vibranium/cache.db, saved after a browser sign-in at ' + resolve(f.authorizeUrl, prof) + ' · 3 days ago', cls: '' }] });
        const grant = a.type === 'oauth2ClientCredentials' ? 'client_credentials' : 'refresh_token';
        steps.push({ title: 'POST ' + resolve(f.tokenUrl, prof), ok: true, ms: ms[0], lines: [
          { k: 'Request', v: 'POST ' + resolve(f.tokenUrl, prof) + ' · content-type: application/x-www-form-urlencoded' + (f.clientAuth === 'header' ? ' · authorization: Basic ••••(redacted)' : ''), cls: '' },
          { k: 'Body', v: 'grant_type=' + grant + (f.clientAuth === 'header' ? '' : '&client_id=' + red(resolve(f.clientId, prof))) + (a.type === 'oauth2ClientCredentials' ? (f.clientAuth === 'header' ? '' : '&client_secret=••••(redacted)') : '&refresh_token=••••(redacted)') + (f.scopes ? '&scope=' + String(f.scopes).trim().split(/\s+/).join('+') : ''), cls: '' },
          { k: 'Response', v: '200 OK · ' + ms[0] + ' ms · application/json', cls: 'ok' },
          { k: 'Saved', v: 'access_token = eyJh…(redacted) · token_type = Bearer · expires_in = 3600', cls: '' }] });
        hdrs.push({ k: 'authorization', v: 'Bearer eyJh…(redacted)' });
        const rb = /^(\d+)m$/.test(f.refreshBefore || '') ? +f.refreshBefore.slice(0, -1) : 1;
        expiry = 'expires in 59 min · refresh ' + (f.refreshBefore || '1m').replace(/m$/, ' min').replace(/s$/, ' s').replace(/h$/, ' h') + ' before' + (rb >= 59 ? ' (refreshes on every run)' : '');
      } else if (a.type === 'flow') {
        (a.steps || []).forEach((st, i) => {
          const body = String(st.body || '').split('\n').filter((x) => x.trim()).map((x) => resolve(x.trim(), prof, saved)).join(', ');
          const sv = (st.save || []).filter((x) => x[0]);
          sv.forEach((x) => { saved[x[0]] = /token/i.test(x[0]) ? 'eyJh…(redacted)' : /expires/i.test(x[0]) ? '1800' : /id$/i.test(x[0]) ? 'ses_4821…(redacted)' : 'saved'; });
          const lines = [{ k: 'Request', v: st.method + ' ' + resolve(st.url, prof, saved), cls: '' }];
          if (body) lines.push({ k: 'Body', v: body, cls: '' });
          lines.push({ k: 'Response', v: '200 OK · ' + ms[i % 4] + ' ms', cls: 'ok' });
          lines.push({ k: 'Saved', v: sv.length ? sv.map((x) => x[0] + ' = ' + saved[x[0]]).join(' · ') : 'nothing', cls: '' });
          steps.push({ title: (st.id || 'step' + (i + 1)) + ' · ' + st.method + ' ' + resolve(st.url, prof, saved), ok: true, ms: ms[i % 4], lines });
        });
        (a.headers || []).filter((h) => h[0]).forEach((h) => hdrs.push({ k: h[0], v: resolve(h[1], prof, saved) }));
        expiry = saved.expiresIn ? 'expires in 29 min (expiresIn 1800 s) · refresh 1 min before' : 'no expiry reported · the flow runs again after a 401';
      } else if (a.type === 'basic') { steps.push({ title: 'Build the Basic header', ok: true, ms: 0, lines: [{ k: 'User name', v: red(resolve((a.f || {}).username, prof)), cls: '' }, { k: 'Password', v: '••••(redacted)', cls: '' }] }); hdrs.push({ k: 'authorization', v: 'Basic ••••(redacted)' }); expiry = 'no expiry · computed for each request'; }
      else if (a.type === 'bearer') { steps.push({ title: 'Read the token', ok: true, ms: 0, lines: [{ k: 'Token', v: '••••(redacted)', cls: '' }] }); hdrs.push({ k: 'authorization', v: 'Bearer ••••(redacted)' }); expiry = 'fixed token · no expiry known to vc'; }
      else if (a.type === 'apiKey') { const f = a.f || {}; steps.push({ title: 'Place the key in the ' + (f.in || 'header'), ok: true, ms: 0, lines: [{ k: f.in === 'query' ? 'Query' : 'Header', v: (f.name || 'x-api-key') + ' = ••••(redacted)', cls: '' }] }); if (f.in === 'query') hdrs.push({ k: 'query', v: '?' + (f.name || 'key') + '=••••(redacted) added to every URL' }); else hdrs.push({ k: f.name || 'x-api-key', v: '••••(redacted)' }); expiry = 'no expiry · sent with every request'; }
      else if (a.type === 'plugin') { const f = a.f || {}; steps.push({ title: 'Plugin ' + (f.plugin || '?') + ' signs a sample request', ok: true, ms: 3, lines: [{ k: 'Request', v: 'GET ' + (varVal('usersUrl', prof) || '') + '/api/v1/users?limit=1', cls: '' }, { k: 'Options', v: String(f.with || '').split('\n').filter((x) => x.trim()).join(', ') || 'none', cls: '' }] }); hdrs.push({ k: 'authorization', v: 'AWS4-HMAC-SHA256 Credential=AKIA…(redacted)/eu-west-1/execute-api, Signature=…(redacted)' }); hdrs.push({ k: 'x-amz-date', v: '20261023T140211Z' }); expiry = 'signed per request · no token to expire'; }
      if (failed) { const first = steps[0]; steps.slice(1).forEach((x) => { x.skipped = true; }); first.ok = false; }
      return { steps, fix, hdrs, expiry, failed };
    };
    const P = plan(selA, tprof);
    const T = s.test && s.test.def === selA.id && s.test.prof === tprof ? s.test : null;
    const nShown = T ? T.shown : 0; const running = !!(T && T.running); const done = !!(T && !T.running);
    const stepsV = P.steps.map((x, i) => {
      let st = 'waiting';
      if (T) { if (i < nShown) st = x.skipped ? 'skipped' : (x.ok ? 'ok' : 'fail'); else if (running && i === nShown) st = 'running'; else if (done) st = 'skipped'; }
      const L = { waiting: ['skip', 'not run'], running: ['accent', 'running…'], ok: ['pass', 'ok'], fail: ['fail', 'failed'], skipped: ['skip', 'skipped'] }[st];
      return { n: i + 1, title: x.title, pill: L[0], stLabel: L[1], ms: st === 'ok' || st === 'fail' ? x.ms + ' ms' : '', showLines: st === 'ok' || st === 'fail', lines: x.lines, cls: 'ts-' + st };
    });
    const runTest = () => {
      clearTimeout(this._tt);
      const n = P.failed ? 1 : P.steps.length;
      set({ test: { def: selA.id, prof: tprof, shown: 0, running: true, n } });
      const tick = () => { const t = (this.state || {}).test; if (!t || !t.running) return; const shown = t.shown + 1; this.setState({ test: Object.assign({}, t, { shown, running: shown < t.n }) }); if (shown < t.n) this._tt = setTimeout(tick, 480); };
      this._tt = setTimeout(tick, 480);
    };
    const tst = {
      profs: PROFS.map((p) => opt(p, p === tprof, () => set({ tprof: p }))),
      prof: tprof, run: runTest, running, runLabel: running ? 'Testing…' : (done ? 'Test again' : 'Test ' + selA.id),
      steps: stepsV, done, ok: done && !P.failed, failed: done && P.failed,
      summary: done ? (P.failed ? selA.id + ' failed on ' + tprof : selA.id + ' works on ' + tprof) : '',
      headers: P.hdrs.length ? P.hdrs : [{ k: 'authorization', v: '…' }], expiry: P.expiry,
      fixTitle: P.fix ? P.fix.title : '', fixText: P.fix ? P.fix.text : '', fixSteps: (P.fix ? P.fix.steps : ['']).map((t) => { const m = t.match(/^(.*?: )((?:op|vault|export) .*)$/); return m ? { t: m[1], code: m[2] } : { t, code: '' }; }),
      hasFixAdd: !!(P.fix && P.fix.add), fixAddLabel: P.fix && P.fix.add ? 'Add ' + P.fix.add + ' to .env.schema' : '', fixAdd: () => { if (P.fix && P.fix.add) addEnvFor(P.fix.add, selA.id); },
      goSecrets: go('secrets'),
      note: dirty ? 'Tests the definition as edited here, unsaved changes included. Nothing is written.' : 'Runs the definition from vibranium.config.yaml against ' + tprof + '. Nothing is written; tokens stay in memory.'
    };

    // ---------- new auth wizard ----------
    const wz0 = Object.assign({}, W0, s.wiz || {});
    const setW = (o) => set({ wiz: Object.assign({}, wz0, o) });
    const camel = (str) => { const w = String(str).replace(/([a-z0-9])([A-Z])/g, '$1 $2').toLowerCase().replace(/[^a-z0-9]+/g, ' ').trim().split(/\s+/).filter(Boolean).filter((x) => ['a', 'an', 'the', 'for', 'of'].indexOf(x) < 0); return w.map((x, i) => (i ? x[0].toUpperCase() + x.slice(1) : x)).join('').replace(/^[0-9]+/, ''); };
    const wid = wz0.idEdited ? wz0.id : camel(wz0.name);
    let widErr = '';
    if (!wid) widErr = 'Type a name; the id is derived from it.';
    else if (!/^[a-z][a-zA-Z0-9]*$/.test(wid)) widErr = 'Auth ids are camelCase nouns, like shopOauth or partnerLogin.';
    else if (cfg.auth.some((a) => a.id === wid)) widErr = wid + ' already exists.';
    else if (/^(auth|login)$/i.test(wid)) widErr = 'Too generic: name what it signs in to, like adminLogin or billingApiKey.';
    const widSugg = wid && widErr && !cfg.auth.some((a) => a.id === camel(wid)) && /^[a-z]/.test(camel(wid)) && camel(wid) !== wid ? camel(wid) : '';
    const fDefaults = {}; (FIELDS[wz0.type] || []).forEach((fd) => { fDefaults[fd[0]] = FDEF[fd[0]] != null ? FDEF[fd[0]] : (fd[0] === 'plugin' && cfg.plugins[0] ? cfg.plugins[0].id : ''); });
    const wf = Object.assign({}, fDefaults, (wz0.f || {})[wz0.type] || {});
    const wModel = { id: wid || 'newAuth', type: wz0.type, f: wf, steps: wz0.type === 'flow' ? wz0.steps : [], headers: wz0.type === 'flow' ? wz0.headers : [] };
    const writeW = (fn) => { const all = Object.assign({}, wz0.f || {}); const cur = Object.assign({}, wf); fn(cur); all[wz0.type] = cur; setW({ f: all }); };
    const wFields = fieldsFor(wModel.type === 'flow' ? Object.assign({}, INIT.auth[0], { id: wid || 'newAuth' }) : wModel, (fn) => { if (wModel.type !== 'flow') writeW(fn); }, null);
    const wErrs = wModel.type === 'flow' ? wz0.steps.map((st) => checkField('url', st.url, wModel, 'url').err).filter(Boolean).concat(wz0.headers.some((h) => h[0]) ? [] : ['Add at least one header to apply.']) : wFields.filter((x) => x.helpCls === 'err').map((x) => x.label + ': ' + x.help);
    const wNewSecrets = []; allStrings(wModel).forEach((x) => refsOf(x, 'secret').forEach((n) => { if (envNames.indexOf(n) < 0 && wNewSecrets.indexOf(n) < 0) wNewSecrets.push(n); }));
    const step2ok = !widErr && wErrs.length === 0;
    const wfe = flowEditor(wModel.type === 'flow' ? wModel : { id: 'x', type: 'flow', steps: W0.steps, headers: W0.headers }, (fn) => { const m = { steps: clone(wz0.steps), headers: clone(wz0.headers) }; fn(m); setW({ steps: m.steps, headers: m.headers }); }, wid);
    const wz = {
      step1: wz0.step === 1, step2: wz0.step === 2, step3: wz0.step === 3,
      stepLabel: 'Step ' + wz0.step + ' of 3 · ' + ['Pick a type', 'Fill in the details', 'Review and create'][wz0.step - 1],
      steps: ['Type', 'Details', 'Review'].map((l, i) => ({ label: (i + 1) + ' ' + l, cls: wz0.step === i + 1 ? 'on' : (wz0.step > i + 1 ? 'done' : '') })),
      types: TYPES.map((t) => ({ key: t[0], label: t[1], desc: t[2], on: wz0.type === t[0], cls: wz0.type === t[0] ? 'on' : '', pick: () => setW({ type: t[0] }) })),
      typeLabel: TYPE_LABEL[wz0.type], isFlow: wz0.type === 'flow', notFlow: wz0.type !== 'flow',
      name: wz0.name, setName: (e) => setW({ name: val(e) }),
      id: wid, setId: (e) => setW({ id: val(e), idEdited: true }), idEdited: !!wz0.idEdited, resetId: () => setW({ idEdited: false, id: '' }),
      idCls: widErr && wz0.name ? 'err' : '', idHelp: widErr || (wz0.idEdited ? 'Edited by hand; still camelCase.' : 'Derived from the name. Api files use it as auth: ' + wid), idHelpCls: widErr && wz0.name ? 'err' : '',
      hasIdSugg: !!widSugg, idSugg: widSugg, useIdSugg: () => setW({ id: widSugg, idEdited: true }),
      fields: wFields,
      yaml: codeOf(authLines(wModel, '  '), lineOf('secrets') - 1).map((l) => Object.assign(l, { no: '+' })),
      hasNewSecrets: wNewSecrets.length > 0, newSecrets: wNewSecrets.length ? 'Also adds ' + wNewSecrets.join(', ') + ' to .env.schema with a 1Password source. Set the value there, then use Test.' : '',
      errs: wErrs.length ? wErrs[0] : '', hasErrs: wErrs.length > 0 && wz0.step === 2,
      hasBack: wz0.step > 1, back: () => setW({ step: Math.max(1, wz0.step - 1) }),
      notLast: wz0.step < 3, isLast: wz0.step === 3,
      next: () => { if (wz0.step === 2 && !step2ok) return; setW({ step: Math.min(3, wz0.step + 1) }); },
      nextDisabled: wz0.step === 2 && !step2ok, nextTitle: wz0.step === 2 && !step2ok ? (widErr || wErrs[0] || '') : 'Continue',
      create: () => {
        if (!step2ok) return;
        const c = clone(cfg); const m = clone(wModel); m.id = wid;
        if (m.type !== 'flow') { const f2 = {}; (FIELDS[m.type] || []).forEach((fd) => { if (m.f[fd[0]] !== '' && m.f[fd[0]] != null) f2[fd[0]] = m.f[fd[0]]; }); m.f = f2; m.steps = []; m.headers = []; } else m.f = {};
        c.auth.push(m);
        const e2 = clone(env); wNewSecrets.forEach((n) => e2.push(newEnvEntry(n, wid)));
        set(Object.assign({}, keep, { cfg: c, env: e2, dlg: '', wiz: null, authSel: wid, sec: 'auth', test: null, toast: '' }));
      },
      wfePlaceholder: ''
    };

    // ---------- secrets ----------
    const runCheck = () => {
      clearTimeout(this._ct);
      set({ chk: { running: true, at: s.chk.at, note: '' } });
      this._ct = setTimeout(() => {
        const st0 = this.state || {}; const e0 = st0.env || env; const eb = st0.envBase || envBase;
        const fixSt = (list) => list.map((e) => { const o = clone(e); PROFS.forEach((p) => { if (!o.st[p] || o.st[p] === 'unchecked') o.st[p] = 'missing'; }); return o; });
        const e1 = fixSt(e0);
        const miss = []; e1.forEach((e) => PROFS.forEach((p) => { if (e.st[p] === 'missing') miss.push(e.name + ' (' + p + ')'); }));
        this.setState({ env: e1, envBase: fixSt(eb), base: st0.base || base, cfg: st0.cfg || cfg, chk: { running: false, at: '14:06', note: 'Checked ' + plural(e1.length, 'secret') + ' × ' + plural(PROFS.length, 'profile') + ' at 14:06 · ' + (miss.length ? miss.length + ' not set: ' + miss.join(', ') : 'all resolved') } });
      }, 1300);
    };
    const ns = Object.assign({}, D.ns, s.ns || {});
    const setNs = (o) => set({ ns: Object.assign({}, ns, o) });
    const nsName = ns.name.trim();
    const nsErr = !nsName ? '' : (!/^[A-Z][A-Z0-9_]*$/.test(nsName) ? 'UPPER_SNAKE_CASE, like BILLING_API_KEY.' : (envNames.indexOf(nsName) >= 0 ? nsName + ' is already declared.' : ''));
    const nsRefPh = ns.src === 'op' ? 'op://shop-$VC_PROFILE/billing/api_key' : ns.src === 'vault' ? 'secret/shop/$VC_PROFILE/billing#api_key' : 'where it is set, e.g. CI secret';
    const nsValid = !!nsName && !nsErr && !!ns.desc.trim() && (ns.src === 'env' || !!ns.ref.trim());
    const nsEntry = { name: nsName || 'NAME', desc: ns.desc.trim() || 'description', src: ns.src, ref: ns.ref.trim() || nsRefPh, st: {} };
    const usedBySecret = (n) => { const out = []; cfg.auth.forEach((a) => { if (allStrings(a).some((x) => refsOf(x, 'secret').indexOf(n) >= 0)) out.push('auth ' + a.id); }); cfg.alerts.channels.forEach((ch) => { if (refsOf(ch.target, 'secret').indexOf(n) >= 0) out.push('alert ' + ch.id); }); cfg.plugins.forEach((p) => { if (p.sec.indexOf(n) >= 0) out.push('plugin ' + p.id.split('/').pop()); }); return out; };
    const sx = {
      cols: 'minmax(220px, 1.6fr) minmax(200px, 1.4fr) repeat(' + PROFS.length + ', minmax(96px, 0.6fr))',
      head: PROFS.map((p) => ({ label: p })),
      checking: !!s.chk.running, check: runCheck, checkLabel: s.chk.running ? 'Checking…' : 'Check secrets',
      last: s.chk.note || 'Last checked today at ' + s.chk.at + ' by vc ui on start. Values are never shown, only whether they resolve.',
      rows: env.map((e, i) => ({
        name: e.name, desc: e.desc, used: usedBySecret(e.name).length ? 'used by ' + usedBySecret(e.name).join(', ') : 'not referenced yet',
        srcLabel: e.src === 'op' ? '1Password' : e.src === 'vault' ? 'Vault' : 'Environment', ref: e.ref, isNew: !envBase.some((x) => x.name === e.name),
        st: PROFS.map((p) => { const v = s.chk.running ? 'checking' : stOf(e, p); return { p, label: v === 'ok' ? 'resolved' : v === 'missing' ? 'not set' : v === 'checking' ? 'checking' : 'not checked', cls: v === 'ok' ? 'pass' : v === 'missing' ? 'fail' : v === 'checking' ? 'accent' : 'skip', aria: e.name + ' on ' + p + ': ' + (v === 'ok' ? 'resolved' : v === 'missing' ? 'not set' : 'not checked') }; }),
        remove: () => updEnv((x) => { x.splice(i, 1); }), canRemove: !usedBySecret(e.name).length && !envBase.some((x) => x.name === e.name), rmTitle: 'Remove ' + e.name
      })),
      addOpen: !!s.nsOpen, toggleAdd: () => set({ nsOpen: !s.nsOpen }), addLabel: s.nsOpen ? 'Close' : 'Add secret',
      ns: {
        name: ns.name, setName: (e) => setNs({ name: val(e) }), cls: nsErr ? 'err' : '', help: nsErr || 'UPPER_SNAKE_CASE. Reference it as ${secret.' + (nsName || 'NAME') + '}.', helpCls: nsErr ? 'err' : '',
        desc: ns.desc, setDesc: (e) => setNs({ desc: val(e) }),
        srcs: [['op', '1Password'], ['vault', 'Vault'], ['env', 'Environment']].map(([k, l]) => opt(l, ns.src === k, () => setNs({ src: k }))),
        ref: ns.ref, setRef: (e) => setNs({ ref: val(e) }), refPh: nsRefPh, refLabel: ns.src === 'op' ? '1Password reference' : ns.src === 'vault' ? 'Vault path and field' : 'Where it is set (note only)',
        refHelp: ns.src === 'env' ? 'Read from the environment of the shell or CI job.' : '$VC_PROFILE is replaced by the profile name, so one line serves dev, staging and prod.',
        preview: nsEntry.name + '=' + srcExpr(nsEntry), valid: nsValid, invalid: !nsValid,
        add: () => { if (!nsValid) return; updEnv((x) => { x.push(clone(nsEntry)); }, { ns: D.ns, nsOpen: false }); }
      }
    };

    // ---------- reports ----------
    const wr = (fn) => upd((c) => fn(c.reports));
    const rp = {
      formats: [['ctrf', 'CTRF JSON', 'Common Test Report Format; dashboards and CI annotations read it.'], ['junit', 'JUnit XML', 'For CI systems that only read JUnit.'], ['html', 'HTML report', 'The browsable report with requests, responses and diffs.']].map(([k, l, d]) => ({ key: k, label: l, desc: d, on: !!R[k], toggle: () => wr((r) => { r[k] = !r[k]; }) })),
      formatHelp: !R.ctrf && !R.junit && !R.html ? 'Pick at least one format.' : 'Written to .vibranium/reports/<run>/ after every run.', formatHelpCls: !R.ctrf && !R.junit && !R.html ? 'err' : '',
      htmlOff: !R.html, layoutNote: R.html ? '' : 'Turn on the HTML report to choose a layout.',
      layouts: [['folder', 'Folder with a page per test', 'report/index.html plus one page per test; fast to open for 400+ tests.'], ['single', 'Single file', 'report-482.html with everything inlined; easy to attach to a ticket.']].map(([k, l, d]) => ({ label: l, desc: d, on: R.layout === k, cls: R.layout === k ? 'on' : '', dis: !R.html, pick: () => { if (R.html) wr((r) => { r.layout = k; }); } })),
      bodies: [['failures', 'Failures only'], ['all', 'Every request'], ['none', 'None']].map(([k, l]) => opt(l, R.bodies === k, () => wr((r) => { r.bodies = k; }))),
      bodiesDesc: R.bodies === 'failures' ? 'Request and response bodies are kept for failed, blocked and flaky tests. Passing tests keep status and timing only.' : R.bodies === 'all' ? 'Every body is kept. Reports and history grow quickly; redact rules still apply.' : 'No bodies are kept, only status, headers and timing. Failures are harder to debug.',
      maxBody: { v: R.maxBody, set: (e) => { const x = val(e); wr((r) => { r.maxBody = x; }); }, dis: R.bodies === 'none', cls: R.bodies !== 'none' && !/^\d+(kb|mb)$/.test(String(R.maxBody).trim()) ? 'err' : '', help: R.bodies === 'none' ? 'Not used while bodies is none.' : (/^\d+(kb|mb)$/.test(String(R.maxBody).trim()) ? 'Larger bodies are cut and marked as truncated.' : 'Use a size like 64kb or 1mb.'), helpCls: R.bodies !== 'none' && !/^\d+(kb|mb)$/.test(String(R.maxBody).trim()) ? 'err' : '' },
      runs: { v: R.runs, set: (e) => { const x = val(e); wr((r) => { r.runs = x; }); }, cls: intIn(R.runs, 1, 100000) ? '' : 'err', help: intIn(R.runs, 1, 100000) ? 'Newest runs kept in .vibranium/history.db.' : 'A whole number.', helpCls: intIn(R.runs, 1, 100000) ? '' : 'err' },
      days: { v: R.days, set: (e) => { const x = val(e); wr((r) => { r.days = x; }); }, cls: intIn(R.days, 1, 3650) ? '' : 'err', help: intIn(R.days, 1, 3650) ? 'Older runs and their report folders are pruned.' : 'A whole number of days.', helpCls: intIn(R.days, 1, 3650) ? '' : 'err' },
      retention: 'Whichever limit is reached first prunes history and reports. Today: 482 runs over 61 days, 214 MB.'
    };

    // ---------- alerts ----------
    const A = cfg.alerts;
    const nc = Object.assign({}, D.nc, s.nc || {});
    const setNc = (o) => set({ nc: Object.assign({}, nc, o) });
    const ncId = nc.id.trim();
    const ncIdErr = !ncId ? '' : (!/^[a-z][a-zA-Z0-9]*$/.test(ncId) ? 'camelCase, like releaseChannel.' : (A.channels.some((c) => c.id === ncId) ? ncId + ' already exists.' : ''));
    const ncT = nc.target.trim();
    const ncTErr = !ncT ? '' : (nc.type === 'slack' ? (SEC_RE.test(ncT) ? '' : 'A Slack webhook is a secret: use ${secret.NAME}.') : nc.type === 'webhook' ? (/^(https:\/\/|\$\{(var|secret)\.)/.test(ncT) ? '' : 'Use an https:// URL or ${var.…}.') : (/^[^@\s]+@[^@\s]+\.[a-z]+$/.test(ncT) ? '' : 'An email address.'));
    const ncValid = !!ncId && !ncIdErr && !!ncT && !ncTErr;
    const aprof = profOk(s.alert.prof) ? s.alert.prof : (profOk(s.profile) ? s.profile : defProf);
    const sendRes = (ch) => {
      const sec = refsOf(ch.target, 'secret')[0];
      const e = sec ? env.find((x) => x.name === sec) : null;
      if (sec && (!e || stOf(e, aprof) !== 'ok')) return { st: 'failed', cls: 'fail', msg: sec + (e ? ' is not set for profile ' + aprof + '. Set it in ' + (e.src === 'env' ? 'the environment' : e.src === 'op' ? '1Password' : 'Vault') + ', or send the test from another profile.' : ' is not declared in .env.schema.') };
      return ch.type === 'slack' ? { st: 'sent', cls: 'pass', msg: 'Posted to ' + (ch.note || 'the webhook channel') + ' · 200 · 312 ms' } : ch.type === 'webhook' ? { st: 'sent', cls: 'pass', msg: 'POST ' + ch.target + ' · 202 · 148 ms' } : { st: 'sent', cls: 'pass', msg: 'Accepted for ' + ch.target + ' · 250 OK' };
    };
    const al = {
      channels: A.channels.map((ch, i) => ({ id: ch.id, type: ch.type, target: ch.target, note: ch.note || '', hasNote: !!ch.note, remove: () => upd((c) => { c.alerts.channels.splice(i, 1); }), rmAria: 'Remove channel ' + ch.id,
        st: s.alert.state === 'done' ? sendRes(ch).st : (s.alert.state === 'sending' ? 'sending' : 'not sent'), stCls: s.alert.state === 'done' ? sendRes(ch).cls : (s.alert.state === 'sending' ? 'accent' : 'skip'), msg: s.alert.state === 'done' ? sendRes(ch).msg : '', hasMsg: s.alert.state === 'done' })),
      noChannels: A.channels.length === 0,
      addOpen: !!s.ncOpen, toggleAdd: () => set({ ncOpen: !s.ncOpen }), addLabel: s.ncOpen ? 'Close' : 'Add channel',
      nc: {
        types: [['slack', 'Slack'], ['webhook', 'Webhook'], ['email', 'Email']].map(([k, l]) => opt(l, nc.type === k, () => setNc({ type: k }))),
        id: nc.id, setId: (e) => setNc({ id: val(e) }), idCls: ncIdErr ? 'err' : '', idHelp: ncIdErr || 'camelCase, used in logs: releaseChannel', idHelpCls: ncIdErr ? 'err' : '',
        target: nc.target, setTarget: (e) => setNc({ target: val(e) }), targetLabel: nc.type === 'slack' ? 'Webhook (a secret reference)' : nc.type === 'webhook' ? 'URL' : 'Send to',
        targetPh: nc.type === 'slack' ? '${secret.SLACK_RELEASE_WEBHOOK}' : nc.type === 'webhook' ? 'https://hooks.shop.example/qa' : 'qa@shop.example',
        targetCls: ncTErr ? 'err' : '', targetHelp: ncTErr || (nc.type === 'slack' ? 'Declare it in .env.schema; the URL itself never goes in the config.' : nc.type === 'webhook' ? 'Receives the CTRF summary as JSON.' : 'Plain-text summary with a link to the report.'), targetHelpCls: ncTErr ? 'err' : '',
        isSlack: nc.type === 'slack', note: nc.note, setNote: (e) => setNc({ note: val(e) }),
        valid: ncValid, invalid: !ncValid,
        add: () => { if (!ncValid) return; upd((c) => { c.alerts.channels.push({ id: ncId, type: nc.type, target: ncT, note: nc.type === 'slack' ? nc.note.trim() : '' }); }, { nc: D.nc, ncOpen: false }); }
      },
      triggers: [['failed', 'A run has failed tests', 'Once per run, with the failing ids and the first error.'], ['blocked', 'Tests were blocked', 'A lifecycle action such as beforeRun failed and tests could not run.'], ['flaky', 'A test passed only on retry', 'At most once a day per test.'], ['recovered', 'Back to green', 'The first passing run after a failing one.']].map(([k, l, d]) => ({ key: k, label: l, desc: d, on: !!A.on[k], toggle: () => upd((c) => { c.alerts.on[k] = !c.alerts.on[k]; }) })),
      profiles: PROFS.map((p) => ({ label: p, on: A.profiles.indexOf(p) >= 0, toggle: () => upd((c) => { const i = c.alerts.profiles.indexOf(p); if (i >= 0) c.alerts.profiles.splice(i, 1); else c.alerts.profiles.push(p); }) })),
      profilesNote: A.profiles.length ? 'Runs on ' + A.profiles.filter(profOk).join(' and ') + ' can alert. Runs on other profiles never do.' : 'No profile is selected, so nothing ever alerts.',
      sendProfs: PROFS.map((p) => opt(p, p === aprof, () => set({ alert: { state: 'idle', prof: p } }))),
      sending: s.alert.state === 'sending', notSending: s.alert.state !== 'sending', sendLabel: s.alert.state === 'sending' ? 'Sending…' : 'Send test alert',
      sendDisabled: s.alert.state === 'sending' || A.channels.length === 0,
      send: () => { if (!A.channels.length) return; clearTimeout(this._at); set({ alert: { state: 'sending', prof: aprof } }); this._at = setTimeout(() => { const st0 = this.state || {}; this.setState({ alert: { state: 'done', prof: (st0.alert || {}).prof || aprof } }); }, 1100); },
      sendNote: 'Sends a sample “run #482 failed on ' + aprof + '” message to every channel, using ' + aprof + '’s secrets. Nothing is written.'
    };

    // ---------- agents ----------
    const agp = profOk(s.agentProf) ? s.agentProf : (profOk(s.profile) ? s.profile : defProf);
    const agi = PROFS.indexOf(agp); const AG = cfg.profiles[agi].agents;
    const wa = (fn) => upd((c) => fn(c.profiles[agi].agents));
    const modeOf = (m) => (AG.call.indexOf(m) >= 0 ? 'call' : AG.approve.indexOf(m) >= 0 ? 'ask' : 'deny');
    const setMode = (m, mode) => wa((ag) => { ag.call = ag.call.filter((x) => x !== m); ag.approve = ag.approve.filter((x) => x !== m); if (mode === 'call') ag.call = METHODS.filter((x) => x === m || ag.call.indexOf(x) >= 0); if (mode === 'ask') ag.approve = METHODS.filter((x) => x === m || ag.approve.indexOf(x) >= 0); });
    const listW = (arr) => (arr.length === 0 ? 'none' : arr.length === 1 ? arr[0] : arr.slice(0, -1).join(', ') + ' and ' + arr[arr.length - 1]);
    const deny = METHODS.filter((m) => modeOf(m) === 'deny');
    const ag = {
      profs: PROFS.map((p) => opt(p, p === agp, () => set({ agentProf: p }))), prof: agp,
      run: !!AG.run, toggleRun: () => wa((x) => { x.run = !x.run; }),
      runHelp: AG.run ? 'Agents may start test runs on ' + agp + ' (the run_tests tool). Runs show up in Runs and history, marked with the agent’s name.' : 'Agents cannot start runs on ' + agp + '. They can still read definitions and past results.',
      methods: METHODS.map((m) => ({ m, opts: [['call', 'Without asking'], ['ask', 'Ask a person'], ['deny', 'Not allowed']].map(([k, l]) => opt(l, modeOf(m) === k, () => setMode(m, k))), mcls: 'method m-' + m.toLowerCase() })),
      writes: [['no', 'No'], ['ask', 'Ask'], ['yes', 'Yes']].map(([k, l]) => opt(l, AG.write === k, () => wa((x) => { x.write = k; }))),
      writeHelp: AG.write === 'no' ? 'Agents can read .api.yaml and .test.yaml files but cannot change them.' : AG.write === 'ask' ? 'Each change to a definition waits for approval, shown as a diff on the AI agents page.' : 'Agents write definitions directly. You review the change in git like any other commit.',
      summary: 'On ' + agp + ', an agent ' + (AG.run ? 'can run tests' : 'cannot run tests') + ', calls ' + listW(AG.call) + ' on its own' + (AG.approve.length ? ', must ask a person before ' + listW(AG.approve) : '') + (deny.length ? ', is refused ' + listW(deny) : '') + ', and ' + (AG.write === 'yes' ? 'may edit definitions directly' : AG.write === 'ask' ? 'asks before editing a definition' : 'cannot edit definitions') + '.',
      yaml: (() => { const pl0 = profLines(cfg.profiles[agi], '  '); const k = pl0.findIndex((l) => /agents:$/.test(l)); const at = curLines.indexOf('  - id: ' + agp); return codeOf(pl0.slice(k), at + 1 + k); })()
    };

    // ---------- plugins ----------
    const pl = {
      rows: cfg.plugins.map((p, i) => ({
        id: p.id, version: p.version, what: p.what,
        groups: [['Network', p.net, 'no network access'], ['Secrets', p.sec, 'no secrets'], ['Files', p.files, 'no file access']].map(([l, arr, none]) => ({ label: l, items: arr.map((t) => ({ t })), none: arr.length === 0, noneLabel: none })),
        remove: () => {
          const users = cfg.auth.filter((a) => a.type === 'plugin' && (a.f || {}).plugin === p.id).map((a) => a.id);
          if (users.length) { set({ plMsg: p.id + ' is used by auth ' + users.join(', ') + '. Delete or change that definition first.' }); return; }
          upd((c) => { c.plugins.splice(i, 1); }, { plMsg: 'Removed ' + p.id + ' from plugins. The package stays in package.json until you run npm uninstall ' + p.id + '.' });
        }, rmAria: 'Remove plugin ' + p.id
      })),
      noPlugins: cfg.plugins.length === 0, hasMsg: !!s.plMsg, msg: s.plMsg, closeMsg: () => set({ plMsg: '' })
    };

    // ---------- diff dialog ----------
    const dfile = s.diffFile === 'env' ? 'env' : 'cfg';
    const ops = dfile === 'env' ? envOps : cfgOps;
    const H = hunksOf(ops);
    const dlines = [];
    if (H.length) H.forEach((h) => { dlines.push({ cls: 'hunk', a: '', b: '', sign: '', segs: [{ c: '', t: h.head }] }); h.ops.forEach((o) => dlines.push({ cls: o.t === '+' ? 'add' : o.t === '-' ? 'del' : 'keep', a: o.a || '', b: o.b || '', sign: o.t === ' ' ? '' : o.t, segs: tok(o.x) })); });
    else (dfile === 'env' ? envCurLines : curLines).forEach((x, i) => dlines.push({ cls: 'keep', a: i + 1, b: i + 1, sign: '', segs: tok(x) }));
    const adds = ops.filter((o) => o.t === '+').length, dels = ops.filter((o) => o.t === '-').length;
    const df = {
      tabs: [['cfg', 'vibranium.config.yaml', nCfg], ['env', '.env.schema', nEnv]].map(([k, l, n]) => opt(l, dfile === k, () => set({ diffFile: k }), { count: n ? plural(n, 'change') : 'no changes' })),
      lines: dlines, summary: H.length ? plural(H.length, 'change') + ' · +' + adds + ' −' + dels + ' lines' : 'No changes in this file. Showing it as it is on disk.'
    };

    const problemText = problems.length ? (problems.length === 1 ? '1 problem to fix before saving: ' : problems.length + ' problems to fix before saving. First: ') + problems[0].text : '';
    const pendLabel = [nCfg ? plural(nCfg, 'change') + ' to vibranium.config.yaml' : '', nEnv ? plural(nEnv, 'change') + ' to .env.schema' : ''].filter(Boolean).join(' · ');
    return {
      sideCollapsed: !!s.sideCollapsed, appCls: s.sideCollapsed ? 'side-collapsed' : '', sideOpen: !s.sideCollapsed, sideLabel: s.sideCollapsed ? 'Expand sidebar' : 'Collapse sidebar', sideChev: s.sideCollapsed ? 'M9 6l2 2-2 2' : 'M11 6 9 8l2 2', toggleSide: () => set({ sideCollapsed: !s.sideCollapsed }),
      profile: profOk(s.profile) ? s.profile : 'staging', setProfile: (e) => set({ profile: val(e), test: null }),
      secs, cfgCount: nCfg ? String(nCfg) : '', envCount: nEnv ? String(nEnv) : '', cfgCountTitle: nCfg ? plural(nCfg, 'unsaved change') : 'No unsaved changes', envCountTitle: nEnv ? plural(nEnv, 'unsaved change') : 'No unsaved changes',
      dirty, pendLabel, hasProblems: problems.length > 0, problemText, goProblem: () => set({ sec: problems.length ? problems[0].sec : sec }),
      saveDisabled: problems.length > 0, saveTitle: problems.length ? 'Fix the problems first' : 'Write the changes to disk',
      openDiff: () => set({ dlg: 'diff', diffFile: nCfg || !nEnv ? 'cfg' : 'env' }),
      discard: () => set({ cfg: base, env: envBase, test: null, toast: 'Discarded. vibranium.config.yaml and .env.schema are unchanged.' }),
      save: () => { if (problems.length) return; set({ base: cfg, envBase: env, cfg, env, dlg: '', toast: 'Saved ' + [nCfg ? 'vibranium.config.yaml' : '', nEnv ? '.env.schema' : ''].filter(Boolean).join(' and ') + ' · vc check found no problems. Commit when ready: git diff shows the change.' }); },
      hasToast: !!s.toast, toast: s.toast, dismissToast: () => set({ toast: '' }),
      isGeneral: sec === 'general', isProfiles: sec === 'profiles', isAuth: sec === 'auth', isSecrets: sec === 'secrets', isReports: sec === 'reports', isAlerts: sec === 'alerts', isAgents: sec === 'agents', isPlugins: sec === 'plugins',
      locs, g, pr, mx, np: npV, dp: dpV, au, ad, fe, tst, wz, wfe, sx, rp, al, ag, pl, df,
      dlgNewProfile: s.dlg === 'newProfile', dlgDelProfile: s.dlg === 'delProfile', dlgWizard: s.dlg === 'wizard', dlgDiff: s.dlg === 'diff',
      closeDlg: () => set({ dlg: '' })
    };
  }
}
