/* Settings space (#project-settings): the sections sit in the space's sidebar, grouped by
   three clearly badged scopes; the main area shows the one selected section, full width. Project = vibranium.config.yaml
   (shared through git), You only = ~/.config/vibranium (this machine), UI preferences (this
   browser). Config edits collect in a pending bar and are written through a diff. Secrets
   are never shown: •••• plus where they come from, with no Reveal. */
(function () {
  'use strict';
  const VC = window.VC, D = VC.data, A = VC.actions, SH = VC.sh, S = VC.state;
  const { $, $$, esc, ic } = VC.h;
  const T = D.shared.settings;

  const C = { scope: 'project', sec: { project: 'general', user: 'defaults', ui: 'appearance' }, pending: {}, saved: {}, more: {} };
  const SCOPES = {
    project: { label: 'Project', file: 'vibranium.config.yaml', where: 'vibranium.config.yaml · shared with your team through git', ico: 'folder' },
    user: { label: 'You only', file: '~/.config/vibranium/config.yaml', where: '~/.config/vibranium/config.yaml · this machine, never committed', ico: 'user' },
    ui: { label: 'UI preferences', file: null, where: 'this browser · applied at once', ico: 'sliders' }
  };
  const SECTIONS = {
    project: [['general', 'General'], ['paths', 'Paths'], ['defaults', 'Defaults'], ['profiles', 'Profiles'], ['vars', 'Vars'], ['http', 'HTTP'], ['tls', 'TLS'], ['auth', 'Auth'], ['secrets', 'Secrets'], ['reports', 'Reports & history'], ['alerts', 'Alerts'], ['agents', 'Agent access'], ['plugins', 'Plugins']],
    user: [['defaults', 'Defaults'], ['reports', 'Reports'], ['vars', 'Vars'], ['plugins', 'Plugins']],
    ui: [['appearance', 'Appearance'], ['access', 'Accessibility'], ['layout', 'Layout & tabs']]
  };
  /* density, idle-tab time and tints are app prefs (VC.state + VC.savePrefs in app.js) */

  /* ---- fields ------------------------------------------------------------------ */
  const fileOf = (key) => (key.startsWith('u:') ? SCOPES.user.file : SCOPES.project.file);
  const val = (key, d) => (C.pending[key] ? C.pending[key].now : key in C.saved ? C.saved[key] : d);
  const fid = (key) => 'cf-' + key.replace(/[^a-z0-9]+/gi, '-');
  const hint = (h) => (h ? `<span class="hint">${h}</span>` : '');
  function inp(key, label, d, o) {
    o = o || {};
    const v = val(key, d), id = fid(key);
    const ctl = o.area ? `<textarea class="textarea mono" id="${id}" rows="${o.rows || 3}" data-cfg="${key}" data-was="${esc(d)}" data-fk="${id}">${esc(v)}</textarea>`
      : `<input class="input${o.mono ? ' mono' : ''}" id="${id}" value="${esc(v)}" data-cfg="${key}" data-was="${esc(d)}" data-fk="${id}"${o.ph ? ` placeholder="${esc(o.ph)}"` : ''}${o.num ? ' inputmode="numeric"' : ''}>`;
    return `<div class="field${o.wide ? ' cfg-wide' : ''}"><label for="${id}"${o.key ? ` title="${esc(o.key)}"` : ''}>${label}</label>${ctl}${hint(o.hint)}</div>`;
  }
  function sel(key, label, d, opts, o) {
    o = o || {};
    const v = String(val(key, d)), id = fid(key);
    return `<div class="field"><label for="${id}">${label}</label><select class="select" id="${id}" data-cfg="${key}" data-was="${esc(d)}" data-fk="${id}">${opts.map((x) => { const [k, l] = Array.isArray(x) ? x : [x, x]; return `<option value="${esc(k)}"${String(k) === v ? ' selected' : ''}>${esc(l)}</option>`; }).join('')}</select>${hint(o.hint)}</div>`;
  }
  function sw(key, label, d, o) {
    o = o || {};
    const v = String(val(key, String(d))) === 'true';
    return `<div class="cfg-sw"><label class="switch${o.danger ? ' is-danger' : ''}"><input type="checkbox" data-cfg="${key}" data-was="${d}" ${v ? 'checked' : ''} data-fk="${fid(key)}"><span class="track"></span><span>${label}</span></label>${hint(o.hint)}</div>`;
  }
  function segf(key, label, d, opts) {
    const v = val(key, d);
    return `<div class="field"><span class="label">${label}</span><div class="seg" role="group" aria-label="${esc(label)}">${opts.map(([k, l]) => `<button aria-pressed="${v === k}" data-action="cfg-seg" data-cfg="${key}" data-was="${esc(d)}" data-v="${k}" data-fk="${fid(key)}-${k}">${l}</button>`).join('')}</div></div>`;
  }
  function chips(key, label, d, opts) {
    const v = val(key, d).split(',').filter(Boolean);
    return `<div class="field"><span class="label">${label}</span><div class="row row-wrap" role="group" aria-label="${esc(label)}">${opts.map((k) => `<button class="chip chip-lg" aria-pressed="${v.includes(k)}" data-action="cfg-chip" data-cfg="${key}" data-was="${esc(d)}" data-v="${k}" data-fk="${fid(key)}-${k}">${v.includes(k) ? ic('check', 'ico-sm') : ''}${k}</button>`).join('')}</div></div>`;
  }
  const secret = (name, src) => `<span class="cfg-secret" title="${esc(name ? name + ' · from ' + src : 'from ' + src)}"><span class="redacted">••••</span>${name ? `<span class="mono subtle">${esc(name)}</span>` : ''}<span class="subtle">· ${esc(src)}</span></span>`;
  const group = (title, sub, body, act) => `<section class="cfg-group" aria-label="${esc(title)}"><div class="cfg-group-head"><div class="grow"><h3 class="in-h">${title}</h3>${sub ? `<span class="subtle">${sub}</span>` : ''}</div>${act || ''}</div>${body}</section>`;
  const grid = (...f) => `<div class="cfg-grid">${f.join('')}</div>`;
  /* P2: advanced keys sit in "More options", collapsed unless it holds two fields or fewer;
     its open state survives a re-render */
  const more = (id, ...f) => `<details class="more" data-more="${id}"${(C.more[id] ?? f.length <= 2) ? ' open' : ''}><summary>${ic('chev-down', 'chev')}More options</summary><div class="more-body">${grid(...f)}</div></details>`;

  /* ---- project sections ----------------------------------------------------------- */
  const P = {};
  P.general = () => group('General', 'Top of vibranium.config.yaml', grid(
    inp('p:project', 'Project name', T.general.project, { key: 'project' }),
    sel('p:syntaxVersion', 'Syntax version', String(T.general.syntaxVersion), ['2'], { hint: 'Only v2 exists. vc migrate converts v1 files.' })) +
    more('general', inp('p:hooks.threads', 'Hook threads', String(T.general.hooksThreads), { key: 'hooks.threads', num: true, hint: 'Threads for function: hooks. 1 keeps them in order.' })));
  P.paths = () => group('Paths', 'Globs, one per line, relative to the project root', grid(
    inp('p:paths.include', 'Include', T.paths.include, { area: true, key: 'paths.include' }),
    inp('p:paths.exclude', 'Exclude', T.paths.exclude, { area: true, key: 'paths.exclude', hint: '.vibranium is always excluded.' })));
  P.defaults = () => group('Defaults', 'Apply to every call unless an api, test or profile sets its own', grid(
    inp('p:defaults.timeout', 'Timeout', T.defaults.timeout, { mono: true, key: 'defaults.timeout' }),
    inp('p:defaults.concurrency', 'Concurrency', String(T.defaults.concurrency), { num: true, hint: 'Tests in flight across the run.' }),
    inp('p:defaults.retry.attempts', 'Test retries', String(T.defaults.retryAttempts), { num: true, hint: 'A pass on retry marks the test flaky.' })) +
    more('defaults',
      inp('p:defaults.perHost', 'Per host', String(T.defaults.perHost), { num: true, hint: 'Connections to one host at once.' }),
      inp('p:defaults.retry.delay', 'Retry delay', T.defaults.retryDelay, { mono: true }))) +
    group('Default headers', 'Sent on every request', `<div class="kv glass-dense">${T.defaults.headers.map(([k, v]) => `<div class="kv-row"><span class="k">${esc(k)}</span><span class="v"><input class="input" aria-label="${esc(k)} value" value="${esc(val('p:defaults.headers.' + k, v))}" data-cfg="p:defaults.headers.${esc(k)}" data-was="${esc(v)}" data-fk="cf-h-${esc(k)}"></span><span class="s"></span></div>`).join('')}</div>`,
      `<button class="btn btn-sm" data-action="cfg-add" data-what="header">${ic('plus')}Header</button>`);
  P.profiles = () => group('Profiles', 'Where tests run. The default is used when no --profile is given. A profile\'s tint colours the whole UI.', SH.table(['Id', 'Name', 'Default', 'Vars', 'Agents'],
    T.profiles.map((p) => `<tr><td><span class="row"><span class="cfg-tint" data-tint="${esc((D.profiles.find((x) => x.id === p.id) || {}).tint || D.project.tint)}" aria-hidden="true"></span><span class="mono">${esc(p.id)}</span></span></td><td>${esc(p.name)}</td>
      <td><input type="radio" name="cfg-def" data-cfg="p:profiles.default" data-was="local" value="${p.id}" aria-label="Make ${p.id} the default" ${val('p:profiles.default', 'local') === p.id ? 'checked' : ''}></td>
      <td class="muted num">${p.vars} set</td><td><button class="btn btn-ghost btn-sm" data-action="go" data-route="agents">${esc(p.agents)}</button></td></tr>`)),
    `<button class="btn btn-sm" data-action="cfg-add" data-what="profile">${ic('plus')}Profile</button>`);
  P.vars = () => {
    const ps = ['local', 'dev', 'staging', 'prod'];
    const rows = T.vars.map((v) => `<tr><th scope="row" class="mono">${esc(v.k)}</th><td>${cellIn(`p:vars.${v.k}`, v.base, v.k + ' base')}</td>${ps.map((p) => `<td>${cellIn(`p:profiles.${p}.vars.${v.k}`, v[p] === null ? '' : v[p], `${v.k} on ${p}`, 'inherits')}</td>`).join('')}</tr>`);
    return group('Vars', 'Base values, then what each profile overrides. Empty cells inherit. Secrets do not belong here: use ${secret.NAME}.',
      `<div class="table-wrap glass-dense cfg-vars"><table class="table"><thead><tr><th scope="col">Var</th><th scope="col">Base</th>${ps.map((p) => `<th scope="col">${p}</th>`).join('')}</tr></thead><tbody>${rows.join('')}</tbody></table></div>`,
      `<button class="btn btn-sm" data-action="cfg-add" data-what="var">${ic('plus')}Var</button>`);
  };
  const cellIn = (key, d, label, ph) => `<input class="input input-sm mono" aria-label="${esc(label)}" value="${esc(val(key, d))}" data-cfg="${key}" data-was="${esc(d)}" data-fk="${fid(key)}"${ph ? ` placeholder="${ph}"` : ''}>`;
  P.http = () => group('Retries', 'Transport-level, before a test sees the response', grid(
    inp('p:http.retry.attempts', 'Attempts', String(T.http.attempts), { num: true, key: 'http.retry.attempts' }),
    inp('p:http.retry.on', 'Retry on', T.http.on, { mono: true, hint: 'connect, timeout, or status codes.' }),
    inp('p:http.retry.methods', 'Methods', T.http.methods, { mono: true, hint: 'POST and PATCH are left out: they may not be idempotent.' })) +
    more('http',
      inp('p:http.retry.backoff.base', 'Backoff base', T.http.base, { mono: true }),
      inp('p:http.retry.backoff.max', 'Backoff max', T.http.max, { mono: true }),
      sw('p:http.retry.backoff.jitter', 'Jitter', T.http.jitter)));
  P.tls = () => group('Trust', 'Added to the system roots', grid(inp('p:http.tls.ca', 'CA bundle', T.tls.ca, { mono: true, key: 'http.tls.ca' }))) +
    group('Client certificates', 'mTLS, matched by host', SH.table(['Host', 'Certificate', 'Key', 'Expires'], T.tls.clients.map((c) => `<tr><td class="mono">${esc(c.host)}</td><td class="mono">${esc(c.cert)}</td><td>${secret(c.key, c.src)}</td><td class="mono muted">${esc(c.exp)}</td></tr>`)),
      `<button class="btn btn-sm" data-action="cfg-add" data-what="client cert">${ic('plus')}Client cert</button>`);
  P.auth = () => group('Auth', 'Named auth, referenced by apis with <code>auth: id</code>', `<ul class="cfg-auth" role="list">${T.auth.map((a) => `<li class="cfg-auth-item"><div class="row row-wrap">${ic(a.ico)}<b class="mono">${esc(a.id)}</b><span class="chip">${esc(a.type)}</span><span class="grow"></span><button class="btn btn-sm" data-action="cfg-auth-test" data-id="${esc(a.id)}" data-fk="cat-${esc(a.id)}">${ic('play')}Test</button></div>
    <div class="kv glass-dense">${a.fields.map(([k, v, sn, src]) => `<div class="kv-row"><span class="k">${esc(k)}</span><span class="v">${sn ? secret(sn, src) : VC.h.hlVars(v)}</span><span class="s"></span></div>`).join('')}</div></li>`).join('')}</ul>`,
    `<button class="btn btn-sm" data-action="go" data-route="client-auth">${ic('external')}Auth registry</button>`);
  P.secrets = () => group('Provider', 'Where ${secret.NAME} values come from', grid(
    segf('p:secrets.provider', 'Provider', 'env', [['env', 'env + .env'], ['varlock', 'varlock']]),
    inp('p:secrets.schema', 'Schema', '.env.schema', { mono: true, hint: 'Names, types and which are optional. Values never go in the repo.' }))) +
    group('Secrets', 'Values are never shown here, in history, in copies or to agents.', SH.table(['Name', 'Value', 'Status', ['Used by', 'r'], ''],
      T.secrets.map((s) => `<tr><td class="mono">${esc(s.name)}</td><td>${s.set ? secret('', s.src) : '<span class="subtle">not set</span>'}</td>
        <td>${s.set ? '<span class="badge badge-sm st-pass">' + ic('check') + 'set</span>' : `<span class="badge badge-sm st-${s.optional ? 'skipped' : 'fail'}">${ic(s.optional ? 'skipped' : 'x')}${s.optional ? 'optional · missing' : 'missing'}</span>`}</td>
        <td class="r muted num">${s.used}</td><td class="r"><button class="btn btn-sm" data-action="cfg-secret-set" data-name="${esc(s.name)}">${s.set ? 'Replace…' : 'Set…'}</button></td></tr>`)));
  P.reports = () => group('Reports', '', grid(
    inp('p:reports.dir', 'Folder', T.reports.dir, { mono: true }),
    chips('p:reports.formats', 'Formats', T.reports.formats.join(','), ['ctrf', 'junit', 'html']),
    segf('p:reports.html.layout', 'HTML layout', T.reports.layout, [['folder', 'Folder'], ['single', 'Single file']]))) +
    group('History', 'Stored in .vibranium/history.db', grid(
      segf('p:history.bodies', 'Store bodies', T.reports.bodies, [['none', 'None'], ['failures', 'Failures'], ['all', 'All']]),
      inp('p:history.keep', 'Keep runs', String(T.reports.keep), { num: true })) +
      more('history', inp('p:history.maxBodySize', 'Max body size', T.reports.maxBodySize, { mono: true, hint: 'Larger bodies are cut; redaction runs first.' })));
  P.alerts = () => group('Alert rules', 'Sent when a run on these profiles matches. Webhook secrets show as ••••.', `<ul class="cfg-auth" role="list">${T.alerts.map((r, i) => `<li class="cfg-auth-item"><div class="row row-wrap"><span class="grow cfg-rule">On <b>${r.on.map(esc).join(', ')}</b> for <span class="mono">${r.profiles.map(esc).join(', ')}</span>${r.tags.length ? ` tagged <span class="mono">${r.tags.map(esc).join(', ')}</span>` : ''}</span><button class="btn btn-sm" data-action="cfg-alert-test" data-i="${i}" data-fk="cfa-${i}">${ic('send')}Send test alert</button></div>
      <div class="kv glass-dense">${r.ch.map(([c, t, s]) => `<div class="kv-row"><span class="k">${esc(c)}</span><span class="v">${esc(t)}</span><span class="s">${s ? secret(s, (T.secrets.find((x) => x.name === s) || {}).src || 'env') : ''}</span></div>`).join('')}</div></li>`).join('')}</ul>`,
    `<button class="btn btn-sm" data-action="cfg-add" data-what="alert rule">${ic('plus')}Rule</button>`);
  P.agents = () => group('Agent access', 'Per profile. Edited on the Agents screen.', SH.table(['Profile', 'Policy'], T.profiles.map((p) => `<tr><td class="mono">${esc(p.id)}</td><td>${p.id === 'prod' ? `<span class="badge badge-sm st-skipped">${ic('lock')}reads only</span>` : esc(p.agents)}</td></tr>`)),
    `<button class="btn btn-sm" data-action="go" data-route="agents">${ic('bot')}Open Agents</button>`);
  P.plugins = () => group('Plugins', 'Pinned in vibranium.lock. A plugin whose files change is refused until you approve it again.', `<ul class="cfg-auth" role="list">${T.plugins.map((p) => `<li class="cfg-auth-item"><div class="row row-wrap">${ic('plug')}<b class="mono">${esc(p.id)}</b><span class="subtle">${esc(p.kind)}</span><span class="mono subtle ellipsis grow" title="${esc(p.source)} → ${esc(p.resolved)}">${esc(p.source)}</span>
      ${p.ok ? `<span class="badge badge-sm st-pass">${ic('check')}verified</span>` : `<span class="badge badge-sm st-fail" title="${esc(p.note)}">${ic('warn')}files changed</span><button class="btn btn-sm" data-action="cfg-plugin-approve" data-id="${esc(p.id)}">Review & approve</button>`}
      <button class="btn btn-ghost btn-sm" data-action="cfg-plugin-remove" data-id="${esc(p.id)}" aria-label="Remove ${esc(p.id)}">${ic('x')}Remove</button></div>
      <div class="cfg-perms subtle">May use <span class="mono">${p.perms.map(esc).join(' · ')}</span></div></li>`).join('')}</ul>`,
    `<button class="btn btn-sm" data-action="cfg-plugin-add">${ic('plus')}Add plugin</button>`);

  /* ---- you-only sections -------------------------------------------------------------- */
  const UO = {};
  UO.defaults = () => group('Defaults on this machine', 'Override the project for you only. Empty means "use the project\'s".', grid(
    sel('u:profile', 'Default profile', T.user.profile, D.profiles.map((p) => p.id), { hint: 'Used when you run without --profile.' }),
    inp('u:defaults.timeout', 'Timeout', T.user.timeout, { mono: true, ph: T.defaults.timeout + ' (project)' }),
    inp('u:defaults.concurrency', 'Concurrency', T.user.concurrency, { num: true, ph: T.defaults.concurrency + ' (project)', hint: 'Lower on a laptop on battery.' })));
  UO.reports = () => group('Reports', '', grid(inp('u:reports.dir', 'Folder', T.user.reportsDir, { mono: true, ph: T.reports.dir + ' (project)' }), inp('u:reports.formats', 'Formats', T.user.formats, { mono: true, ph: 'ctrf, junit, html (project)' })));
  UO.vars = () => group('Your vars', 'Win over project and profile vars, on this machine only. For secrets use the keychain.', `<div class="kv glass-dense">${T.user.vars.map(([k, v]) => `<div class="kv-row"><span class="k">${esc(k)}</span><span class="v"><input class="input" aria-label="${esc(k)}" value="${esc(val('u:vars.' + k, v))}" data-cfg="u:vars.${esc(k)}" data-was="${esc(v)}" data-fk="cf-uv-${esc(k)}"></span><span class="s"></span></div>`).join('')}</div>`,
    `<button class="btn btn-sm" data-action="cfg-add" data-what="var">${ic('plus')}Var</button>`);
  UO.plugins = () => group('Plugin sandbox', '', sw('u:plugins.allowUnconfined', 'Allow unconfined plugins', T.user.allowUnconfined, { danger: true, hint: 'Lets a plugin skip its declared permissions on this machine. Never set in CI.' }) +
    (String(val('u:plugins.allowUnconfined', 'false')) === 'true' ? `<p class="callout" data-tone="danger">${ic('warn')}<span><b>Plugins could read any file and reach any host.</b> Turn this off once you finish debugging the plugin.</span></p>` : ''));

  /* ---- UI preferences (applied at once, like the dev corner) ----------------------------- */
  /* the same names as tokens.css [data-tint]; a saved 'violet' reads as graphite (app.js applyRoot) */
  const TINTS = ['graphite', 'blue', 'teal', 'amber', 'red'];
  const tintOf = (t) => (t === 'violet' ? 'graphite' : t);
  const pseg = (label, k, cur, opts) => `<div class="field"><span class="label">${label}</span><div class="seg" role="group" aria-label="${esc(label)}">${opts.map(([v, l]) => `<button aria-pressed="${cur === v}" data-action="cfg-pref" data-k="${k}" data-v="${v}" data-fk="pf-${k}-${v}">${l}</button>`).join('')}</div></div>`;
  const tintPick = (label, who, cur, none) => `<div class="cfg-tints" role="radiogroup" aria-label="${esc(label)}">${(none ? [[null, none]] : []).concat(TINTS.map((t) => [t, t])).map(([t, l]) => `<button class="cfg-swatch" role="radio" aria-checked="${tintOf(cur) === t}" data-action="cfg-tint" data-who="${who}" data-v="${t || ''}" data-tint="${t || 'none'}" title="${l}" aria-label="${l}" data-fk="tn-${who}-${t}"></button>`).join('')}</div>`;
  const UI = {};
  UI.appearance = () => group('Theme', 'Match system follows the OS light or dark setting.', pseg('Theme', 'theme', S.theme, [['system', 'Match system'], ['light', 'Light'], ['dark', 'Dark']])) +
    group('Space switcher', 'Where the Client, Tests, Runs, Agents and Settings icons sit. ⌘1–⌘5 work either way.',
      pseg('Space switcher', 'switcher', S.switcher === 'foot' ? 'foot' : 'rail', [['rail', 'Icon rail'], ['foot', 'Sidebar bottom']])) +
    group('Tint', 'The project has a tint; a profile may set its own so risky ones stand out. Saved to vibranium.config.yaml so the team sees the same colours.',
      `<div class="cfg-tint-rows"><div class="cfg-tint-row"><span class="mono grow">project · ${esc(D.project.name)}</span>${tintPick('Project tint', 'project', D.project.tint)}</div>
      ${D.profiles.map((p) => `<div class="cfg-tint-row"><span class="mono grow">profile · ${esc(p.id)}</span>${tintPick(`${p.id} tint`, p.id, p.tint, 'Same as project')}</div>`).join('')}</div>`) +
    group('This browser', 'Override every tint while you work here.', pseg('Browser tint', 'tint', tintOf(S.tint), [['auto', 'Follow profile']].concat(TINTS.map((t) => [t, t[0].toUpperCase() + t.slice(1)]))));
  UI.access = () => group('Motion and transparency', 'Match system follows your OS settings.', grid(
    pseg('Motion', 'motion', S.motion, [['system', 'Match system'], ['reduce', 'Reduce'], ['full', 'Full']]),
    pseg('Transparency', 'transparency', S.transparency, [['system', 'Match system'], ['reduce', 'Reduce'], ['full', 'Full']])));
  UI.layout = () => group('Density', 'Compact fits more rows in tables and trees.', pseg('Density', 'density', S.density, [['comfortable', 'Comfortable'], ['compact', 'Compact']])) +
    group('Tabs', 'Client tabs you have not looked at for this long close themselves. Unsaved edits are kept as a draft.', grid(
      `<div class="field"><label for="cf-idle">Close idle tabs after</label><select class="select" id="cf-idle" data-fk="cf-idle">${[['1h', '1 hour'], ['4h', '4 hours'], ['12h', '12 hours (default)'], ['24h', '1 day'], ['never', 'Never']].map(([k, l]) => `<option value="${k}"${S.idle === k ? ' selected' : ''}>${l}</option>`).join('')}</select></div>`));

  /* ---- shell ------------------------------------------------------------------------------ */
  const RENDER = { project: P, user: UO, ui: UI };
  const pendingFor = (scope) => Object.entries(C.pending).filter(([k]) => (scope === 'user' ? k.startsWith('u:') : k.startsWith('p:')));
  function pendingBar() {
    const n = Object.keys(C.pending).length;
    if (!n) return `<span title="${C.scope === 'ui' ? '' : 'digest ' + T.digest}">${ic('check', 'ico-sm')} ${C.scope === 'ui' ? 'UI preferences apply at once' : 'No unsaved changes'}</span>`;
    const files = [...new Set(Object.keys(C.pending).map(fileOf))];
    return `<span class="cfg-dirty">${ic('info', 'ico-sm')}<b>${n} unsaved change${n > 1 ? 's' : ''}</b> in ${files.map((f) => `<span class="mono">${esc(f)}</span>`).join(' and ')}</span><span class="grow"></span>
      <button class="btn btn-sm" data-action="cfg-discard" data-fk="cfg-discard">Discard</button><button class="btn btn-primary btn-sm" data-action="cfg-review" data-fk="cfg-review">${ic('eye')}Review & save</button>`;
  }
  /* the sidebar: the three scopes are quiet group labels, their sections sit under them */
  const dirtyIn = (scope, k) => pendingFor(scope).some(([key]) => key.slice(2).startsWith(k === 'vars' ? 'vars' : k) || (k === 'vars' && key.includes('.vars.')));
  const onSettings = () => S.route === 'project-settings';
  function navGroup([sc, o]) {
    const n = sc === 'ui' ? 0 : pendingFor(sc).length;
    const items = SECTIONS[sc].map(([k, l]) => ({
      label: l, action: 'cfg-sec', attrs: `data-scope="${sc}" data-v="${k}"`, fk: `cfs-${sc}-${k}`,
      current: onSettings() && sc === C.scope && C.sec[sc] === k, meta: dirtyIn(sc, k) ? '<span class="cfg-dot" aria-label="unsaved"></span>' : ''
    }));
    return `<section class="sb-section" aria-labelledby="cfn-${sc}"><h2 class="sb-label" id="cfn-${sc}">${esc(o.label)}${n ? `<span class="count" aria-label="${n} unsaved">${n}</span>` : ''}</h2>${VC.sbNav(items)}</section>`;
  }
  const sidebar = () => Object.entries(SCOPES).map(navGroup).join('');
  VC.spaces.settings.sidebar = sidebar;
  const secLabel = () => SECTIONS[C.scope].find(([k]) => k === C.sec[C.scope])[1];
  VC.screens['project-settings'] = {
    space: 'settings',
    crumbs: () => [D.project.name, 'Settings', SCOPES[C.scope].label, secLabel()],
    status: () => '',
    render: () => {
      const sc = SCOPES[C.scope], sec = C.sec[C.scope];
      return `<div class="stage-one sh-scope"><section class="glass stage-one rn-hug" aria-labelledby="cfg-title">
        <div class="card-head"><div class="grow"><h2 class="card-title" id="cfg-title">${esc(secLabel())}</h2><div class="card-sub">${ic(sc.ico, 'ico-sm')}<span class="cfg-where" data-scope="${C.scope}">${esc(sc.label)}</span><span>${esc(sc.where)}</span></div></div>
          ${C.scope !== 'ui' ? `<button class="btn btn-sm" data-action="cfg-open-file" data-fk="cfg-open">${ic('code')}Open YAML</button>` : ''}</div>
        <div class="card-body cfg-panel" id="cfs-panel" data-sk="cfg-${C.scope}-${sec}">${RENDER[C.scope][sec]()}</div>
        <div class="card-foot cfg-pending" id="cfg-pending" role="status" data-dirty="${!!Object.keys(C.pending).length}">${pendingBar()}</div>
      </section></div>`;
    }
  };

  /* ---- pending changes ---------------------------------------------------------------------- */
  function record(key, was, now) {
    if (String(now) === String(was)) delete C.pending[key]; else C.pending[key] = { was: String(was), now: String(now) };
    const bar = $('#cfg-pending');
    if (bar) { bar.innerHTML = pendingBar(); bar.dataset.dirty = String(!!Object.keys(C.pending).length); }
    const sb = $('#sidebar .sb-body');
    if (sb && S.space === 'settings') sb.innerHTML = sidebar();
  }
  function yamlLines(key, p) {
    const path = key.slice(2).split('.'), leaf = path.pop(), ind = '  '.repeat(path.length);
    const ln = (path.length ? 8 : 1) + ([...key].reduce((h, c) => (h * 31 + c.charCodeAt(0)) % 997, 7) % (path.length ? 80 : 4));
    const out = [['@', `@@ -${ln} +${ln} @@ ${path[0] || leaf}`]].concat(path.length ? [[' ', path.map((s, i) => '  '.repeat(i) + s + ':').pop()]] : []);
    const multi = (v) => (v.includes('\n') ? v.split('\n').map((x) => `${ind}  - ${x}`) : null);
    const was = multi(p.was), now = multi(p.now);
    if (was || now) { out.push([' ', `${ind}${leaf}:`]); (was || []).forEach((l) => out.push(['-', l])); (now || []).forEach((l) => out.push(['+', l])); return out; }
    if (p.was !== '') out.push(['-', `${ind}${leaf}: ${p.was}`]);
    if (p.now !== '') out.push(['+', `${ind}${leaf}: ${p.now}`]);
    return out;
  }
  A['cfg-review'] = () => {
    const by = {};
    Object.entries(C.pending).forEach(([k, p]) => { (by[fileOf(k)] = by[fileOf(k)] || []).push(...yamlLines(k, p)); });
    VC.diffDialog({
      title: 'Save settings', sub: 'Comments and key order in the files are kept. vc check runs after the write.',
      files: Object.entries(by).map(([path, lines]) => ({ path, lines })), confirm: 'Write',
      callout: 'If a file changed on disk since you opened Settings, you get Reload or Keep mine.',
      after: () => { Object.entries(C.pending).forEach(([k, p]) => { C.saved[k] = p.now; }); C.pending = {}; T.digest = 'c81a9d2'; VC.render(); }
    });
  };
  A['cfg-discard'] = () => { C.pending = {}; VC.render(); VC.toast('Discarded unsaved changes'); };
  /* VC.go re-renders on the same route and closes the narrow drawer; each section keeps its own scroll */
  A['cfg-sec'] = (el) => { C.scope = el.dataset.scope; C.sec[C.scope] = el.dataset.v; VC.go('project-settings'); };
  A['cfg-seg'] = (el) => { record(el.dataset.cfg, el.dataset.was, el.dataset.v); VC.render(); };
  A['cfg-chip'] = (el) => {
    const k = el.dataset.cfg, cur = val(k, el.dataset.was).split(',').filter(Boolean);
    const next = cur.includes(el.dataset.v) ? cur.filter((x) => x !== el.dataset.v) : cur.concat(el.dataset.v);
    record(k, el.dataset.was, ['ctrf', 'junit', 'html'].filter((x) => next.includes(x)).join(','));
    VC.render();
  };
  A['cfg-open-file'] = () => VC.toast(`Opened ${SCOPES[C.scope].file}`, 'In your editor ($VISUAL). Settings reloads when the file changes.');
  A['cfg-add'] = (el) => VC.toast(`Add ${el.dataset.what}`, 'Adds an empty row here; it joins the pending changes.');
  A['cfg-auth-test'] = (el) => VC.toast(`${el.dataset.id}: got a token in 182 ms`, 'Expires in 59 min · scopes match · the token is not shown');
  A['cfg-alert-test'] = (el) => { const r = T.alerts[+el.dataset.i]; VC.toast(`Sent a test alert to ${r.ch.map((c) => c[0]).join(' and ')}`, 'Marked as a test · no run attached'); };
  A['cfg-secret-set'] = (el) => {
    const n = el.dataset.name;
    VC.dialog(`<div class="dlg dlg-form"><div class="dlg-head"><div class="grow"><h2 id="dlg-title">Set ${esc(n)}</h2><div class="sub">Stored in the macOS keychain for this project. The UI does not keep or show it.</div></div><button class="btn btn-ghost btn-icon" data-action="close-dialog" aria-label="Close">${ic('x')}</button></div>
      <div class="dlg-body"><div class="field"><label for="cfg-secret-v">Value</label><input class="input mono" id="cfg-secret-v" type="password" autocomplete="off" spellcheck="false"><span class="hint" title="vc secrets set ${esc(n)}">Redacted everywhere it appears.</span></div></div>
      <div class="dlg-foot"><button class="btn" data-action="close-dialog">Cancel</button><button class="btn btn-primary" data-action="cfg-secret-save" data-name="${esc(n)}">${ic('lock')}Store</button></div></div>`);
  };
  A['cfg-secret-save'] = (el) => {
    const s = T.secrets.find((x) => x.name === el.dataset.name);
    if (s) { s.set = true; s.src = s.src === 'env' && !s.set ? 'keychain' : s.src; }
    VC.closeDialog(); VC.toast(`Stored ${el.dataset.name}`, 'keychain · runs pick it up at once'); VC.render();
  };
  const lockLines = (p, sign) => [['@', `@@ plugins.${p.id} @@`], [sign, `${p.id}:`], [sign, `  source: ${p.source}`], [sign, `  resolved: ${p.resolved}`], [sign, '  permissions:']].concat(p.perms.map((x) => [sign, `    - ${x}`]));
  A['cfg-plugin-remove'] = (el) => {
    const i = T.plugins.findIndex((x) => x.id === el.dataset.id), p = T.plugins[i];
    VC.diffDialog({ title: `Remove ${p.id}`, sub: 'Apis or reporters that use it fail vc check until you change them.', confirm: 'Remove',
      files: [{ path: 'vibranium.config.yaml', lines: [['@', '@@ plugins @@'], ['-', `  - id: ${p.id}`], ['-', `    source: ${p.source}`]] }, { path: 'vibranium.lock', lines: lockLines(p, '-') }],
      after: () => { T.plugins.splice(i, 1); VC.render(); } });
  };
  A['cfg-plugin-approve'] = (el) => {
    const p = T.plugins.find((x) => x.id === el.dataset.id);
    VC.diffDialog({ title: `Approve ${p.id} again`, sub: 'Its files changed since vc plugin add. Check the change, then pin the new digest.', confirm: 'Approve',
      file: 'vibranium.lock', lines: [['@', `@@ plugins.${p.id} @@`], [' ', `${p.id}:`], [' ', `  source: ${p.source}`], ['-', '  integrity: blake3-41c0e2f7…'], ['+', '  integrity: blake3-9ad13b05…'], [' ', '  permissions:'], [' ', `    - ${p.perms[0]}`]],
      after: () => { p.ok = true; p.resolved = 'blake3-9ad1…'; VC.render(); } });
  };
  A['cfg-plugin-add'] = () => {
    const p = { id: 'slackRich', source: 'npm:@acme/vibranium-slack@^3', resolved: '3.1.0', perms: ['network: hooks.slack.com', 'secrets: SLACK_WEBHOOK'] };
    VC.diffDialog({ title: 'Add plugin', sub: 'The plugin asks for the permissions below. It gets nothing else.', confirm: 'Add and pin',
      fields: `<div class="field"><label for="cfg-pl-src">Source</label><input class="input mono" id="cfg-pl-src" value="${p.source}"><span class="hint">npm:package@range or path:folder</span></div>`,
      files: [{ path: 'vibranium.config.yaml', lines: [['@', '@@ plugins @@'], ['+', `  - id: ${p.id}`], ['+', `    source: ${p.source}`]] }, { path: 'vibranium.lock', lines: lockLines(p, '+') }],
      after: () => { T.plugins.push(Object.assign({ kind: 'notifier', ok: true }, p)); VC.render(); } });
  };
  A['cfg-pref'] = (el) => {
    const k = el.dataset.k, v = el.dataset.v;
    if (VC.setPref) VC.setPref(k, v); else { S[k] = v; VC.savePrefs(); VC.render(); }
  };
  A['cfg-tint'] = (el) => {
    const who = el.dataset.who, v = el.dataset.v || null;
    if (who === 'project') D.project.tint = v; else D.profiles.find((p) => p.id === who).tint = v;
    S.tints = Object.assign({}, S.tints, { [who === 'project' ? '@project' : who]: v }); VC.savePrefs();
    VC.render();
    VC.toast(`${who === 'project' ? 'Project' : who} tint: ${v || 'same as project'}`, 'Written to vibranium.config.yaml (ui.tint)');
  };
  document.addEventListener('toggle', (e) => { const d = e.target; if (d.dataset && d.dataset.more) C.more[d.dataset.more] = d.open; }, true);
  document.addEventListener('input', (e) => { const t = e.target; if (t.dataset && t.dataset.cfg && t.type !== 'checkbox' && t.type !== 'radio') record(t.dataset.cfg, t.dataset.was, t.value); });
  document.addEventListener('change', (e) => {
    const t = e.target;
    if (t.id === 'cf-idle') { S.idle = t.value; VC.savePrefs(); VC.toast(`Idle tabs close after ${t.selectedOptions[0].text.replace(' (default)', '')}`); return; }
    if (!t.dataset || !t.dataset.cfg) return;
    if (t.type === 'checkbox') { record(t.dataset.cfg, t.dataset.was, String(t.checked)); VC.render(); }
    else if (t.type === 'radio') { record(t.dataset.cfg, t.dataset.was, t.value); VC.render(); }
    else if (t.tagName === 'SELECT') record(t.dataset.cfg, t.dataset.was, t.value);
  });
})();
