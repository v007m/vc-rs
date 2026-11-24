/* Client · streams: WebSocket (#client-ws) and server-sent events (#client-sse).
   A live feed ticks every ~2 s while connected. Ticks append rows to the list
   (or to the pause buffer); they never call VC.render(). */
(function () {
  'use strict';
  const VC = window.VC, D = VC.data, S = VC.state, A = VC.actions;
  const { $, esc, ic, method, hlJson, hlYaml } = VC.h;

  const mk = (src) => ({
    connected: true, paused: false, buffer: [], filter: '', dir: 'all', open: new Set(), feed: 0,
    msgs: src.transcript.map((m) => Object.assign({}, m)), reconnect: true, lastId: '103', draft: ''
  });
  const ST = { ws: mk(D.ws), sse: mk(D.sse) };
  ST.ws.draft = D.ws.snippets[1].body;
  const kindOf = () => (S.route === 'client-sse' ? 'sse' : 'ws');
  const cur = () => ST[kindOf()];

  /* ---- rows -------------------------------------------------------------------- */
  const DIR = { in: ['down', 'received'], out: ['up', 'sent'], sys: ['dots', 'connection'] };
  const preview = (m) => (typeof m.data === 'string' ? m.data : JSON.stringify(m.data));
  const fmtSize = (n) => (n ? (n < 1024 ? n + ' B' : (n / 1024).toFixed(1) + ' KB') : '—');
  function matches(st, m) {
    if (st.dir !== 'all' && m.dir !== st.dir) return false;
    const q = st.filter.trim().toLowerCase();
    return !q || (m.kind + ' ' + preview(m)).toLowerCase().includes(q);
  }
  function rowHtml(m, i, st, isNew) {
    const open = st.open.has(i);
    const [ico, label] = DIR[m.dir];
    const kind = kindOf() === 'sse' && m.id ? `${esc(m.kind)} <span class="id">#${esc(m.id)}</span>` : esc(m.kind);
    const body = typeof m.data === 'string' ? `<pre class="code glass-dense wrap">${esc(m.data)}</pre>` : `<pre class="code glass-dense">${hlJson(m.data)}</pre>`;
    return `<li class="msg${m.dir === 'sys' ? ' is-sys' : ''}${open ? ' is-open' : ''}${isNew ? ' is-new' : ''}" data-i="${i}">
      <button aria-expanded="${open}" data-action="msg-toggle" data-i="${i}" data-fk="msg-${i}"><span class="t">${esc(m.t.slice(0, 12))}</span><span class="dir ${m.dir}">${ic(ico)}<span class="sr-only">${label}</span></span><span class="sz">${fmtSize(m.size)}</span><span class="kind">${kind}</span><span class="pv">${esc(preview(m))}</span></button>
      ${open ? `<div class="msg-body">${body}<div class="row"><button class="btn btn-sm" data-action="msg-check" data-i="${i}">${ic('check')}Check this message</button><button class="btn btn-ghost btn-sm" data-action="toast" data-msg="Message copied">${ic('copy')}Copy</button></div></div>` : ''}
    </li>`;
  }
  function listHtml(st) {
    const rows = st.msgs.map((m, i) => (matches(st, m) ? rowHtml(m, i, st) : '')).join('');
    return rows || `<li class="empty">${ic('filter')}<b>No messages match</b><span>Clear the filter to see all ${st.msgs.length}.</span></li>`;
  }
  const countText = (st) => `${st.msgs.length} ${kindOf() === 'ws' ? 'message' : 'event'}${st.msgs.length === 1 ? '' : 's'}${st.buffer.length ? ` · ${st.buffer.length} buffered` : ''}`;
  function msgsCard(st, title) {
    return `<section class="glass msgs-card" aria-label="${esc(title)}">
      <div class="card-head"><h2 class="card-title">${esc(title)}</h2><span class="grow"></span>
        <span class="conn-state"><span class="dot ${st.connected ? 'st-pass is-live' : 'st-neutral'}"></span>${st.connected ? (kindOf() === 'ws' ? 'Open · ws 13' : 'Open · text/event-stream') : 'Closed'}</span>
        ${VC.orientToggle()}</div>
      <div class="card-body">
        <div class="msgs-tools">
          <div class="input-group">${ic('filter')}<input class="input input-sm" id="msg-filter" value="${esc(st.filter)}" placeholder="Filter by kind or text" aria-label="Filter messages" data-fk="msg-filter"></div>
          ${kindOf() === 'ws' ? `<div class="seg seg-sm" role="group" aria-label="Direction">${[['all', 'All'], ['in', 'In'], ['out', 'Out']].map(([k, l]) => `<button aria-pressed="${st.dir === k}" data-action="msg-dir" data-v="${k}" data-fk="dir-${k}">${l}</button>`).join('')}</div>` : ''}
          <button class="btn btn-sm" data-action="msg-pause" aria-pressed="${st.paused}" data-fk="pause">${ic(st.paused ? 'play' : 'pause')}${st.paused ? 'Resume' : 'Pause'}</button>
          <span class="subtle" id="msg-count" aria-live="polite">${countText(st)}</span>
        </div>
        ${st.paused ? `<div class="paused-bar" id="paused-bar">${ic('pause')}<span>Paused. New messages are buffered: <b id="buf-n">${st.buffer.length}</b></span></div>` : ''}
        <ul class="msgs${kindOf() === 'sse' ? ' is-sse' : ''}" id="msgs" data-sk="msgs" aria-label="Messages, oldest first">${listHtml(st)}</ul>
      </div>
    </section>`;
  }

  /* ---- live feed ---------------------------------------------------------------- */
  let timer = null;
  /* fake clock: continues from the last message so the transcript reads as one session */
  function now() {
    const last = cur().msgs[cur().msgs.length - 1].t.split(/[:.]/).map(Number);
    const ms = ((last[0] * 60 + last[1]) * 60 + last[2]) * 1000 + last[3] + 1800 + Math.floor(Math.random() * 900);
    const p = (n, w) => String(n).padStart(w || 2, '0');
    return `${p(Math.floor(ms / 3600000) % 24)}:${p(Math.floor(ms / 60000) % 60)}:${p(Math.floor(ms / 1000) % 60)}.${p(ms % 1000, 3)}`;
  }
  function tick() {
    const k = kindOf(), st = ST[k];
    if (!st.connected) return;
    const f = D[k].feed[st.feed++ % D[k].feed.length];
    const m = k === 'ws'
      ? { t: now(), dir: 'in', kind: 'text', size: JSON.stringify(f).length, data: f }
      : { t: now(), dir: 'in', kind: f.kind, id: String(+st.lastId + 1), size: JSON.stringify(f.data).length + f.kind.length + 12, data: f.data };
    if (k === 'sse') st.lastId = m.id;
    if (st.paused) { st.buffer.push(m); const b = $('#buf-n'); if (b) b.textContent = st.buffer.length; }
    else append(st, m);
    const c = $('#msg-count'); if (c) c.textContent = countText(st);
  }
  function append(st, m) {
    st.msgs.push(m);
    const ul = $('#msgs');
    if (!ul || !matches(st, m)) return;
    const atEnd = ul.scrollHeight - ul.scrollTop - ul.clientHeight < 40;
    if (ul.querySelector(':scope > .empty')) ul.innerHTML = '';
    ul.insertAdjacentHTML('beforeend', rowHtml(m, st.msgs.length - 1, st, true));
    if (atEnd) ul.scrollTop = ul.scrollHeight;
  }
  const start = () => { clearInterval(timer); timer = setInterval(tick, 2200); };
  const stop = () => { clearInterval(timer); timer = null; };
  const refreshList = () => { const st = cur(); const ul = $('#msgs'); if (ul) ul.innerHTML = listHtml(st); const c = $('#msg-count'); if (c) c.textContent = countText(st); };

  A['msg-toggle'] = (el) => {
    const st = cur(), i = +el.dataset.i;
    if (st.open.has(i)) st.open.delete(i); else st.open.add(i);
    const li = el.closest('.msg');
    li.outerHTML = rowHtml(st.msgs[i], i, st);
    const b = $(`#msgs [data-fk="msg-${i}"]`); if (b) b.focus();
  };
  A['msg-dir'] = (el) => { cur().dir = el.dataset.v; VC.render(); };
  A['msg-pause'] = () => {
    const st = cur();
    st.paused = !st.paused;
    if (!st.paused && st.buffer.length) { const n = st.buffer.length; st.msgs.push(...st.buffer); st.buffer = []; VC.render(); VC.toast(`${n} buffered message${n === 1 ? '' : 's'} added`); return; }
    VC.render();
  };
  A['ws-conn'] = () => {
    const st = cur();
    st.connected = !st.connected;
    st.msgs.push({ t: now(), dir: 'sys', kind: st.connected ? 'open' : 'close', size: 0, data: st.connected ? '101 Switching Protocols' : 'close 1000 · normal closure (you)' });
    VC.render();
  };
  document.addEventListener('input', (e) => {
    if (e.target.id === 'msg-filter') { cur().filter = e.target.value; refreshList(); }
    if (e.target.id === 'ws-draft') ST.ws.draft = e.target.value;
    if (e.target.id === 'sse-last') ST.sse.lastId = e.target.value;
  });
  document.addEventListener('change', (e) => { if (e.target.id === 'sse-reconnect') ST.sse.reconnect = e.target.checked; });

  /* ---- WebSocket ---------------------------------------------------------------- */
  function wsSend() {
    const st = ST.ws;
    if (!st.connected) return VC.toast('Not connected', 'Connect first, then send.', 'warn');
    let data;
    try { data = JSON.parse(st.draft); } catch (e) { return VC.toast('Not valid JSON', 'Sent as text instead would need type: text. Fix the JSON or pick a snippet.', 'warn'); }
    append(st, { t: now(), dir: 'out', kind: 'text', size: st.draft.length, data });
    const c = $('#msg-count'); if (c) c.textContent = countText(st);
    if (data.type === 'say') setTimeout(() => { append(st, { t: now(), dir: 'in', kind: 'text', size: 60, data: { kind: 'message', id: st.msgs.length, from: 'ada', text: data.text } }); }, 300);
  }
  A['ws-snippet'] = (el) => { ST.ws.draft = D.ws.snippets[+el.dataset.i].body; const t = $('#ws-draft'); if (t) { t.value = ST.ws.draft; t.focus(); } };
  function wsComposer() {
    const st = ST.ws;
    return `<section class="glass" aria-label="Composer">
      <div class="card-head"><h2 class="card-title">Send</h2><span class="grow"></span><span class="subtle">text frame · JSON</span></div>
      <div class="card-body">
        <div class="composer"><label class="section-label" for="ws-draft">Message</label>
          <textarea class="textarea mono" id="ws-draft" spellcheck="false" data-fk="ws-draft">${esc(st.draft)}</textarea>
          <div class="row"><span class="subtle">⌘↵ sends · \${vars} resolve under ${esc(S.profile)}</span><span class="grow"></span><button class="btn btn-primary btn-sm" data-action="send">${ic('send')}Send</button></div></div>
        <section class="layer"><div class="layer-head"><h3 class="section-label">Snippets</h3><span class="subtle">from rooms.api.yaml · messages</span></div>
          <div class="snippets glass-dense">${D.ws.snippets.map((s, i) => `<button class="snippet" data-action="ws-snippet" data-i="${i}">${ic('code')}<span>${esc(s.name)}</span><code>${esc(s.body)}</code></button>`).join('')}</div></section>
        <section class="layer"><div class="layer-head"><h3 class="section-label">Make it a test</h3></div>
          <p class="subtle">Turns what you sent and what came back into an <code>exchange</code> with message checks.</p>
          <div class="row"><button class="btn" data-action="ws-exchange" data-fk="ws-exchange">${ic('wand')}Turn into exchange check</button></div></section>
      </div></section>`;
  }
  A['ws-exchange'] = () => VC.diffDialog({
    title: 'Turn into exchange check', sub: 'Your 2 sends become send steps; the replies after each become receive checks.',
    file: 'services/rooms/lobby.test.yaml',
    lines: [['@', '@@ -1,4 +1,17 @@'], [' ', 'tests:'], [' ', '  - id: echoesChatMessage'], [' ', '    name: Echoes a chat message'], [' ', '    call: rooms#join'],
      ['+', '    exchange:'], ['+', '      - send: { type: join, name: ada, room: lobby }'], ['+', '      - receive:'], ['+', '          - { type: text, data: { match: { kind: welcome, room: lobby } } }'],
      ['+', '      - send: { type: say, text: hello lobby }'], ['+', '      - receive:'], ['+', '          - { type: text, data: { match: { kind: message, text: hello lobby } } }'],
      ['+', '    expect:'], ['+', '      messages:'], ['+', '        none: { type: close, code: { ne: 1000 } }']],
    callout: 'history and presence messages are left out: they vary between runs. Add them back with some: if they matter.',
    confirm: 'Write test'
  });
  A['msg-check'] = (el) => {
    const m = cur().msgs[+el.dataset.i];
    const sse = kindOf() === 'sse';
    const keys = m.data && typeof m.data === 'object' ? Object.entries(m.data).slice(0, 2).map(([k, v]) => `${k}: ${typeof v === 'string' ? v : JSON.stringify(v)}`).join(', ') : '';
    VC.diffDialog({
      title: 'Check this message', sub: 'Adds a some: check that passes when any message matches.',
      file: sse ? 'services/payments/events.test.yaml' : 'services/rooms/lobby.test.yaml',
      lines: [['@', '@@ -9,3 +9,5 @@'], [' ', '    expect:'], [' ', '      messages:'],
        ['+', sse ? `        some: { event: ${m.kind}, data: { match: { ${keys} } } }` : `        some: { type: text, data: { match: { ${keys} } } }`]],
      confirm: 'Write test'
    });
  };

  VC.screens['client-ws'] = {
    enter: () => { S.api = 'rooms#join'; },
    crumbs: () => [D.project.name, 'rooms', D.apis['rooms#join'].name],
    status: () => (ST.ws.connected ? '<span class="dot st-pass is-live"></span> connected · ws 13 · rooms#join' : 'disconnected'),
    render: () => {
      const st = ST.ws;
      return `<div class="client">${VC.reqbar({ m: 'WS', typeLabel: 'WebSocket', url: D.ws.url,
        extra: `<button class="btn ${st.connected ? 'btn-danger' : ''}" data-action="ws-conn" data-fk="ws-conn">${ic(st.connected ? 'unplug' : 'plug')}${st.connected ? 'Disconnect' : 'Connect'}</button>` })}
        <div class="split"${VC.splitAttrs()}>${msgsCard(st, 'Messages')}${VC.divider('Resize messages and composer')}${wsComposer()}</div></div>`;
    },
    mount: (v) => { VC.mountSplit(v); start(); },
    unmount: stop,
    send: wsSend
  };

  /* ---- SSE ---------------------------------------------------------------------- */
  function sseSide() {
    const st = ST.sse;
    return `<section class="glass" aria-label="Stream options">
      <div class="card-head"><h2 class="card-title">Stream</h2><span class="grow"></span><span class="subtle">payments#events · payments.api.yaml:52</span></div>
      <div class="card-body">
        <label class="switch"><input type="checkbox" id="sse-reconnect" ${st.reconnect ? 'checked' : ''}><span class="track"></span><span>Reconnect on drop <span class="subtle">· sse.reconnect</span></span></label>
        <div class="field"><label for="sse-last">Last-Event-ID</label><input class="input mono" id="sse-last" value="${esc(st.lastId)}" data-fk="sse-last"><span class="hint">Sent on reconnect, so the server resumes after this id. Server asked for retry: 3000 ms.</span></div>
        <section class="layer"><div class="layer-head"><h3 class="section-label">Fields per event</h3></div>
          <div class="glass-dense kv is-readonly" role="table" aria-label="SSE fields">${[['event', 'charge.failed'], ['data', '{ charge, code, decline }'], ['id', '102'], ['retry', '3000'], ['text', 'raw data before JSON parsing']].map(([k, v]) => `<div class="kv-row" role="row"><span class="k mono" role="cell">${k}</span><span class="v" role="cell"><span>${esc(v)}</span></span><span class="s" role="cell"></span></div>`).join('')}</div></section>
        <section class="layer"><div class="layer-head"><h3 class="section-label">Make it a test</h3></div>
          <pre class="code glass-dense wrap">${hlYaml('expect:\n  messages:\n    some: { event: charge.failed, data: { match: { code: card_declined } } }\n    none: { event: error }')}</pre>
          <div class="row"><button class="btn" data-action="sse-check" data-fk="sse-check">${ic('wand')}Turn into messages check</button></div></section>
      </div></section>`;
  }
  A['sse-check'] = () => VC.diffDialog({
    title: 'Turn into messages check', sub: 'Waits up to 5 s for the stream, then checks every event it saw.',
    file: 'services/payments/events.test.yaml', isNew: true,
    lines: [['@', '@@ -0,0 +1,11 @@'], ['+', 'tests:'], ['+', '  - id: streamsDeclinedCharge'], ['+', '    name: Streams a declined charge'], ['+', '    call: payments#events'],
      ['+', '    with:'], ['+', '      sse: { reconnect: true, until: { count: 3, within: 5s } }'], ['+', '    expect:'], ['+', '      messages:'],
      ['+', '        some: { event: charge.failed, data: { match: { code: card_declined } } }'], ['+', '        none: { event: error }']],
    confirm: 'Create file'
  });
  VC.screens['client-sse'] = {
    enter: () => { S.api = 'payments#events'; },
    crumbs: () => [D.project.name, 'payments', D.apis['payments#events'].name],
    status: () => (ST.sse.connected ? `<span class="dot st-pass is-live"></span> streaming · last id ${esc(ST.sse.lastId)}` : 'closed'),
    render: () => {
      const st = ST.sse;
      return `<div class="client">${VC.reqbar({ m: 'SSE', typeLabel: 'Server-sent events', url: D.sse.url, sendLabel: st.connected ? 'Disconnect' : 'Connect', sendIco: st.connected ? 'unplug' : 'plug' })}
        <div class="split"${VC.splitAttrs()}>${msgsCard(st, 'Events')}${VC.divider('Resize events and options')}${sseSide()}</div></div>`;
    },
    mount: (v) => { VC.mountSplit(v); start(); },
    unmount: stop,
    send: () => { A['ws-conn'](); }
  };
})();
