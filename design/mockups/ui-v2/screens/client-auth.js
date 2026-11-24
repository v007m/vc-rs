/* Client · auth registry (#client-auth) and the request card's Auth tab (VC.authTab).
   Secrets are never shown: only their name and where they come from. */
(function () {
  'use strict';
  const VC = window.VC, D = VC.data, S = VC.state, A = VC.actions;
  const { esc, ic, chipSrc, hlYaml } = VC.h;
  Object.assign(S, { authType: 'oauth2ClientCredentials', authTested: false });

  const PLACEHOLDER = {
    username: 'ada', password: null, token: null, in: 'header', name: 'x-api-key', value: null,
    tokenUrl: '${baseUrl}/oauth/token', clientId: 'shop-tests', clientSecret: null, scopes: 'users:read', audience: '',
    authorizeUrl: '${baseUrl}/oauth/authorize', pkce: 'S256', redirectPort: '7799', refreshToken: null,
    steps: 'users#login', apply: 'headers.authorization: Bearer ${token}', expiresIn: '${body.expires_in}', refreshBefore: '60s', cache: 'profile',
    region: 'eu-west-1', service: 'execute-api', accessKeyId: null, secretAccessKey: null, sessionToken: null, clientCert: 'certs/shop-tests.pem'
  };
  const SECRET_NAME = { password: 'SHOP_PASSWORD', token: 'API_TOKEN', value: 'API_KEY', clientSecret: 'OAUTH_CLIENT_SECRET', refreshToken: 'REFRESH_TOKEN', accessKeyId: 'AWS_ACCESS_KEY_ID', secretAccessKey: 'AWS_SECRET_ACCESS_KEY', sessionToken: 'AWS_SESSION_TOKEN' };
  const WIDE = new Set(['scopes', 'steps', 'apply']);
  function rows(type) {
    const t = D.auth.find((x) => x.id === type);
    if (D.authConfig[type]) return D.authConfig[type];
    return t.keys.map((k) => (PLACEHOLDER[k] === null ? { k, secret: SECRET_NAME[k], src: 'keychain' } : { k, v: PLACEHOLDER[k] }));
  }
  function fieldHtml(r, i) {
    const id = `af-${i}`;
    if (r.secret) {
      const where = r.src === 'env' ? 'environment variable' : 'macOS keychain';
      return `<div class="field"><label for="${id}">${esc(r.k)}</label>
        <div class="secret-row"><div class="secret-field" title="Stored in the ${where}">${ic('lock', 'ico-sm')}<span class="grow" id="${id}">\${secret.${esc(r.secret)}}</span></div><button class="btn btn-sm" data-action="toast" data-msg="Secret updated" data-sub="Stored in the ${r.src === 'env' ? 'shell environment' : 'keychain'}; the file keeps only \${secret.${esc(r.secret)}}">Change</button></div></div>`;
    }
    return `<div class="field${WIDE.has(r.k) ? ' field-wide' : ''}"><label for="${id}">${esc(r.k)}</label><input class="input mono" id="${id}" value="${esc(r.v)}" data-fk="${id}"></div>`;
  }
  function yaml(type) {
    const name = type === 'oauth2ClientCredentials' ? 'shopAuth' : type === 'awsSigV4' ? 'gatewayAuth' : type + 'Auth';
    return `auth:\n  ${name}:\n    type: ${type}\n` + rows(type).map((r) => `    ${r.k}: ${r.secret ? '${secret.' + r.secret + '}' : /[:{}#]|^\$/.test(r.v) ? JSON.stringify(r.v) : r.v}`).join('\n');
  }
  function testResult() {
    if (!S.authTested) return '';
    if (S.authType === 'awsSigV4') {
      return `<section class="layer" aria-label="Test result"><div class="layer-head"><h3 class="section-label">${ic('pass', 'ico-sm')}Signed a test request</h3><span class="subtle">no network call</span></div>
        <pre class="code">${esc('GET /prod/users HTTP/1.1\nhost: abc123.execute-api.eu-west-1.amazonaws.com\nx-amz-date: 20261123T140612Z\nx-amz-content-sha256: e3b0c442…b855\nx-amz-security-token: ')}<span class="redacted">••••</span>\n${esc('authorization: AWS4-HMAC-SHA256 Credential=')}<span class="redacted">••••</span>${esc('/20261123/eu-west-1/execute-api/aws4_request, SignedHeaders=host;x-amz-content-sha256;x-amz-date;x-amz-security-token, Signature=')}<span class="redacted">••••</span></pre></section>`;
    }
    return `<section class="layer" aria-label="Test result"><div class="layer-head"><h3 class="section-label">${ic('pass', 'ico-sm')}Token issued · 38 ms</h3><span class="subtle" title="POST ${esc(PLACEHOLDER.tokenUrl)} → 200">cached for 59 min</span></div>
      <details class="disc"><summary>${ic('chev-down', 'chev')}Token claims<span class="meta">decoded locally</span></summary><div class="disc-body">
      <div class="kv is-readonly" role="table" aria-label="Token claims">${D.claims.map(([k, v]) => `<div class="kv-row" role="row"><span class="k mono" role="cell">${esc(k)}</span><span class="v" role="cell"><span>${v === '••••' ? '<span class="redacted">••••</span>' : esc(v)}</span></span><span class="s" role="cell">${k === 'scope' ? '<span class="chip">catalog:cost missing</span>' : ''}</span></div>`).join('')}</div>
      <p class="subtle">The token itself is never shown; the signature is not checked here.</p></div></details></section>`;
  }
  function render() {
    const t = D.auth.find((x) => x.id === S.authType);
    const types = D.auth.filter((a) => !a.coming);
    const list = types.map((a) => `<button class="auth-item" role="option" aria-selected="${a.id === S.authType}" data-action="auth-type" data-v="${a.id}" data-fk="auth-${a.id}">${ic(a.ico)}<span>${esc(a.label)}</span>${a.used ? `<span class="meta">${a.used}</span>` : ''}</button>`).join('');
    const pick = `<div class="field auth-pick"><label for="auth-type-pick">Auth type</label><select class="select" id="auth-type-pick">${types.map((a) => `<option value="${a.id}"${a.id === S.authType ? ' selected' : ''}>${esc(a.label)}${a.used ? ' · ' + esc(a.used) : ''}</option>`).join('')}</select></div>`;
    return `<div class="stage-one"><section class="glass card-hug" aria-label="Auth registry">
      <div class="card-head"><div class="grow"><h2 class="card-title">Auth</h2><div class="card-sub" title="Named auth blocks in vibranium.config.yaml; secrets stay in the keychain or env">Profiles and apis pick one of these by name.</div></div>
        <button class="btn btn-sm" data-action="toast" data-msg="New auth block" data-sub="Pick a type, then name it">${ic('plus')}New</button></div>
      <div class="card-body"><div class="auth-grid">
        <div class="auth-list" role="listbox" aria-label="Auth types">${list}</div>${pick}
        <div class="stack auth-detail">
          <div class="row">${ic(t.ico)}<h3 class="card-title">${esc(t.label)}</h3><span class="chip mono">type: ${esc(t.id)}</span><span class="grow"></span>
            <button class="btn btn-primary btn-sm" data-action="auth-test" data-fk="auth-test">${ic('bolt')}${S.authType === 'awsSigV4' ? 'Test signing' : 'Test auth'}</button></div>
          ${t.id === 'mtls' ? `<p class="callout" data-tone="info">${ic('cert')}<span>mTLS is set under <code>http.tls.clientCert</code>; it can sit beside any other auth. V3107 warns 30 days before the certificate expires.</span></p>` : ''}
          ${t.id === 'flow' ? `<p class="callout" data-tone="info">${ic('flow')}<span>A flow runs the listed calls once per profile, then applies values from their responses to every request. Tokens are cached and refreshed before expiry.</span></p>` : ''}
          <div class="auth-form">${rows(S.authType).map(fieldHtml).join('')}</div>
          ${testResult()}
          <details class="disc"><summary>${ic('chev-down', 'chev')}YAML<span class="meta">used by local, dev</span></summary><div class="disc-body"><pre class="code">${hlYaml(yaml(S.authType))}</pre></div></details>
        </div>
      </div></div>
    </section></div>`;
  }
  A['auth-type'] = (el) => { S.authType = el.dataset.v; S.authTested = false; VC.render(); };
  A['auth-test'] = () => { S.authTested = true; VC.render(); VC.toast(S.authType === 'awsSigV4' ? 'Signed with SigV4' : 'Auth works', S.authType === 'awsSigV4' ? 'Credentials from env · region eu-west-1' : 'Token issued and cached for 59 min'); };

  document.addEventListener('change', (e) => {
    if (e.target.id !== 'auth-type-pick') return;
    S.authType = e.target.value; S.authTested = false; VC.render();
    const s = document.getElementById('auth-type-pick'); if (s) s.focus();
  });

  VC.screens['client-auth'] = {
    crumbs: () => [D.project.name, 'Auth', D.auth.find((x) => x.id === S.authType).label],
    status: () => '',
    render
  };

  /* the request card's Auth tab */
  VC.authTab = function () {
    return `<section class="layer" aria-label="Inherited auth"><div class="layer-head"><h3 class="section-label">${ic('lock', 'ico-sm')}Inherited <span class="subtle">· read-only</span></h3><span class="subtle">from profile ${esc(S.profile)}</span></div>
      <div class="kv is-readonly" role="table" aria-label="Inherited auth">
        <div class="kv-row" role="row"><span class="k" role="cell">auth</span><span class="v" role="cell"><span class="mono">shopAuth · oauth2ClientCredentials</span></span><span class="s" role="cell">${chipSrc('profile', 'vibranium.config.yaml:21')}</span></div>
        <div class="kv-row" role="row"><span class="k" role="cell">authorization</span><span class="v" role="cell"><span class="mono">Bearer <span class="redacted">••••</span></span></span><span class="s" role="cell"><span class="subtle">refreshed 14:05</span></span></div>
        <div class="kv-row" role="row"><span class="k" role="cell">clientCert</span><span class="v" role="cell"><span class="mono subtle">none</span></span><span class="s" role="cell">${chipSrc('config', 'http.tls')}</span></div>
      </div></section>
      <section class="layer" aria-label="This request"><div class="layer-head"><h3 class="section-label">This request</h3></div>
        <div class="field"><label for="auth-pick">Use</label><select class="select" id="auth-pick"><option>Inherit (shopAuth)</option><option>none</option><option>adminAuth · oauth2ClientCredentials</option><option>gatewayAuth · awsSigV4</option></select><span class="hint">Pick none to test a 401.</span></div>
        <div class="row"><button class="link" data-action="go" data-route="client-auth" title="Secrets resolve at send time and are redacted in history">Open auth registry</button></div>
      </section>`;
  };
})();
