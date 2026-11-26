/* Shared overlays: diff dialog, Copy as cURL, command palette (⌘K), shortcut sheet (?).
   VC.diffDialog({ title, sub, fields, callout, confirm,
                   files: [{ path, isNew, lines: [[sign, text], ...] }] })   sign: '@' hunk, ' ', '+', '-'
   or the one-file shorthand { file, isNew, lines }. */
(function () {
  'use strict';
  const VC = window.VC, D = VC.data, S = VC.state, A = VC.actions;
  const { $, $$, esc, ic, method, hlVars } = VC.h;

  /* ---- diff ------------------------------------------------------------------ */
  function diffLines(lines) {
    let o = 0, n = 0;
    return lines.map(([sign, text]) => {
      if (sign === '@') {
        const m = text.match(/-(\d+)(?:,\d+)? \+(\d+)/);
        if (m) { o = +m[1]; n = +m[2]; }
        return `<div class="dl hunk"><span></span><span></span><span></span><span>${esc(text)}</span></div>`;
      }
      const cls = sign === '+' ? ' add' : sign === '-' ? ' del' : '';
      const on = sign === '+' ? '' : o++, nn = sign === '-' ? '' : n++;
      return `<div class="dl${cls}"><span>${on}</span><span>${nn}</span><span>${sign === ' ' ? '' : sign}</span><span>${esc(text)}</span></div>`;
    }).join('');
  }
  const counts = (lines) => [lines.filter((l) => l[0] === '+').length, lines.filter((l) => l[0] === '-').length];
  VC.diffHtml = (lines, label) => `<div class="diff glass-dense" role="region" aria-label="${esc(label || 'Diff')}" tabindex="0">${diffLines(lines)}</div>`;

  let cur = null;
  function diffBody(o) {
    const files = o.files;
    const f = files[o.sel || 0];
    if (files.length === 1) {
      const [a, d] = counts(f.lines);
      return `<div class="row"><span class="mono">${esc(f.path)}</span>${f.isNew ? '<span class="chip">new file</span>' : ''}<span class="grow"></span><span class="diff-file"><span class="add">+${a}</span> <span class="del">−${d}</span></span></div>${VC.diffHtml(f.lines, f.path)}`;
    }
    const list = files.map((x, i) => { const [a, d] = counts(x.lines); return `<button class="diff-file" role="tab" aria-selected="${i === (o.sel || 0)}" tabindex="${i === (o.sel || 0) ? 0 : -1}" data-action="diff-file" data-i="${i}" data-fk="df-${i}"><span class="ellipsis grow mono">${esc(x.path)}</span>${x.isNew ? '<span class="chip">new</span>' : ''}<span class="add">+${a}</span><span class="del">−${d}</span></button>`; }).join('');
    return `<div class="review"><div class="diff-files glass-dense" role="tablist" aria-orientation="vertical" aria-label="Files">${list}</div><div class="stack-tight" role="tabpanel">${VC.diffHtml(f.lines, f.path)}</div></div>`;
  }
  VC.diffDialog = function (o) {
    if (!o.files) o.files = [{ path: o.file, isNew: o.isNew, lines: o.lines }];
    o.sel = 0;
    cur = o;
    const [ta, td] = o.files.reduce(([a, d], f) => { const [x, y] = counts(f.lines); return [a + x, d + y]; }, [0, 0]);
    VC.dialog(`<div class="dlg ${o.files.length > 1 ? 'dlg-wide' : ''}">
      <div class="dlg-head"><div class="grow"><h2 id="dlg-title">${esc(o.title)}</h2>${o.sub ? `<div class="sub">${esc(o.sub)}</div>` : ''}</div>
        <button class="btn btn-ghost btn-icon" data-action="close-dialog" aria-label="Close">${ic('x')}</button></div>
      <div class="dlg-body">${o.fields || ''}<div id="diff-body">${diffBody(o)}</div>
        ${o.callout ? `<p class="callout" data-tone="info">${ic('info')}<span>${esc(o.callout)}</span></p>` : ''}</div>
      <div class="dlg-foot"><button class="btn" data-action="close-dialog">Cancel</button><button class="btn btn-primary" data-action="diff-confirm" autofocus title="${o.files.length} file${o.files.length === 1 ? '' : 's'} · +${ta} −${td} · vc fmt runs on write, then vc check">${ic('check')}${esc(o.confirm || 'Write')}</button></div>
    </div>`, o.files.length > 1 ? 'is-wide' : '');
  };
  A['diff-file'] = (el) => { cur.sel = +el.dataset.i; $('#diff-body').innerHTML = diffBody(cur); const b = $(`#diff-body [data-i="${cur.sel}"]`); if (b) b.focus(); };
  A['diff-confirm'] = () => {
    const o = cur;
    VC.closeDialog();
    VC.toast(o.files.length > 1 ? `Wrote ${o.files.length} files` : `Wrote ${o.files[0].path}`, 'vc check · 0 errors · the tree and history show the change');
    if (o.after) o.after();
  };

  /* ---- Copy as cURL ------------------------------------------------------------ */
  let creds = false;
  function curlText(withCreds) {
    const a = VC.api();
    const url = 'http://localhost:7421' + a.path.replace('${userId}', D.vars.userId.value) + (a.q || '');
    const tok = withCreds ? 'eyJhbGciOiJSUzI1NiIsImtpZCI6InNob3AtMSJ9.eyJzdWIiOiJzaG9wLXRlc3RzIn0.Qm9ndXM' : '$VC_TOKEN';
    return `curl -X ${a.m === 'GQL' || a.m === 'SSE' || a.m === 'WS' ? 'GET' : a.m} '${url}' \\\n  -H 'accept: application/json' \\\n  -H 'x-client: vibranium' \\\n  -H 'x-tenant-id: t_shop_01' \\\n  -H "authorization: Bearer ${tok}"`;
  }
  function curlBody() {
    return `<label class="switch is-danger"><input type="checkbox" id="curl-creds" ${creds ? 'checked' : ''}><span class="track"></span><span>Include credentials</span></label>
      ${creds ? `<p class="callout" data-tone="warn">${ic('warn')}<span><b>The copy will hold a live token</b> for 59 min. This turns itself off after one copy.</span></p>`
        : `<p class="subtle" title="export VC_TOKEN=$(vc auth token shopAuth)">Secrets copy as <code>$VC_TOKEN</code>.</p>`}
      <pre class="code glass-dense wrap">${esc(curlText(creds))}</pre>`;
  }
  VC.curlDialog = function () {
    creds = false;
    VC.dialog(`<div class="dlg dlg-narrow is-curl">
      <div class="dlg-head"><div class="grow"><h2 id="dlg-title">Copy as cURL</h2><div class="sub" title="Inherited headers and auth are resolved">${esc(S.api)} under ${esc([S.profile].concat(S.layers).join(' + '))}</div></div>
        <button class="btn btn-ghost btn-icon" data-action="close-dialog" aria-label="Close">${ic('x')}</button></div>
      <div class="dlg-body" id="curl-body">${curlBody()}</div>
      <div class="dlg-foot"><button class="btn" data-action="close-dialog">Cancel</button><button class="btn btn-primary" data-action="curl-copy" autofocus>${ic('copy')}Copy</button></div>
    </div>`);
  };
  A['copy-curl'] = () => VC.curlDialog();
  A['curl-copy'] = () => {
    const withCreds = creds;
    VC.closeDialog();
    VC.copy(curlText(withCreds), withCreds ? 'Copied with a live token' : 'Copied as cURL', withCreds ? 'Include credentials is off again.' : 'Placeholders: $VC_TOKEN');
    creds = false;
  };
  document.addEventListener('change', (e) => {
    if (e.target.id !== 'curl-creds') return;
    creds = e.target.checked;
    $('#curl-body').innerHTML = curlBody();
    $('#curl-creds').focus();
  });

  /* ---- command palette ----------------------------------------------------------- */
  const SCREENS = [['client-auth', 'Auth registry', 'key'], ['history', 'History', 'history'], ['import', 'Import…', 'import'], ['welcome', 'Welcome', 'sparkle'],
    ['tests', 'Tests', 'flow'], ['tests-load', 'Tests · Load', 'chart'],
    ['runs', 'Runs', 'runs'], ['runs/compare', 'Compare runs #481 → #482', 'columns'], ['runs-live', 'Runs · Live run', 'play'], ['runs-load', 'Runs · Load run', 'chart'],
    ['insights', 'Runs · Insights', 'chart'], ['schedules', 'Runs · Schedules', 'clock'], ['agents', 'Agents', 'bot'], ['project-settings', 'Settings', 'gear']];
  const P = { q: '', i: 0, items: [], args: {} };
  function items(q) {
    q = q.trim().toLowerCase();
    const cmdMode = q.startsWith('>') || q.startsWith('vc');
    const qq = q.replace(/^>\s*/, '');
    const match = (s) => !qq || s.toLowerCase().includes(qq);
    const cmds = D.commands.filter((c) => match(c.cmd + ' ' + c.desc)).map((c) => ({ kind: 'cmd', c }));
    if (cmdMode) return cmds;
    const apis = Object.entries(D.apis).filter(([id, a]) => match(id + ' ' + a.name + ' ' + a.path)).map(([id, a]) => ({ kind: 'api', id, a }));
    const scr = SCREENS.filter(([, l]) => match(l)).map(([r, l, i]) => ({ kind: 'go', r, l, i }));
    return apis.slice(0, 8).concat(scr, cmds.slice(0, qq ? 9 : 5));
  }
  const iid = (i) => `pal-${i}`;
  function itemHtml(it, i) {
    const sel = i === P.i;
    const attr = `class="palette-item" role="option" id="${iid(i)}" aria-selected="${sel}" data-action="pal-run" data-i="${i}"`;
    if (it.kind === 'api') return `<div ${attr}>${method(it.a.m)}<span class="ellipsis">${esc(it.a.name)}</span><span class="meta mono">${esc(it.id)}</span></div>`;
    if (it.kind === 'go') return `<div ${attr}>${ic(it.i)}<span>${esc(it.l)}</span><span class="meta">screen</span></div>`;
    return `<div ${attr}>${ic('terminal')}<span class="mono">${esc(it.c.cmd)}</span>${it.c.args.map((a) => `<span class="arg">&lt;${esc(a.n)}&gt;</span>`).join(' ')}<span class="meta">command</span></div>`;
  }
  function listHtml() {
    if (!P.items.length) return `<div class="empty empty-quiet" role="status"><span>No matches</span><span class="subtle">Type <span class="kbd">&gt;</span> for commands only</span></div>`;
    let last = '';
    return P.items.map((it, i) => {
      const g = { api: 'Jump to api', go: 'Screens', cmd: 'Commands' }[it.kind];
      const head = g !== last ? `<div class="pop-label" role="presentation">${g}</div>` : '';
      last = g;
      return head + itemHtml(it, i);
    }).join('');
  }
  function argVal(c, a) { return (P.args[c.cmd] || {})[a.n] !== undefined ? P.args[c.cmd][a.n] : a.v; }
  function cmdLine(c) {
    const flag = { kind: (v) => v, source: (v) => v, selector: (v) => v, code: (v) => v, file: (v) => v, from: (v) => v, to: (v) => v };
    return [c.cmd].concat(c.args.map((a) => (flag[a.n] || ((v) => v))(argVal(c, a)))).join(' ') + (c.cmd === 'vc run' || c.cmd === 'vc plan' ? ` --profile ${S.profile}` : '');
  }
  function previewHtml() {
    const it = P.items[P.i];
    if (!it) return '';
    if (it.kind === 'api') {
      return `<div class="row">${method(it.a.m)}<b>${esc(it.a.name)}</b></div><div class="mono subtle">${hlVars('${baseUrl}' + it.a.path + (it.a.q || ''))}</div>
        <div class="subtle">${esc(VC.fileOf(it.id))}:${it.a.line}</div><div class="row"><span class="kbd">↵</span><span class="subtle">open in a tab</span></div>`;
    }
    if (it.kind === 'go') return `<div class="row">${ic(it.i)}<b>${esc(it.l)}</b></div><div class="subtle">#${esc(it.r)}</div><div class="row"><span class="kbd">↵</span><span class="subtle">go</span></div>`;
    const c = it.c;
    return `<div><b class="mono">${esc(c.cmd)}</b><div class="subtle">${esc(c.desc)}</div></div>
      ${c.args.map((a, k) => `<div class="field"><label for="pal-arg-${k}">${esc(a.n)} <span class="subtle">· ${esc(a.t)}</span></label><input class="input input-sm mono" id="pal-arg-${k}" data-arg="${esc(a.n)}" value="${esc(argVal(c, a))}"></div>`).join('')}
      <pre class="code glass-dense wrap" id="pal-line">$ ${esc(cmdLine(c))}</pre>
      <div class="callout" data-tone="info">${ic('info')}<span>${esc(c.preview)}</span></div>
      <div class="row"><span class="kbd">↵</span><span class="subtle">run</span>${c.args.length ? '<span class="kbd">Tab</span><span class="subtle">edit args</span>' : ''}</div>`;
  }
  function paint() {
    const list = $('#pal-list'), prev = $('#pal-preview'), inp = $('#pal-input');
    list.innerHTML = listHtml();
    prev.innerHTML = previewHtml();
    inp.setAttribute('aria-activedescendant', P.items.length ? iid(P.i) : '');
    const s = $('#' + iid(P.i));
    if (s) s.scrollIntoView({ block: 'nearest' });
  }
  VC.openPalette = function (q) {
    const d = $('#palette');
    P.q = q || ''; P.i = 0; P.items = items(P.q);
    d.innerHTML = `<div class="dlg">
      <div class="palette-input">${ic('search')}<input id="pal-input" role="combobox" aria-expanded="true" aria-controls="pal-list" aria-autocomplete="list" aria-label="Search apis, screens and vc commands" placeholder="Jump to an api or screen · type > for vc commands" value="${esc(P.q)}" autocomplete="off" spellcheck="false"><span class="kbd">esc</span></div>
      <div class="palette-body"><div class="palette-list" id="pal-list" role="listbox" aria-label="Results"></div><div class="palette-preview" id="pal-preview" aria-live="polite"></div></div>
    </div>`;
    if (!d.open) d.showModal();
    paint();
    $('#pal-input').focus();
  };
  function run(i) {
    const it = P.items[i];
    if (!it) return;
    $('#palette').close();
    if (it.kind === 'api') return VC.openApi(it.id);
    if (it.kind === 'go') return VC.goView(it.r);
    const c = it.c;
    if (c.cmd === 'vc import') return VC.go('import');
    if (c.cmd === 'vc run') { VC.toast(`$ ${cmdLine(c)}`, 'Running 6 tests'); return VC.go(VC.screens['runs-live'] ? 'runs-live' : 'runs'); }
    VC.toast(`$ ${cmdLine(c)}`, c.preview);
  }
  A['pal-run'] = (el) => run(+el.dataset.i);
  document.addEventListener('input', (e) => {
    if (e.target.id === 'pal-input') { P.q = e.target.value; P.items = items(P.q); P.i = 0; paint(); return; }
    if (e.target.dataset.arg && e.target.closest('#palette')) {
      const c = P.items[P.i].c;
      P.args[c.cmd] = P.args[c.cmd] || {};
      P.args[c.cmd][e.target.dataset.arg] = e.target.value;
      $('#pal-line').textContent = '$ ' + cmdLine(c);
    }
  });
  document.addEventListener('keydown', (e) => {
    if (!e.target.closest || !e.target.closest('#palette')) return;
    const n = P.items.length;
    if (e.target.id === 'pal-input') {
      if (e.key === 'ArrowDown' && n) { e.preventDefault(); P.i = (P.i + 1) % n; paint(); }
      else if (e.key === 'ArrowUp' && n) { e.preventDefault(); P.i = (P.i - 1 + n) % n; paint(); }
      else if (e.key === 'Enter') { e.preventDefault(); run(P.i); }
      else if (e.key === 'Tab' && !e.shiftKey && $('#pal-arg-0')) { e.preventDefault(); $('#pal-arg-0').select(); $('#pal-arg-0').focus(); }
    } else if (e.target.dataset.arg && e.key === 'Enter') { e.preventDefault(); run(P.i); }
  });

  /* ---- shortcut sheet (?) --------------------------------------------------------- */
  const KEYS = [
    ['Navigate', [['Command palette', ['⌘', 'K']], ['Client · Tests · Runs · Agents', ['⌘', '1–4']], ['Settings', ['⌘', ',']], ['Run tests', ['⌘', 'R']], ['Shortcuts', ['?']], ['Close drawer / menu', ['esc']]]],
    ['Tabs', [['New scratch tab', ['⌘', 'T']], ['Close tab', ['⌘', 'W']], ['Next tab', ['⌃', 'Tab']], ['Previous tab', ['⌃', '⇧', 'Tab']]]],
    ['Request', [['Send', ['⌘', '↵']], ['Switch tab', ['←', '→']], ['Resize split', ['←', '→']], ['Resize faster', ['⇧', '←']]]],
    ['Tree and lists', [['Move', ['↑', '↓']], ['Expand / collapse', ['→', '←']], ['Open', ['↵']], ['First / last', ['Home', 'End']]]]
  ];
  VC.openShortcuts = function () {
    VC.dialog(`<div class="dlg">
      <div class="dlg-head"><div class="grow"><h2 id="dlg-title">Keyboard shortcuts</h2><div class="sub" title="Browsers may keep ⌘T and ⌘W; Alt+T and Alt+W also work">⌘ is Ctrl on Windows and Linux.</div></div>
        <button class="btn btn-ghost btn-icon" data-action="close-dialog" aria-label="Close" autofocus>${ic('x')}</button></div>
      <div class="dlg-body"><div class="keys">${KEYS.map(([g, rows]) => `<section><h3 class="section-label">${g}</h3><dl>${rows.map(([l, ks]) => `<dt>${l}</dt><dd>${ks.map((k) => `<span class="kbd">${k}</span>`).join('')}</dd>`).join('')}</dl></section>`).join('')}</div></div>
    </div>`);
  };
  A.shortcuts = () => VC.openShortcuts();
  A.palette = () => VC.openPalette();
})();
