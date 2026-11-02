class Component extends DCLogic {
  static lib() {
    if (!Component._L) Component._L = (function () {
/*__LIB__*/
    })();
    return Component._L;
  }

  constructor(props) {
    super(props);
    const L = Component.lib();
    this.timers = [];
    this.runTimers = {};
    const catalog = L.initialCatalog();
    const config = L.initialConfig();
    const flows = L.initialFlows();
    flows[0].testName = 'Checks out a two-item cart';
    const env = { catalog: catalog, config: config, profile: 'staging' };
    const saved = {
      userId: { value: 'u_7f3k29qp', from: 'users.create', ago: '12 min ago' },
      cartId: { value: 'c_51Hx9w', from: 'carts.create', ago: '9 min ago' },
      orderId: { value: 'ord_8Kq2Tz', from: 'orders.place', ago: '6 min ago' },
      exportId: { value: 'exp_20QmT1', from: 'users.startExport', ago: '1 h ago' }
    };
    const sv = L.savedValues(saved);
    const t1 = L.reqFromApi(L.findApi(catalog, 'users.get'), 't1');
    t1.response = L.execRequest(t1, env, sv);
    const t2 = L.reqFromApi(L.findApi(catalog, 'orders.place'), 't2');
    t2.response = L.execRequest(t2, env, sv);
    t2.respTab = 'checks';
    t2.reqTab = 'body';
    const t3 = this.makeFlowTab(flows[0], catalog, 't3');
    const t4 = L.reqFromApi(L.findApi(catalog, 'users.create'), 't4');
    t4.reqTab = 'vars';
    this.state = {
      profile: 'staging', catalog: catalog, config: config, flows: flows, saved: saved,
      history: [
        { id: 'h1', method: 'POST', apiId: 'orders.place', status: 201, failed: 1, ago: '6 min ago' },
        { id: 'h2', method: 'POST', apiId: 'carts.addItem', status: 201, failed: 0, ago: '8 min ago' },
        { id: 'h3', method: 'GET', apiId: 'users.get', status: 200, failed: 0, ago: '12 min ago' },
        { id: 'h4', method: 'POST', apiId: 'users.create', status: 201, failed: 0, ago: '12 min ago' },
        { id: 'h5', method: 'POST', apiId: 'catalog.seedProducts', status: 503, failed: 1, ago: '1 h ago' }
      ],
      collapsed: { orders: true, carts: true, catalog: true, geo: true, payments: true },
      sideQ: '', tabs: [t1, t2, t3, t4], activeKey: 't1', seq: 5, flowSeq: 1,
      flowRuns: {}, dialog: null, imp: null, toast: null, picker: null, sideCollapsed: false
    };
  }

  static get KNOWN_TESTS() {
    return { 'users.crud': ['createsMember', 'rejectsBadEmail', 'getsCreatedUser'], 'orders.checkout': ['placesOrder', 'appliesCoupon'], 'carts.pricing': ['appliesBulkDiscount'] };
  }

  componentWillUnmount() {
    this.timers.forEach((id) => clearTimeout(id));
    this.timers = [];
    this.runTimers = {};
  }

  later(fn, ms) {
    const id = setTimeout(() => { this.timers = this.timers.filter((x) => x !== id); fn(); }, ms);
    this.timers.push(id);
    return id;
  }

  /* ---------- helpers ---------- */
  env() { return { catalog: this.state.catalog, config: this.state.config, profile: this.state.profile }; }
  tab() { return this.state.tabs.filter((t) => t.key === this.state.activeKey)[0] || null; }
  flowById(id) { return this.state.flows.filter((f) => f.id === id)[0] || null; }
  nextKey() { const n = this.state.seq; return { key: 't' + n, seq: n + 1 }; }
  patchTab(key, patch) {
    this.setState({ tabs: this.state.tabs.map((t) => (t.key === key ? Object.assign({}, t, typeof patch === 'function' ? patch(t) : patch) : t)) });
  }
  patchCur(patch) { const t = this.tab(); if (t) this.patchTab(t.key, patch); }
  patchFlow(id, patch, keepRun) {
    const flows = this.state.flows.map((f) => (f.id === id ? Object.assign({}, f, typeof patch === 'function' ? patch(f) : patch, { dirty: true }) : f));
    const st = { flows: flows };
    if (!keepRun && this.state.flowRuns[id]) {
      this.clearRun(id);
      const fr = Object.assign({}, this.state.flowRuns);
      delete fr[id];
      st.flowRuns = fr;
    }
    this.setState(st);
  }
  toast(text) {
    if (this.toastTimer) clearTimeout(this.toastTimer);
    this.setState({ toast: { text: text } });
    this.toastTimer = this.later(() => this.setState({ toast: null }), 3600);
  }
  methodBits(m) { m = m || 'GET'; return { mshort: m === 'DELETE' ? 'DEL' : m, mcls: 'm-' + m.toLowerCase() }; }
  fmtMs(ms) { return ms >= 1000 ? (ms / 1000).toFixed(1) + ' s' : ms + ' ms'; }
  makeFlowTab(flow, catalog, key) {
    const L = Component.lib();
    const st = flow.stages[0];
    const found = st ? L.findApi(catalog, st.api) : null;
    const r = found ? L.reqFromApi(found, key) : L.blankReq(key);
    if (st) { r.callVars = L.clone(st.vars || {}); r.stageUid = st.uid; }
    r.kind = 'flow';
    r.mode = 'flow';
    r.flowId = flow.id;
    return r;
  }
  savedFor(tab) {
    const L = Component.lib();
    if (tab && tab.stageUid && tab.flowId) {
      const flow = this.flowById(tab.flowId);
      const fr = this.state.flowRuns[tab.flowId];
      if (!flow) return {};
      const idx = flow.stages.findIndex((s) => s.uid === tab.stageUid);
      let snap = {};
      if (fr) for (let i = idx - 1; i >= 0; i--) { const u = flow.stages[i].uid; if (fr.shown[u] && fr.snapshots[u]) { snap = fr.snapshots[u]; break; } }
      const out = {};
      Object.keys(snap).forEach((k) => { if (k.indexOf('__') !== 0) out[k] = snap[k]; });
      return out;
    }
    return L.savedValues(this.state.saved);
  }
  stageOf(tab) {
    if (!tab || !tab.stageUid || !tab.flowId) return null;
    const flow = this.flowById(tab.flowId);
    if (!flow) return null;
    const i = flow.stages.findIndex((s) => s.uid === tab.stageUid);
    return i < 0 ? null : { flow: flow, stage: flow.stages[i], index: i };
  }
  stageItem(tab) {
    const L = Component.lib();
    const so = this.stageOf(tab);
    if (!so) return undefined;
    const items = L.loopItems(so.stage);
    return items ? items[0] : undefined;
  }

  /* ---------- actions: tabs ---------- */
  setProfile(p) {
    this.setState({ profile: p });
    this.toast('Profile ' + p + ': urls and auth now resolve for ' + p + '.');
  }
  openApi(fullId, response) {
    const L = Component.lib();
    const ex = this.state.tabs.filter((t) => t.kind === 'request' && t.apiId === fullId && !t.stageUid)[0];
    if (ex && !response) { this.setState({ activeKey: ex.key, tabs: this.state.tabs.map((t) => (t.key === ex.key ? Object.assign({}, t, { mode: 'request' }) : t)) }); return; }
    const found = L.findApi(this.state.catalog, fullId);
    if (!found) return;
    const k = this.nextKey();
    const t = L.reqFromApi(found, k.key);
    if (response) t.response = response;
    this.setState({ tabs: this.state.tabs.concat([t]), activeKey: k.key, seq: k.seq });
  }
  openFlow(flowId) {
    const ex = this.state.tabs.filter((t) => t.flowId === flowId)[0];
    if (ex) { this.setState({ activeKey: ex.key, tabs: this.state.tabs.map((t) => (t.key === ex.key ? Object.assign({}, t, { mode: 'flow' }) : t)) }); return; }
    const flow = this.flowById(flowId);
    const k = this.nextKey();
    const t = this.makeFlowTab(flow, this.state.catalog, k.key);
    this.setState({ tabs: this.state.tabs.concat([t]), activeKey: k.key, seq: k.seq });
  }
  newTab() {
    const L = Component.lib();
    const k = this.nextKey();
    this.setState({ tabs: this.state.tabs.concat([L.blankReq(k.key)]), activeKey: k.key, seq: k.seq });
  }
  closeTab(key) {
    const tabs = this.state.tabs;
    const i = tabs.findIndex((t) => t.key === key);
    const rest = tabs.filter((t) => t.key !== key);
    let active = this.state.activeKey;
    if (active === key) active = rest.length ? rest[Math.max(0, i - 1)].key : null;
    this.setState({ tabs: rest, activeKey: active });
  }
  toMode(mode) {
    const t = this.tab();
    if (!t) return;
    if (mode === 'flow' && !t.flowId) {
      const L = Component.lib();
      const n = this.state.flowSeq;
      const id = 'draftFlow' + n;
      const stages = t.apiId ? [{ uid: id + 's0', api: t.apiId, vars: L.clone(t.callVars || {}) }] : [];
      const flow = { id: id, name: 'Untitled flow ' + n, stopOnFail: true, stages: stages, dirty: true };
      this.setState({ flows: this.state.flows.concat([flow]), flowSeq: n + 1, tabs: this.state.tabs.map((x) => (x.key === t.key ? Object.assign({}, x, { mode: 'flow', flowId: id, stageUid: stages.length ? stages[0].uid : null }) : x)) });
      return;
    }
    this.patchTab(t.key, { mode: mode });
  }
  openStage(uid) {
    const L = Component.lib();
    const t = this.tab();
    const flow = this.flowById(t.flowId);
    const st = flow.stages.filter((s) => s.uid === uid)[0];
    const found = L.findApi(this.state.catalog, st.api);
    if (!found) return;
    const r = L.reqFromApi(found, t.key);
    r.callVars = L.clone(st.vars || {});
    const fr = this.state.flowRuns[flow.id];
    const res = fr && fr.shown[uid] ? fr.results[uid] : null;
    this.patchTab(t.key, Object.assign(r, { kind: t.kind, flowId: t.flowId, stageUid: uid, stagePicked: true, mode: 'request', name: t.name, response: res && res.response ? res.response : null }));
  }
  applyStageVars() {
    const t = this.tab();
    const so = this.stageOf(t);
    if (!so) return;
    const L = Component.lib();
    this.patchFlow(so.flow.id, (f) => ({ stages: f.stages.map((s) => (s.uid === so.stage.uid ? Object.assign({}, s, { vars: L.clone(t.callVars || {}) }) : s)) }));
    this.toast('Stage ' + (so.index + 1) + ' now uses these vars.');
  }

  /* ---------- actions: request editing ---------- */
  rowsOp(field, op, i, patch) {
    const t = this.tab();
    if (!t) return;
    let rows = (t[field] || []).slice();
    if (op === 'add') rows.push({ k: '', v: '', on: true });
    if (op === 'remove') rows.splice(i, 1);
    if (op === 'edit') rows[i] = Object.assign({}, rows[i], patch);
    this.patchTab(t.key, { [field]: rows });
  }
  setCallVar(name, value) {
    const t = this.tab();
    const cv = Object.assign({}, t.callVars || {});
    if (value === '') delete cv[name]; else cv[name] = value;
    this.patchTab(t.key, { callVars: cv });
  }
  savesOp(op, i, patch) {
    const t = this.tab();
    let rows = (t.saves || []).slice();
    if (op === 'add') rows.push({ name: '', expr: '${response.body.}' });
    if (op === 'remove') rows.splice(i, 1);
    if (op === 'edit') rows[i] = Object.assign({}, rows[i], patch);
    this.patchTab(t.key, { saves: rows });
  }
  addCheck() {
    const t = this.tab();
    const nc = t.newCheck || { kind: 'body', path: '', op: 'eq', value: '' };
    this.patchTab(t.key, { extraChecks: (t.extraChecks || []).concat([Object.assign({}, nc)]), newCheck: { kind: nc.kind, path: nc.kind === 'timing' ? 'total' : '', op: nc.kind === 'timing' ? 'lt' : 'eq', value: '' } });
  }
  setNc(patch) {
    const t = this.tab();
    const nc = Object.assign({ kind: 'body', path: '', op: 'eq', value: '' }, t.newCheck || {}, patch);
    if (patch.kind === 'timing') { nc.path = 'total'; nc.op = 'lt'; }
    if (patch.kind === 'body') { nc.path = ''; nc.op = 'eq'; }
    if (patch.kind === 'status') { nc.op = 'eq'; }
    this.patchTab(t.key, { newCheck: nc });
  }
  send() {
    const L = Component.lib();
    const t = this.tab();
    if (!t || t.sending) return;
    const key = t.key;
    this.patchTab(key, { sending: true });
    const env = this.env();
    const resp = L.execRequest(t, env, this.savedFor(t), { item: this.stageItem(t) });
    const delay = resp.error ? 160 : Math.max(320, Math.min(1100, 220 + resp.timing.total * 1.6));
    this.later(() => {
      const cur = this.state.tabs.filter((x) => x.key === key)[0];
      if (!cur) return;
      const st = { tabs: this.state.tabs.map((x) => (x.key === key ? Object.assign({}, x, { sending: false, response: resp }) : x)) };
      if (!resp.error) {
        const isStage = !!cur.stageUid;
        if (!isStage) {
          const saved = Object.assign({}, this.state.saved);
          resp.saved.forEach((s) => { if (s.ok) saved[s.name] = { value: s.value, from: cur.apiId || 'request', ago: 'just now' }; });
          st.saved = saved;
        }
        const h = { id: 'h' + Date.now(), method: cur.method, apiId: cur.apiId || L.shapeOf((L.splitUrl(resp.url) || { path: resp.url }).path), status: resp.status, failed: resp.failed, ago: 'just now', response: resp };
        st.history = [h].concat(this.state.history).slice(0, 12);
      }
      this.setState(st);
    }, delay);
  }

  /* ---------- actions: flows ---------- */
  clearRun(flowId) {
    (this.runTimers[flowId] || []).forEach((id) => clearTimeout(id));
    this.runTimers[flowId] = [];
  }
  runAll() {
    const L = Component.lib();
    const t = this.tab();
    const flow = this.flowById(t.flowId);
    if (!flow || !flow.stages.length) return;
    this.clearRun(flow.id);
    const res = L.runFlow(flow, this.env());
    const order = flow.stages.map((s) => s.uid);
    this.setState({ flowRuns: Object.assign({}, this.state.flowRuns, { [flow.id]: { results: res.results, snapshots: res.snapshots, shown: {}, running: true, stopped: false, single: null, order: order } }) });
    let acc = 0;
    order.forEach((uid, i) => {
      const r = res.results[uid];
      acc += r.status === 'skipped' ? 140 : Math.max(380, Math.min(1500, 260 + r.time * 0.5));
      const id = this.later(() => {
        const fr = this.state.flowRuns[flow.id];
        if (!fr || !fr.running) return;
        const shown = Object.assign({}, fr.shown, { [uid]: true });
        const done = i === order.length - 1;
        this.setState({ flowRuns: Object.assign({}, this.state.flowRuns, { [flow.id]: Object.assign({}, fr, { shown: shown, running: !done }) }) });
        if (done) {
          const failed = order.filter((u) => res.results[u].status === 'failed').length;
          this.toast(flow.name + ': ' + (failed ? failed + ' stage failed' : 'all stages passed') + '.');
        }
      }, acc);
      (this.runTimers[flow.id] = this.runTimers[flow.id] || []).push(id);
    });
  }
  stopRun() {
    const t = this.tab();
    const fr = this.state.flowRuns[t.flowId];
    this.clearRun(t.flowId);
    if (fr) this.setState({ flowRuns: Object.assign({}, this.state.flowRuns, { [t.flowId]: Object.assign({}, fr, { running: false, stopped: true }) }) });
  }
  runStageOnly(uid) {
    const L = Component.lib();
    const t = this.tab();
    const flow = this.flowById(t.flowId);
    const prev = this.state.flowRuns[flow.id];
    const base = prev ? { results: Object.assign({}, prev.results), snapshots: Object.assign({}, prev.snapshots) } : null;
    if (base) Object.keys(base.results).forEach((u) => { if (!prev.shown[u]) { delete base.results[u]; delete base.snapshots[u]; } });
    const res = L.runFlow(flow, this.env(), uid, base);
    const shown = Object.assign({}, prev ? prev.shown : {});
    delete shown[uid];
    const fr = { results: res.results, snapshots: res.snapshots, shown: shown, running: false, stopped: false, single: uid, order: flow.stages.map((s) => s.uid) };
    this.setState({ flowRuns: Object.assign({}, this.state.flowRuns, { [flow.id]: fr }) });
    const r = res.results[uid];
    const id = this.later(() => {
      const cur = this.state.flowRuns[flow.id];
      if (!cur || cur.single !== uid) return;
      this.setState({ flowRuns: Object.assign({}, this.state.flowRuns, { [flow.id]: Object.assign({}, cur, { single: null, shown: Object.assign({}, cur.shown, { [uid]: true }) }) }) });
    }, Math.max(380, Math.min(1500, 260 + r.time * 0.5)));
    (this.runTimers[flow.id] = this.runTimers[flow.id] || []).push(id);
  }
  moveStage(i, d) {
    const t = this.tab();
    this.patchFlow(t.flowId, (f) => { const s = f.stages.slice(); const x = s[i]; s.splice(i, 1); s.splice(i + d, 0, x); return { stages: s }; });
  }
  removeStage(i) {
    const t = this.tab();
    const flow = this.flowById(t.flowId);
    const uid = flow.stages[i].uid;
    this.patchFlow(t.flowId, (f) => ({ stages: f.stages.filter((_, j) => j !== i) }));
    if (t.stageUid === uid) this.patchTab(t.key, { stageUid: null });
  }
  setLoop(i, kind) {
    const t = this.tab();
    this.patchFlow(t.flowId, (f) => ({ stages: f.stages.map((s, j) => {
      if (j !== i) return s;
      const n = Object.assign({}, s);
      if (kind === 'once') delete n.loop;
      else if (kind === 'repeat2') n.loop = { kind: 'repeat', count: 2 };
      else if (kind === 'repeat3') n.loop = { kind: 'repeat', count: 3 };
      else n.loop = { kind: 'forEach', items: (s.loop && s.loop.items) || (s.api === 'carts.addItem' ? 'SKU-1, SKU-2' : 'a, b'), count: 2 };
      return n;
    }) }));
  }
  setItems(i, v) {
    const t = this.tab();
    this.patchFlow(t.flowId, (f) => ({ stages: f.stages.map((s, j) => (j === i ? Object.assign({}, s, { loop: Object.assign({}, s.loop, { items: v }) }) : s)) }));
  }
  addStage(fullId) {
    const t = this.tab();
    const uid = 'st' + Date.now().toString(36) + Math.floor(Math.random() * 1000);
    this.patchFlow(t.flowId, (f) => ({ stages: f.stages.concat([{ uid: uid, api: fullId, vars: {} }]) }));
    this.setState({ picker: null });
  }
  addToFlow(flowId) {
    const L = Component.lib();
    const t = this.tab();
    let flows = this.state.flows;
    let id = flowId;
    let n = this.state.flowSeq;
    if (!id) {
      id = 'draftFlow' + n;
      flows = flows.concat([{ id: id, name: 'Untitled flow ' + n, stopOnFail: true, stages: [], dirty: true }]);
      n++;
    }
    const uid = 'st' + Date.now().toString(36);
    flows = flows.map((f) => (f.id === id ? Object.assign({}, f, { dirty: true, stages: f.stages.concat([{ uid: uid, api: t.apiId, vars: L.clone(t.callVars || {}) }]) }) : f));
    const fname = flows.filter((f) => f.id === id)[0].name;
    const fr = Object.assign({}, this.state.flowRuns);
    delete fr[id];
    this.clearRun(id);
    const tabs = this.state.tabs.map((x) => (x.key === t.key ? Object.assign({}, x, { flowId: id, mode: 'flow', stageUid: uid }) : x));
    this.setState({ flows: flows, flowSeq: n, dialog: null, tabs: tabs, flowRuns: fr });
    this.toast(t.apiId + ' added as the last stage of ' + fname + '.');
  }

  /* ---------- dialogs ---------- */
  openSaveTest() {
    const L = Component.lib();
    const t = this.tab();
    const found = t.apiId ? L.findApi(this.state.catalog, t.apiId) : null;
    const svc = t.apiId ? t.apiId.split('.')[0] : 'misc';
    const file = L.TEST_FILES.filter((f) => f.id.indexOf(svc + '.') === 0)[0];
    const name = L.thirdPerson(found ? found.api.name : 'Calls the endpoint');
    const exp = L.suggestExpectations(t.response, t.apiId);
    (t.extraChecks || []).forEach((c, i) => {
      if (c.kind === 'body' && c.path) exp.push({ key: 'match.' + c.path, path: c.path, value: c.value, label: 'body.match.' + c.path + ': ' + c.value + '  (added check)', on: true });
    });
    this.setState({ dialog: { kind: 'saveTest', name: name, fileId: file ? file.id : 'new', area: file ? '' : 'smoke', exp: exp, includeVars: true, done: false } });
  }
  saveTestModel(d) {
    const L = Component.lib();
    const t = this.tab();
    const id = L.deriveTestId(d.name);
    const svc = (t.apiId || 'misc.x').split('.')[0];
    const fileId = d.fileId === 'new' ? svc + '.' + (d.area || '').toLowerCase().replace(/[^a-z0-9.]+/g, '') : d.fileId;
    const path = L.testFilePath(fileId);
    const vars = d.includeVars ? L.clone(t.callVars || {}) : {};
    const test = L.testFromRequest({ id: id, name: d.name.trim(), apiId: t.apiId, vars: vars, expectations: d.exp, status: t.response && !t.response.error ? t.response.status : 200, saves: [] });
    const exists = d.fileId !== 'new';
    const yaml = exists ? '# appended under tests: in ' + path + '\n' + L.testBlockYaml(test) : L.toYaml({ id: fileId, name: L.cap(L.humanize(fileId.split('.').slice(1).join(' '))) || 'Tests', tests: [test] });
    let idErr = '';
    if (!d.name.trim()) idErr = 'Type a name: the id is derived from it.';
    else if (!L.validId(id)) idErr = 'Ids are camelCase and start with a letter. Try ' + (L.toCamel(id) || 'createsMember') + '.';
    else if ((Component.KNOWN_TESTS[fileId] || []).indexOf(id) >= 0) idErr = fileId + '.' + id + ' already exists. Name the behaviour this test adds, for example ' + id + 'WithTax.';
    let areaErr = d.fileId === 'new' && !/^[a-z][a-z0-9]*(\.[a-z][a-z0-9]*)*$/.test(d.area || '') ? 'Area is lowercase words joined by dots, like smoke or exports.daily.' : '';
    return { id: id, fileId: fileId, path: path, yaml: yaml, idErr: idErr || areaErr, warn: L.testIdWarning(id), fullId: fileId + '.' + (id || '…') };
  }
  openCurl() {
    const L = Component.lib();
    const t = this.tab();
    const b = L.buildRequest(t, Object.assign({}, this.env(), { item: this.stageItem(t) }), this.savedFor(t));
    this.setState({ dialog: { kind: 'curl', text: L.curlFor(b), copied: false } });
  }
  copyText(text, msg) {
    try { if (navigator && navigator.clipboard) navigator.clipboard.writeText(text).catch(() => {}); } catch (e) { /* clipboard blocked */ }
    this.toast(msg);
  }
  openSaveApi() { this.setState({ dialog: { kind: 'saveApi' } }); }
  saveApiModel() {
    const L = Component.lib();
    const t = this.tab();
    const found = L.findApi(this.state.catalog, t.apiId);
    if (!found) return { err: 'This request is not linked to an api.' };
    const r = L.reqToApi(t, found);
    if (r.error) return { err: r.error, found: found };
    const file2 = L.clone(found.file);
    file2.apis = file2.apis.map((a) => (a.id === found.api.id ? r.api : a));
    const strip = (f) => { const c = L.clone(f); c.apis.forEach((a) => { delete a.isNew; delete a.updated; }); return c; };
    const dv = L.diffView(L.apiFileYaml(strip(found.file)), L.apiFileYaml(strip(file2)));
    return { found: found, api: r.api, dv: dv };
  }
  writeApi() {
    const m = this.saveApiModel();
    if (m.err) return;
    const t = this.tab();
    const fid = m.found.file.id;
    const catalog = this.state.catalog.map((f) => (f.id !== fid ? f : Object.assign({}, f, { apis: f.apis.map((a) => (a.id === m.found.api.id ? Object.assign({}, m.api, { updated: true }) : a)) })));
    this.setState({ catalog: catalog, dialog: null });
    this.toast('Wrote ' + t.apiId + ' to ' + m.found.file.path + '.');
  }
  openFlowTest(lifecycle) {
    const L = Component.lib();
    const t = this.tab();
    const flow = this.flowById(t.flowId);
    const name = flow.testName || L.thirdPerson(flow.name);
    const last = flow.stages[flow.stages.length - 1];
    const svc = last ? last.api.split('.')[0] : 'orders';
    const file = L.TEST_FILES.filter((f) => f.id.indexOf(svc + '.') === 0)[0] || L.TEST_FILES[2];
    const n = Math.max(1, Math.min(flow.stages.length - 1, flow.stages.findIndex((s) => s.api.indexOf(svc + '.') === 0) > 0 ? flow.stages.findIndex((s) => s.api.indexOf(svc + '.') === 0) : flow.stages.length - 1));
    this.setState({ dialog: { kind: 'flowTest', lifecycle: !!lifecycle, name: name, fileId: file.id, n: n } });
  }
  flowTestModel(d) {
    const L = Component.lib();
    const t = this.tab();
    const flow = this.flowById(t.flowId);
    const id = L.deriveTestId(d.name);
    const path = L.testFilePath(d.fileId);
    let yamlRows;
    let previewTitle;
    if (d.lifecycle) {
      const lc = L.flowToLifecycle(flow, d.n, id, d.name.trim());
      const isCheckout = d.fileId === 'orders.checkout';
      const before = isCheckout ? L.CHECKOUT_BEFORE : 'id: ' + d.fileId + '\nname: ' + L.cap(d.fileId.split('.')[1]) + '\ntests: []';
      let after;
      if (isCheckout) {
        const lines = before.split('\n');
        const s = lines.indexOf('beforeEach:');
        const e = lines.indexOf('tests:');
        const be = L.toYaml({ beforeEach: lc.beforeEach }).split('\n');
        after = lines.slice(0, s).concat(be, lines.slice(e)).join('\n') + '\n' + L.testBlockYaml(lc.test);
      } else {
        after = 'id: ' + d.fileId + '\nname: ' + L.cap(d.fileId.split('.')[1]) + '\n' + L.toYaml({ beforeEach: lc.beforeEach }) + '\ntests:\n' + L.testBlockYaml(lc.test);
      }
      yamlRows = L.diffView(before, after).rows;
      previewTitle = 'Changes to ' + path;
    } else {
      const test = L.flowToTest(flow, id, d.name.trim());
      yamlRows = L.codeLines('# appended under tests: in ' + path + '\n' + L.testBlockYaml(test), 'yaml').map((l) => Object.assign(l, { sign: '' }));
      previewTitle = 'YAML to be written';
    }
    let idErr = '';
    if (!d.name.trim()) idErr = 'Type a name: the id is derived from it.';
    else if (!L.validId(id)) idErr = 'Ids are camelCase and start with a letter.';
    else if ((Component.KNOWN_TESTS[d.fileId] || []).indexOf(id) >= 0) idErr = d.fileId + '.' + id + ' already exists. Pick another name.';
    const stagesText = flow.stages.slice(0, d.n).map((s) => s.api).join(', ');
    return { id: id, path: path, rows: yamlRows, previewTitle: previewTitle, idErr: idErr, warn: L.testIdWarning(id), fullId: d.fileId + '.' + (id || '…'), nText: d.n + ' of ' + flow.stages.length + ': ' + stagesText, flow: flow };
  }
  writeFlowTest() {
    const d = this.state.dialog;
    const m = this.flowTestModel(d);
    if (m.idErr) return;
    const t = this.tab();
    this.setState({ dialog: null, flows: this.state.flows.map((f) => (f.id === t.flowId ? Object.assign({}, f, { dirty: false }) : f)) });
    this.toast(d.lifecycle ? 'Moved ' + d.n + ' stages into beforeEach of ' + d.fileId + ' and added ' + m.fullId + '.' : 'Added ' + m.fullId + ' to ' + m.path + '.');
  }

  /* ---------- import ---------- */
  openImport() {
    this.setState({ imp: {
      tab: 'har', step: 1, fileOn: true, f: { hideStatic: true, hideThird: true, hidePreflight: true, included: {} }, excluded: {}, ids: {}, actions: {},
      secretName: 'PAYMENTS_KEY', makeFlow: true, keepReads: false, flowName: 'Recorded checkout', reviewIdx: 0,
      oa: { src: 'url', url: 'https://payments.staging.shop.example/v3/api-docs', hasFile: false, loading: false, loaded: false, off: {} },
      cu: { text: "curl -X POST 'https://orders.staging.shop.example/api/v1/orders/ord_8Kq2Tz/notes' \\\n  -H 'authorization: Bearer eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiJzaG9wIn0.c2ln' \\\n  -H 'content-type: application/json' \\\n  --data-raw '{\"text\":\"Leave at the side door\",\"visibleToCustomer\":true}'" },
      pm: { off: {} }
    } });
  }
  patchImp(patch) { this.setState({ imp: Object.assign({}, this.state.imp, patch) }); }
  harModel() {
    const L = Component.lib();
    const imp = this.state.imp;
    if (!this._har) this._har = L.harEntries();
    const entries = imp.fileOn ? this._har : [];
    const counts = L.harCounts(this._har);
    const visible = L.harVisible(entries, imp.f);
    const selected = L.harSelected(entries, imp.f, imp.excluded);
    const groups = L.harMap(selected, this.state.catalog, { ids: imp.ids, actions: imp.actions });
    const secrets = L.harSecrets(selected, imp.secretName);
    const steps = imp.makeFlow ? L.harFlowSteps(groups, selected, imp.keepReads) : null;
    const flowId = L.deriveTestId(imp.flowName);
    const flowTest = steps ? { id: flowId, name: imp.flowName.trim(), tags: ['recorded'], steps: steps } : null;
    const secretOk = /^[A-Z][A-Z0-9_]*$/.test(imp.secretName);
    const files = L.harFiles(groups, this.state.catalog, { config: this.state.config, secretName: secretOk ? imp.secretName : 'PAYMENTS_KEY', hasKeySecret: secrets.some((s) => s.kind === 'query'), flowTest: flowTest });
    files.files.sort((a, b) => (a.status === b.status ? 0 : a.status === 'new' ? -1 : 1));
    const idErrors = groups.reduce((n, g) => n + g.apis.filter((a) => a.action !== 'skip' && a.idError).length, 0);
    return { entries: entries, counts: counts, visible: visible, selected: selected, groups: groups, secrets: secrets, steps: steps, flowTest: flowTest, flowId: flowId, files: files, idErrors: idErrors, secretOk: secretOk };
  }
  applyHar() {
    const L = Component.lib();
    const m = this.harModel();
    const imp = this.state.imp;
    const config = L.clone(this.state.config);
    m.groups.filter((g) => g.isNew).forEach((g) => { config.profiles.staging[g.service + 'Url'] = 'https://' + g.host; });
    let flows = this.state.flows;
    let tabs = this.state.tabs;
    let active = this.state.activeKey;
    let seq = this.state.seq;
    const collapsed = Object.assign({}, this.state.collapsed);
    m.groups.forEach((g) => { if (g.apis.some((a) => a.action !== 'skip')) collapsed[g.fileId] = false; });
    const added = m.groups.reduce((n, g) => n + g.apis.filter((a) => a.action === 'add').length, 0);
    const updated = m.groups.reduce((n, g) => n + g.apis.filter((a) => a.action === 'update').length, 0);
    const catalog = m.files.catalog;
    if (m.flowTest) {
      const fid = 'recorded' + Date.now().toString(36);
      const flow = { id: fid, name: imp.flowName.trim() || 'Recorded checkout', testName: imp.flowName.trim(), stopOnFail: true, stages: L.stepsToStages(m.steps, fid + 's'), dirty: false };
      flows = flows.filter((f) => f.name !== flow.name).concat([flow]);
      const t = this.makeFlowTab(flow, catalog, 't' + seq);
      tabs = tabs.concat([t]);
      active = t.key;
      seq++;
    }
    this.setState({ catalog: catalog, config: config, flows: flows, tabs: tabs, activeKey: active, seq: seq, imp: null, collapsed: collapsed, flowRuns: this.state.flowRuns });
    this.toast('Imported ' + added + ' new apis' + (updated ? ', updated ' + updated : '') + (m.flowTest ? ', created orders.checkout.' + m.flowId : '') + '. Changes are in your working tree.');
  }
  oaModel() {
    const L = Component.lib();
    const rows = L.openapiMap(L.openapiOps(), this.state.catalog, 'payments');
    return rows;
  }
  applyOa() {
    const L = Component.lib();
    const imp = this.state.imp;
    const rows = this.oaModel().filter((r) => !r.match && !imp.oa.off[r.method + r.path]);
    const catalog = this.state.catalog.map((f) => (f.id !== 'payments' ? f : Object.assign({}, f, { apis: f.apis.concat(rows.map((r) => ({ id: r.id, name: L.nameFromId(r.id, 'payments'), method: r.method, path: r.path, expect: { status: r.method === 'POST' ? 201 : 200 }, isNew: true }))) })));
    this.setState({ catalog: catalog, imp: null, collapsed: Object.assign({}, this.state.collapsed, { payments: false }) });
    this.toast('Imported ' + rows.length + ' apis into payments and wrote openapi/payments.json.');
  }
  curlModel() {
    const L = Component.lib();
    const p = L.parseCurl(this.state.imp.cu.text);
    if (p.error) return { error: p.error };
    const u = L.splitUrl(p.url);
    if (!u) return { error: 'The URL must start with http:// or https://' };
    const service = u.host.split('.')[0];
    const sb = L.splitBase(u.path);
    const tp = L.templatePath(sb.rest, { u_7f3k29qp: 'userId', c_51Hx9w: 'cartId', ord_8Kq2Tz: 'orderId' });
    const file = this.state.catalog.filter((f) => f.id === service)[0];
    const match = file ? file.apis.filter((a) => (a.method || 'GET') === p.method && L.shapeOf(a.path) === L.shapeOf(tp.path))[0] : null;
    const id = match ? match.id : L.deriveApiId(p.method, tp.path, service);
    let body = null;
    try { body = p.body ? JSON.parse(p.body) : null; } catch (e) { body = p.body; }
    return { p: p, u: u, service: service, base: sb.base, tp: tp, file: file, match: match, id: id, body: body };
  }
  curlToTab(importIt) {
    const L = Component.lib();
    const m = this.curlModel();
    if (m.error) return;
    let catalog = this.state.catalog;
    const fullId = m.service + '.' + m.id;
    if (importIt && !m.match) {
      const vars = {};
      let body;
      if (m.body && typeof m.body === 'object') { body = {}; Object.keys(m.body).forEach((k) => { vars[k] = m.body[k]; body[k] = '${var.' + k + '}'; }); }
      const api = { id: m.id, name: L.nameFromId(m.id, m.service), method: m.p.method, path: m.tp.path, isNew: true, expect: { status: m.p.method === 'POST' ? 201 : 200 } };
      if (Object.keys(vars).length) api.vars = vars;
      if (body) api.body = body;
      if (m.file) catalog = catalog.map((f) => (f.id === m.service ? Object.assign({}, f, { apis: f.apis.concat([api]) }) : f));
      else catalog = catalog.concat([{ id: m.service, name: L.cap(m.service), path: 'services/' + m.service + '/' + m.service + '.api.yaml', baseUrl: '${var.' + m.service + 'Url}' + m.base, auth: 'shopOauth', headers: {}, vars: {}, apis: [api], isNew: true }]);
    }
    const k = this.nextKey();
    let t;
    const found = (importIt || m.match) ? L.findApi(catalog, fullId) : null;
    if (found) {
      t = L.reqFromApi(found, k.key);
      m.tp.params.forEach((pp) => { if (!this.state.saved[pp.name]) t.callVars[pp.name] = pp.recorded; });
    } else {
      t = L.blankReq(k.key);
      t.method = m.p.method;
      t.urlTpl = '${var.' + m.service + 'Url}' + m.base + m.tp.path;
      if (m.body) { t.bodyMode = 'json'; t.bodyText = JSON.stringify(m.body, null, 2); }
      m.p.headers.forEach((h) => { if (h[0] !== 'authorization' && h[0] !== 'content-type' && h[0] !== 'cookie') t.headers.push({ k: h[0], v: h[1], on: true }); });
      m.tp.params.forEach((pp) => { t.callVars[pp.name] = pp.recorded; });
      t.name = 'cURL ' + m.p.method;
    }
    this.setState({ catalog: catalog, tabs: this.state.tabs.concat([t]), activeKey: k.key, seq: k.seq, imp: null, collapsed: Object.assign({}, this.state.collapsed, { [m.service]: false }) });
    this.toast(importIt && !m.match ? 'Added ' + fullId + ' to services/' + m.service + '/' + m.service + '.api.yaml.' : 'Opened in a new tab. It is not saved anywhere yet.');
  }
  pmModel() {
    const L = Component.lib();
    return L.postmanMap(L.postmanItems(), this.state.catalog);
  }
  applyPm() {
    const L = Component.lib();
    const off = this.state.imp.pm.off;
    const rows = this.pmModel().filter((r) => !r.match && !off[r.name]);
    let catalog = this.state.catalog;
    rows.forEach((r) => {
      const api = { id: r.id, name: L.nameFromId(r.id, r.service), method: r.method, path: r.path, isNew: true, expect: { status: 200 } };
      if (Object.keys(r.query).length) api.query = r.query;
      catalog = catalog.map((f) => (f.id === r.service ? Object.assign({}, f, { apis: f.apis.concat([api]) }) : f));
    });
    const col = Object.assign({}, this.state.collapsed);
    rows.forEach((r) => { col[r.service] = false; });
    this.setState({ catalog: catalog, imp: null, collapsed: col });
    this.toast('Imported ' + rows.length + ' apis from shop-checkout.postman_collection.json.');
  }

  /* ---------- view models ---------- */
  vSide(tab) {
    const s = this.state;
    const q = s.sideQ.trim().toLowerCase();
    const activeApi = tab && tab.mode === 'request' && !tab.stageUid ? tab.apiId : null;
    let total = 0;
    const groups = s.catalog.map((f) => {
      total += f.apis.length;
      const apis = f.apis.filter((a) => !q || (f.id + '.' + a.id).toLowerCase().indexOf(q) >= 0 || String(a.name || '').toLowerCase().indexOf(q) >= 0).map((a) => {
        const full = f.id + '.' + a.id;
        const mb = this.methodBits(a.method);
        return { fullId: full, name: (a.name || full) + ' · ' + (a.method || 'GET') + ' ' + a.path, mshort: mb.mshort, mcls: mb.mcls, cls: full === activeApi ? 'on' : '', isNew: !!(a.isNew || a.updated), open: () => this.openApi(full) };
      });
      const expanded = q ? apis.length > 0 : !s.collapsed[f.id];
      return { id: f.id, count: f.apis.length, apis: apis, expanded: expanded, chevCls: expanded ? 'open' : '', isNew: !!f.isNew, hide: !!q && !apis.length,
        toggle: () => this.setState({ collapsed: Object.assign({}, s.collapsed, { [f.id]: !s.collapsed[f.id] }) }) };
    }).filter((g) => !g.hide);
    const flows = s.flows.filter((f) => !q || f.name.toLowerCase().indexOf(q) >= 0).map((f) => ({
      name: f.name, count: String(f.stages.length), cls: tab && tab.flowId === f.id && tab.mode === 'flow' ? 'on' : '', open: () => this.openFlow(f.id)
    }));
    const history = s.history.filter((h) => !q || h.apiId.toLowerCase().indexOf(q) >= 0).slice(0, 4).map((h) => {
      const mb = this.methodBits(h.method);
      return { apiId: h.apiId, mshort: mb.mshort, mcls: mb.mcls, status: String(h.status), cls: h.status < 400 && !h.failed ? 'ok' : 'bad', ago: h.ago,
        title: h.method + ' ' + h.apiId + ' · ' + h.status + (h.failed ? ' · ' + h.failed + ' check failed' : '') + ' · ' + h.ago,
        open: () => { if (Component.lib().findApi(s.catalog, h.apiId)) this.openApi(h.apiId, h.response || null); } };
    });
    return { q: s.sideQ, setQ: (e) => this.setState({ sideQ: e.target.value }), groups: groups, noApis: !groups.length, apiCount: String(total), flows: flows, noFlows: !flows.length, history: history, noHistory: !history.length };
  }
  vTabs() {
    const L = Component.lib();
    return this.state.tabs.map((t) => {
      const isFlowTab = t.kind === 'flow';
      const flow = t.flowId ? this.flowById(t.flowId) : null;
      const label = isFlowTab ? (flow ? flow.name : 'Flow') : (t.apiId || t.name);
      const dirty = isFlowTab ? !!(flow && flow.dirty) : L.reqDirty(t, this.state.catalog);
      const mb = this.methodBits(t.method);
      const active = t.key === this.state.activeKey;
      return { label: label, isFlowTab: isFlowTab, isReqTab: !isFlowTab, nameCls: isFlowTab ? '' : 'mono', mshort: mb.mshort, mcls: mb.mcls, active: active, cls: active ? 'on' : '', dirty: dirty,
        select: () => this.setState({ activeKey: t.key }), close: () => this.closeTab(t.key) };
    });
  }
  vCur(tab) {
    const isReq = !!tab && tab.mode === 'request';
    const isFlow = !!tab && tab.mode === 'flow';
    const so = isReq ? this.stageOf(tab) : null;
    let banner = { show: false, n: '', m: '', flowName: '', note: '', apply: () => {} };
    if (so) {
      const fr = this.state.flowRuns[so.flow.id];
      const prevShown = fr && so.index > 0 && fr.shown[so.flow.stages[so.index - 1].uid];
      const item = this.stageItem(tab);
      let note = so.index === 0 ? 'first stage' : prevShown ? 'uses values saved by stages 1–' + so.index : 'run stages 1–' + so.index + ' to fill chained values';
      if (item !== undefined) note += ' · item = ' + item;
      banner = { show: true, n: String(so.index + 1), m: String(so.flow.stages.length), flowName: so.flow.name, note: note, apply: () => this.applyStageVars() };
    }
    return { noBanner: !banner.show, isRequest: isReq, isFlow: isFlow, reqSegCls: isReq ? 'on' : '', flowSegCls: isFlow ? 'on' : '', toRequest: () => this.toMode('request'), toFlow: () => this.toMode('flow'), banner: banner };
  }
  vReq(tab, env) {
    const L = Component.lib();
    const s = this.state;
    const saved = this.savedFor(tab);
    const item = this.stageItem(tab);
    const built = L.buildRequest(tab, Object.assign({}, env, { item: item }), saved);
    const found = tab.apiId ? L.findApi(s.catalog, tab.apiId) : null;
    const prof = L.PROFILES.filter((p) => p.id === s.profile)[0];
    const mb = this.methodBits(tab.method);
    const dirty = L.reqDirty(tab, s.catalog);
    const kd = (e) => { if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') { if (e.preventDefault) e.preventDefault(); this.send(); } };

    let urlSegs = [];
    const tpl = String(tab.urlTpl || '');
    tpl.split(/(\$\{[^}]*\})/).forEach((p) => {
      if (!p) return;
      if (/^\$\{/.test(p)) {
        const miss = [];
        const v = L.resolveValue(p, { vars: built.vars, item: item, missing: miss, secrets: { PAYMENTS_KEY: 1 } }, 0);
        urlSegs.push(miss.length ? { t: p, cls: 'miss' } : { t: String(v), cls: 'rv' });
      } else urlSegs.push({ t: p, cls: '' });
    });
    const resolvedBase = L.resolveValue(tpl, { vars: built.vars, item: item, missing: [], secrets: { PAYMENTS_KEY: 1 } }, 0);
    if (built.url.length > resolvedBase.length && built.url.indexOf(resolvedBase) === 0) urlSegs.push({ t: built.url.slice(resolvedBase.length), cls: '' });
    if (!tpl) urlSegs = [{ t: 'Type a URL, or pick an api in the sidebar', cls: 'ph' }];

    const mode = tab.authMode || 'inherit';
    const authText = mode === 'inherit' ? (found ? found.file.auth + ' · ' + prof.tokenStatus : 'no auth file') : mode === 'none' ? 'No auth' : mode === 'bearer' ? 'Bearer token' : mode === 'basic' ? 'Basic auth' : 'partnerLogin · signs in on Send';
    const authTone = mode === 'inherit' ? (found ? prof.tone : 'skip') : mode === 'none' ? 'skip' : 'accent';

    const vrows = L.varRows(tab, built.levels, { vars: built.vars, secrets: { PAYMENTS_KEY: 1 } });
    const lvl = (r) => (r.win ? L.LEVEL_LABEL[r.win.level] : 'not set');
    const lvlCls = (r) => (r.win ? 'l-' + r.win.level : 'l-miss');
    const shown = (r) => (r.unresolved ? 'not set' : L.fmtShort(r.resolved));
    const pathVars = vrows.filter((r) => r.inUrl).map((r) => ({
      name: r.name, callValue: r.callValue, placeholder: r.win ? String(r.win.level === 'call' ? '' : r.win.value) : 'not set', level: lvl(r), lvlCls: lvlCls(r), resolved: r.unresolved ? 'not set' : L.fmt(r.resolved), set: (e) => this.setCallVar(r.name, e.target.value)
    }));
    const vars = vrows.map((r) => ({
      name: r.name, callValue: r.callValue, placeholder: r.win && r.win.level !== 'call' ? String(r.win.value) : (r.defs[1] ? String(r.defs[1].value) : 'not set'), level: lvl(r), lvlCls: lvlCls(r),
      resolved: r.name === 'sku' && item !== undefined ? String(item) : shown(r), lower: r.defs.slice(1).map((d) => L.LEVEL_LABEL[d.level] + ': ' + L.fmtShort(d.value)).join(' · ') || (r.win ? '' : 'set it here or send the request that saves it'),
      set: (e) => this.setCallVar(r.name, e.target.value)
    }));
    const rowsView = (field) => (tab[field] || []).map((r, i) => ({
      k: r.k, v: r.v, on: !!r.on, n: String(i + 1),
      setK: (e) => this.rowsOp(field, 'edit', i, { k: e.target.value }), setV: (e) => this.rowsOp(field, 'edit', i, { v: e.target.value }),
      toggle: (e) => this.rowsOp(field, 'edit', i, { on: !!e.target.checked }), remove: () => this.rowsOp(field, 'remove', i)
    }));
    const inherited = L.inheritedHeaders(tab, env).map((h) => ({ k: h.k, v: h.v, src: h.overridden ? h.src + ' · overridden' : h.src, cls: h.overridden ? 'ovr' : '' }));
    const jsonErr = tab.bodyMode === 'json' ? L.jsonError(tab.bodyText) : null;
    let okText = '';
    if (tab.bodyMode === 'json' && !jsonErr) {
      try { const o = tab.bodyText.trim() ? JSON.parse(tab.bodyText) : null; okText = o === null ? 'Empty body' : 'Valid JSON · ' + (typeof o === 'object' && o ? Object.keys(o).length + ' top-level fields' : typeof o) + ' · sends ' + L.fmtSize(built.bodyText.length); } catch (e) { okText = ''; }
    }
    const bmodes = [['none', 'none'], ['json', 'JSON'], ['text', 'text'], ['form', 'form'], ['multipart', 'multipart'], ['file', 'file']].map((m) => ({
      label: m[1], on: tab.bodyMode === m[0], cls: tab.bodyMode === m[0] ? 'on' : '',
      pick: () => this.patchCur((t) => ({ bodyMode: m[0], form: (m[0] === 'form' || m[0] === 'multipart') && !(t.form || []).length ? [{ k: '', v: '', on: true }] : t.form }))
    }));
    const ac = L.apiChecks(found, built.ctx).map((c) => ({ text: c.label + ': ' + (c.kind === 'status' ? c.expected.join(' or ') : L.fmt(c.expected)) }));
    const extra = (tab.extraChecks || []).map((c, i) => ({
      text: c.kind === 'status' ? 'status: ' + c.value : c.kind === 'timing' ? 'timing.' + c.path + ': ' + c.op + ' ' + c.value + 'ms' : 'body.' + c.path + (c.op === 'exists' ? ': exists' : ': ' + (c.op === 'eq' ? '' : c.op + ' ') + c.value),
      remove: () => this.patchCur((t) => ({ extraChecks: t.extraChecks.filter((_, j) => j !== i) }))
    }));
    const nc = Object.assign({ kind: 'body', path: '', op: 'eq', value: '' }, tab.newCheck || {});
    const ncInvalid = nc.kind === 'status' ? !/^\d{3}$/.test(nc.value) : nc.kind === 'timing' ? !/^\d+$/.test(nc.value) : !nc.path.trim() || (nc.op !== 'exists' && nc.value === '');
    const saves = (tab.saves || []).map((r, i) => ({
      name: r.name, expr: r.expr, n: String(i + 1), errCls: r.name && !L.validId(r.name) ? 'err' : '',
      setName: (e) => this.savesOp('edit', i, { name: e.target.value }), setExpr: (e) => this.savesOp('edit', i, { expr: e.target.value }), remove: () => this.savesOp('remove', i)
    }));
    const qOn = (tab.query || []).filter((r) => r.on).length;
    const subtabs = [['params', 'Params', pathVars.length + qOn], ['headers', 'Headers', (tab.headers || []).filter((r) => r.on).length], ['body', 'Body', ''],
      ['auth', 'Auth', ''], ['vars', 'Vars', vrows.length], ['checks', 'Checks', ac.length + extra.length], ['save', 'Save', saves.length]].map((x) => ({
      label: x[1], count: String(x[2]), hasCount: x[2] !== '' && x[2] !== 0, on: tab.reqTab === x[0], cls: 'tab' + (tab.reqTab === x[0] ? ' on' : ''), pick: () => this.patchCur({ reqTab: x[0] })
    }));
    const tflags = {};
    ['params', 'headers', 'body', 'auth', 'vars', 'checks', 'save'].forEach((k) => { tflags[k] = tab.reqTab === k; });
    const authOpts = [
      ['inherit', 'Inherit from file', found ? found.file.id + '.api.yaml uses auth ' + found.file.auth + ' (oauth2 client credentials)' : 'This request is not in an api file'],
      ['none', 'None', 'Send without credentials, for example to check a 401'],
      ['bearer', 'Bearer token', 'A token for this request only'],
      ['basic', 'Basic', 'Username and password'],
      ['partnerLogin', 'partnerLogin', 'Flow auth from the config: signs in as the partner account and keeps its session cookie']
    ].map((o) => ({ label: o[1], desc: o[2], on: mode === o[0], cls: mode === o[0] ? 'on' : '', pick: () => this.patchCur({ authMode: o[0] }) }));

    return {
      method: tab.method, mcls: mb.mcls, urlTpl: tab.urlTpl, sending: !!tab.sending,
      setMethod: (e) => this.patchCur({ method: e.target.value }), setUrl: (e) => this.patchCur({ urlTpl: e.target.value }), keydown: kd, send: () => this.send(),
      urlSegs: urlSegs, hasMissing: built.missing.length > 0, missingText: 'Not set: ' + built.missing.map((m) => m.replace(/^var\./, '')).join(', '),
      authText: authText, authTone: authTone,
      fileNote: found ? found.file.path : 'not in an api file', cantSaveApi: !found || !dirty, saveApiTitle: !found ? 'Import or add this request to an api file first' : dirty ? 'Write method, path, headers and body back to ' + found.file.path : 'No changes to save',
      openSaveApi: () => this.openSaveApi(),
      subtabs: subtabs, t: tflags,
      pathVars: pathVars, noPathVars: !pathVars.length, query: rowsView('query'), noQuery: !(tab.query || []).length, addQuery: () => this.rowsOp('query', 'add'),
      inherited: inherited, headers: rowsView('headers'), noHeaders: !(tab.headers || []).length, addHeader: () => this.rowsOp('headers', 'add'),
      bodyModes: bmodes, ctNote: tab.bodyMode === 'none' ? '' : 'content-type: ' + String(L.inheritedHeaders(tab, env).filter((h) => h.k === 'content-type').map((h) => h.v)[0] || ''),
      body: {
        isNone: tab.bodyMode === 'none', isJson: tab.bodyMode === 'json', isText: tab.bodyMode === 'text', isForm: tab.bodyMode === 'form' || tab.bodyMode === 'multipart', isFile: tab.bodyMode === 'file',
        text: tab.bodyText, setText: (e) => this.patchCur({ bodyText: e.target.value }), err: jsonErr || '', hasErr: !!jsonErr, errCls: jsonErr ? 'err' : '', ok: !jsonErr, okText: okText,
        format: () => { try { this.patchCur({ bodyText: JSON.stringify(JSON.parse(tab.bodyText), null, 2) }); } catch (e) { /* invalid */ } },
        fileName: tab.fileName || '', hasFile: !!tab.fileName, noFile: !tab.fileName, pickFile: () => this.patchCur({ fileName: 'avatar.png' }), clearFile: () => this.patchCur({ fileName: '' })
      },
      form: rowsView('form'), addForm: () => this.rowsOp('form', 'add'),
      authOpts: authOpts,
      auth: { isInherit: mode === 'inherit' && !!found, isBearer: mode === 'bearer', isBasic: mode === 'basic', status: 'Token ' + prof.tokenStatus, bearer: tab.bearer || '', user: tab.basicUser || '', pass: tab.basicPass || '',
        setBearer: (e) => this.patchCur({ bearer: e.target.value }), setUser: (e) => this.patchCur({ basicUser: e.target.value }), setPass: (e) => this.patchCur({ basicPass: e.target.value }) },
      vars: vars,
      checksFrom: found ? 'From ' + tab.apiId + ' (expect)' : 'From the api', apiChecks: ac, noApiChecks: !ac.length, extraChecks: extra,
      nc: { kind: nc.kind, path: nc.path, op: nc.op, value: nc.value, isBody: nc.kind === 'body', isTiming: nc.kind === 'timing', hasOp: nc.kind !== 'status', hasValue: !(nc.kind === 'body' && nc.op === 'exists'),
        valueLabel: nc.kind === 'status' ? 'Status' : nc.kind === 'timing' ? 'Milliseconds' : 'Value', valuePh: nc.kind === 'status' ? '200' : nc.kind === 'timing' ? '500' : 'member',
        setKind: (e) => this.setNc({ kind: e.target.value }), setPath: (e) => this.setNc({ path: e.target.value }), setOp: (e) => this.setNc({ op: e.target.value }), setValue: (e) => this.setNc({ value: e.target.value }),
        add: () => this.addCheck(), invalid: ncInvalid },
      saves: saves, noSaves: !saves.length, addSave: () => this.savesOp('add')
    };
  }
  vResp(tab) {
    const L = Component.lib();
    const r = tab.response;
    const has = !!r && !r.error && !tab.sending;
    const out = {
      has: has, noHas: !has, sending: !!tab.sending, empty: !r && !tab.sending, error: !!(r && r.error) && !tab.sending,
      host: '', sendingNote: '', errTitle: '', errHelp: '',
      saveTest: () => this.openSaveTest(), addToFlow: () => this.setState({ dialog: { kind: 'addFlow' } }), copyCurl: () => this.openCurl(),
      statusText: '', statusCls: '', time: '', size: '', hasChecks: false, checksText: '', checksCls: '', gotoChecks: () => this.patchCur({ respTab: 'checks' }),
      tabs: [], t: {}, bodyNote: '', copyBody: () => {}, hasBody: false, noBody: false, lines: [], headers: [], sentHeaders: [], phases: [], timingNote: '', checks: [], hasFailed: false, failHint: '', noChecks: false, saved: [], savedNote: ''
    };
    if (tab.sending) {
      const b = L.buildRequest(tab, Object.assign({}, this.env(), { item: this.stageItem(tab) }), this.savedFor(tab));
      const u = L.splitUrl(b.url);
      out.host = u ? u.host : 'the server';
      out.sendingNote = tab.method + ' ' + (u ? u.path : b.url) + ' · ' + this.state.profile;
    }
    if (out.error) {
      if (r.error === 'missing') {
        const v = r.missing[0].replace(/^var\./, '');
        const producer = L.allApis(this.state.catalog).filter((x) => x.api.save && x.api.save[v])[0];
        out.errTitle = 'Not sent: ${' + r.missing[0] + '} is not set';
        out.errHelp = tab.stageUid ? 'Run the earlier stages of the flow first, or set ' + v + ' on the Vars tab.' : 'Set ' + v + ' on the Vars tab at call level' + (producer ? ', or send ' + producer.fullId + ' first: it saves ' + v + '.' : '.');
      } else { out.errTitle = 'Not sent: the JSON body is invalid'; out.errHelp = r.message + '. Fix it on the Body tab.'; }
    }
    if (!has) return out;
    const failed = r.failed;
    out.statusText = r.status + ' ' + r.statusText;
    out.statusCls = r.status < 300 ? 'pass' : r.status < 400 ? 'flaky' : 'fail';
    out.time = this.fmtMs(r.timing.total);
    out.size = L.fmtSize(r.size);
    out.hasChecks = r.checks.length > 0;
    out.checksText = failed ? failed + ' of ' + r.checks.length + ' checks failed' : r.checks.length + (r.checks.length === 1 ? ' check passed' : ' checks passed');
    out.checksCls = failed ? 'fail' : 'pass';
    const rt = tab.respTab || 'body';
    out.tabs = [['body', 'Body', ''], ['headers', 'Headers', r.headers.length], ['timing', 'Timing', ''], ['checks', 'Checks', r.checks.length], ['saved', 'Saved', r.saved.length]].map((x) => ({
      label: x[1], count: x[0] === 'checks' && failed ? failed + ' failed' : String(x[2]), countCls: x[0] === 'checks' && failed ? 'bad' : '', hasCount: x[2] !== '' && x[2] !== 0,
      on: rt === x[0], cls: 'tab' + (rt === x[0] ? ' on' : ''), pick: () => this.patchCur({ respTab: x[0] })
    }));
    ['body', 'headers', 'timing', 'checks', 'saved'].forEach((k) => { out.t[k] = rt === k; });
    out.bodyNote = r.bodyText ? 'application/json · ' + L.fmtSize(r.size) + ' · ' + r.bodyText.split('\n').length + ' lines' : 'Empty body';
    out.copyBody = () => this.copyText(r.bodyText, 'Copied the response body.');
    out.hasBody = !!r.bodyText;
    out.noBody = !r.bodyText;
    out.lines = r.bodyText ? L.codeLines(r.bodyText, 'json') : [];
    out.headers = r.headers.map((h) => ({ k: h[0], v: h[1] }));
    out.sentHeaders = [{ k: r.method, v: r.url }].concat((r.reqHeaders || []).map((h) => ({ k: h.k, v: h.v })));
    const T = r.timing;
    let start = 0;
    out.phases = ['dns', 'connect', 'tls', 'firstByte', 'download'].map((p) => {
      const ms = T[p];
      const left = T.total ? (start / T.total) * 100 : 0;
      const w = T.total ? (ms / T.total) * 100 : 0;
      start += ms;
      return { name: p, ms: ms + ' ms', barStyle: 'margin-left: ' + left.toFixed(2) + '%; width: ' + (ms ? Math.max(0.8, w).toFixed(2) : 0) + '%' };
    });
    out.timingNote = (T.dns + T.connect + T.tls === 0 ? 'Connection reused from an earlier request. ' : '') + 'firstByte is server time plus one network round trip. Add a timing check, such as timing.total lt 500, on the Checks tab.';
    out.checks = r.checks.map((c) => ({ label: c.label, expected: c.expected, actual: c.actual, result: c.pass ? 'pass' : 'fail', cls: c.pass ? 'pass' : 'fail', actualCls: c.pass ? '' : 'badmark', source: c.source === 'api' ? 'api expect' : 'added' }));
    out.hasFailed = failed > 0;
    const fc = r.checks.filter((c) => !c.pass)[0];
    out.failHint = fc ? (fc.label === 'body.match.total' ? 'total is 64.78 but the api expects ${var.orderTotal} = 59.98. The response now has a tax line (4.80): orders added it in a1c9e4f "orders: add tax line". Update orderTotal in orders.api.yaml or match subtotal instead.' : fc.label + ' expected ' + fc.expected + ', got ' + fc.actual + '.') : '';
    out.noChecks = !r.checks.length;
    out.saved = r.saved.map((s) => ({ name: s.name, expr: s.expr, value: s.ok ? L.fmt(s.value) : 'not found in response', cls: s.ok ? '' : 'badmark' }));
    out.savedNote = r.saved.length ? (tab.stageUid ? 'In a flow, these feed later stages.' : 'Available as ${var.' + r.saved[0].name + '} in every request you send next, until you change profile or reload.') : 'This request saves nothing. Add values on the Save tab of the request.';
    return out;
  }
  vFlow(tab, env) {
    const L = Component.lib();
    const s = this.state;
    const flow = this.flowById(tab.flowId);
    if (!flow) return { stages: [], empty: true };
    const fr = s.flowRuns[flow.id] || null;
    const shown = fr ? fr.shown : {};
    const info = L.stageInfo(flow, s.catalog);
    const firstUnshown = fr && fr.running ? flow.stages.filter((st) => !shown[st.uid])[0] : null;
    const busy = !!(fr && (fr.running || fr.single));
    const n = flow.stages.length;
    const statusMeta = { idle: ['Not run', ''], running: ['Running', 'accent'], queued: ['Queued', ''], passed: ['Passed', 'pass'], failed: ['Failed', 'fail'], skipped: ['Skipped', 'skip'], notrun: ['Stopped', 'skip'] };
    let passed = 0, failed = 0, skipped = 0;
    const stages = flow.stages.map((st, i) => {
      const found = L.findApi(s.catalog, st.api);
      const inf = info[i];
      const res = shown[st.uid] ? fr.results[st.uid] : null;
      let status = 'idle';
      if (fr && fr.single === st.uid) status = 'running';
      else if (res) status = res.status;
      else if (fr && fr.running) status = firstUnshown && firstUnshown.uid === st.uid ? 'running' : 'queued';
      else if (fr && fr.stopped) status = 'notrun';
      if (res && res.status === 'passed') passed++;
      if (res && res.status === 'failed') failed++;
      if (res && res.status === 'skipped') skipped++;
      const mb = this.methodBits(inf.method);
      const loop = st.loop || { kind: 'once' };
      const loopKind = loop.kind === 'repeat' ? (Number(loop.count) >= 3 ? 'repeat3' : 'repeat2') : loop.kind === 'forEach' ? 'forEach' : 'once';
      return {
        n: String(i + 1), api: st.api, name: found ? found.api.name : 'unknown api', mshort: mb.mshort, mcls: mb.mcls,
        consumes: inf.consumes.map((c) => ({ name: c.name, from: c.chained ? '← ' + c.from.replace('stage ', '') : c.from, cls: c.chained ? 'chain' : '', title: c.chained ? 'saved by ' + c.from : 'from ' + c.from })),
        noConsumes: !inf.consumes.length,
        saves: inf.saves.map((v) => ({ name: v, val: res && res.saved && res.saved[v] !== undefined ? L.fmtShort(res.saved[v]) : '', hasVal: !!(res && res.saved && res.saved[v] !== undefined) })),
        noSaves: !inf.saves.length,
        loopKind: loopKind, isForEach: loopKind === 'forEach', items: loop.items || '', hasPoll: !!st.poll, pollText: st.poll ? st.poll.field + ' = ' + st.poll.equals + ' · every ' + st.poll.every : '',
        statusText: statusMeta[status][0], statusCls: statusMeta[status][1], isRunning: status === 'running',
        note: res ? res.note : status === 'idle' && loopKind === 'forEach' ? '' : '', noteCls: res && res.status === 'failed' ? 'failnote' : 'muted',
        time: res && res.time ? this.fmtMs(res.time) : '—',
        rowCls: (status === 'running' ? 'running ' : status === 'failed' ? 'failed ' : '') + (tab.stagePicked && tab.stageUid === st.uid ? 'sel' : ''),
        busy: busy, upOff: i === 0 || busy, downOff: i === n - 1 || busy,
        open: () => this.openStage(st.uid), run: () => this.runStageOnly(st.uid), up: () => this.moveStage(i, -1), down: () => this.moveStage(i, 1), remove: () => this.removeStage(i),
        setLoop: (e) => this.setLoop(i, e.target.value), setItems: (e) => this.setItems(i, e.target.value)
      };
    });
    const ran = Object.keys(shown).length;
    let summary = n + (n === 1 ? ' stage' : ' stages');
    if (fr && fr.running) summary += ' · running';
    else if (ran) summary += ' · last run: ' + passed + ' passed' + (failed ? ', ' + failed + ' failed' : '') + (skipped ? ', ' + skipped + ' skipped' : '');
    const table = L.flowVarTable(flow, env, fr);
    const pq = s.picker && s.picker.flowId === flow.id ? s.picker.q : '';
    const pickItems = L.allApis(s.catalog).filter((x) => !pq || x.fullId.toLowerCase().indexOf(pq.toLowerCase()) >= 0 || String(x.api.name).toLowerCase().indexOf(pq.toLowerCase()) >= 0).slice(0, 40).map((x) => {
      const mb = this.methodBits(x.api.method);
      return { fullId: x.fullId, name: x.api.name || '', mshort: mb.mshort, mcls: mb.mcls, add: () => this.addStage(x.fullId) };
    });
    return {
      name: flow.name, setName: (e) => this.patchFlow(flow.id, { name: e.target.value }, true), summary: summary,
      stopOnFail: !!flow.stopOnFail, toggleStop: (e) => this.patchFlow(flow.id, { stopOnFail: !!e.target.checked }, true),
      running: !!(fr && fr.running), idle: !(fr && fr.running), empty: !n, tooShort: n < 2,
      runAll: () => this.runAll(), stop: () => this.stopRun(), openSaveTest: () => this.openFlowTest(false), openLifecycle: () => this.openFlowTest(true),
      stages: stages,
      pickerOpen: !!(s.picker && s.picker.flowId === flow.id), togglePicker: () => this.setState({ picker: s.picker && s.picker.flowId === flow.id ? null : { flowId: flow.id, q: '' } }),
      pickQ: pq, setPickQ: (e) => this.setState({ picker: { flowId: flow.id, q: e.target.value } }), pickItems: pickItems, noPick: !pickItems.length,
      varCols: 'grid-template-columns: 120px repeat(' + Math.max(1, n) + ', minmax(104px, 1fr))', varMinW: 'min-width: ' + (140 + n * 110) + 'px',
      varHead: flow.stages.map((st, i) => ({ label: (i + 1) + ' ' + st.api.split('.')[1] })),
      varRows: table.map((r) => ({ name: r.name, cells: r.cells.map((c) => ({ val: c.val || '—', cls: c.sets ? 'set' : c.uses && c.val ? 'use' : c.val ? '' : 'none' })) })),
      varNote: ran ? 'Values after each stage of the last run. Click a stage to send it on its own with these values.' : 'Run the flow to fill in saved values. Constants from files, profile and config show where a stage uses them.'
    };
  }
  vDialog() {
    const L = Component.lib();
    const d = this.state.dialog;
    const close = () => this.setState({ dialog: null });
    const out = { saveTest: false, curl: false, saveApi: false, addFlow: false, flowTest: false, close: close, st: {}, cu: {}, sa: {}, af: {}, ft: {} };
    if (!d) return out;
    const t = this.tab();
    const upd = (patch) => this.setState({ dialog: Object.assign({}, this.state.dialog, patch) });
    if (d.kind === 'saveTest' && t) {
      const m = this.saveTestModel(d);
      out.saveTest = true;
      const cv = Object.keys(t.callVars || {});
      out.st = {
        apiId: t.apiId || 'request', editing: !d.done, done: !!d.done, name: d.name, setName: (e) => upd({ name: e.target.value }), fullId: m.fullId, idErr: m.idErr, hasIdErr: !!m.idErr, warn: m.warn, hasWarn: !m.idErr && !!m.warn,
        fileId: d.fileId, setFile: (e) => upd({ fileId: e.target.value, area: e.target.value === 'new' ? (d.area || 'smoke') : d.area }), path: m.path, isNewFile: d.fileId === 'new', area: d.area || '', setArea: (e) => upd({ area: e.target.value }), newFileId: m.fileId,
        exp: d.exp.map((x, i) => ({ label: x.label, on: !!x.on, toggle: (e) => upd({ exp: d.exp.map((y, j) => (j === i ? Object.assign({}, y, { on: !!e.target.checked }) : y)) }) })),
        hasVars: cv.length > 0, includeVars: !!d.includeVars, toggleVars: (e) => upd({ includeVars: !!e.target.checked }), varsText: cv.map((k) => k + ': ' + t.callVars[k]).join(', '),
        invalid: !!m.idErr, create: () => { if (!m.idErr) { upd({ done: true }); } },
        doneNote: d.fileId === 'new' ? 'Created ' + m.path + ' with test ' + m.fullId + '.' : 'Added ' + m.fullId + ' to ' + m.path + '.',
        yaml: L.codeLines(m.yaml, 'yaml')
      };
    }
    if (d.kind === 'curl') {
      out.curl = true;
      out.cu = { lines: d.text.split('\n').map((x, i) => ({ no: String(i + 1), text: x })), copy: () => { this.copyText(d.text, 'Copied the cURL command.'); upd({ copied: true }); }, copiedNote: d.copied ? 'Copied to the clipboard.' : 'Paste it in a terminal, a ticket or the cURL import.' };
    }
    if (d.kind === 'saveApi' && t) {
      const m = this.saveApiModel();
      out.saveApi = true;
      out.sa = { apiId: t.apiId || '', hasErr: !!m.err, err: m.err || '', ok: !m.err, path: m.found ? m.found.file.path : '', fileName: m.found ? m.found.file.id + '.api.yaml' : 'api file',
        add: m.dv ? String(m.dv.add) : '0', del: m.dv ? String(m.dv.del) : '0', rows: m.dv ? m.dv.rows : [], write: () => this.writeApi() };
    }
    if (d.kind === 'addFlow' && t) {
      out.addFlow = true;
      out.af = { apiId: t.apiId || 'this request', blocked: !t.apiId, ok: !!t.apiId,
        flows: this.state.flows.map((f) => ({ name: f.name, meta: f.stages.length + ' stages · becomes stage ' + (f.stages.length + 1), pick: () => this.addToFlow(f.id) })), newFlow: () => this.addToFlow(null) };
    }
    if (d.kind === 'flowTest' && t) {
      const m = this.flowTestModel(d);
      out.flowTest = true;
      const n = m.flow.stages.length;
      out.ft = {
        title: d.lifecycle ? 'Save as lifecycle' : 'Save flow as test', isLifecycle: !!d.lifecycle, name: d.name, setName: (e) => upd({ name: e.target.value }), fileId: d.fileId, setFile: (e) => upd({ fileId: e.target.value }), path: m.path,
        fullId: m.fullId, idErr: m.idErr, hasIdErr: !!m.idErr, warn: m.warn, hasWarn: !m.idErr && !!m.warn,
        nText: m.nText, less: () => upd({ n: Math.max(1, d.n - 1) }), more: () => upd({ n: Math.min(n - 1, d.n + 1) }), lessOff: d.n <= 1, moreOff: d.n >= n - 1,
        lcNote: d.fileId === 'orders.checkout' ? 'It replaces the current beforeEach (users.create, carts.create, carts.addItem).' : '',
        previewTitle: m.previewTitle, previewNote: d.lifecycle ? 'diff' : m.path, yaml: m.rows,
        footNote: d.lifecycle ? 'Every test in ' + d.fileId + ' gets these stages before it runs.' : 'Stages become steps; saved values chain the same way.',
        writeLabel: d.lifecycle ? 'Write lifecycle' : 'Create test', invalid: !!m.idErr, write: () => this.writeFlowTest()
      };
    }
    return out;
  }
  vImport() {
    const L = Component.lib();
    const imp = this.state.imp;
    const close = () => this.setState({ imp: null });
    if (!imp) return { open: false, close: close };
    const tabs = [['har', 'HAR file'], ['openapi', 'OpenAPI'], ['curl', 'cURL'], ['postman', 'Postman collection']].map((x) => ({ label: x[1], on: imp.tab === x[0], cls: 'tab' + (imp.tab === x[0] ? ' on' : ''), pick: () => this.patchImp({ tab: x[0] }) }));
    const out = { open: true, close: close, tabs: tabs, isHar: imp.tab === 'har', isOpenapi: imp.tab === 'openapi', isCurl: imp.tab === 'curl', isPostman: imp.tab === 'postman' };
    if (imp.tab === 'har') Object.assign(out, this.vHar(imp));
    if (imp.tab === 'openapi') {
      const oa = imp.oa;
      const rows = this.oaModel().map((r) => {
        const mb = this.methodBits(r.method);
        const key = r.method + r.path;
        return { method: r.method, mshort: mb.mshort, mcls: mb.mcls, path: r.path, id: 'payments.' + r.id, idFrom: r.idFrom === 'operationId' ? 'from operationId ' + r.operationId : r.idFrom, isMatch: !!r.match,
          status: r.match ? 'matches ' + r.match + ', skipped' : 'new api', on: !r.match && !oa.off[key], toggle: (e) => this.patchImp({ oa: Object.assign({}, oa, { off: Object.assign({}, oa.off, { [key]: !e.target.checked }) }) }) };
      });
      const count = rows.filter((r) => r.on).length;
      const po = (p) => this.patchImp({ oa: Object.assign({}, this.state.imp.oa, p) });
      const load = () => { po({ loading: true, loaded: false }); this.later(() => { if (this.state.imp) po({ loading: false, loaded: true }); }, 650); };
      out.oa = { isUrl: oa.src === 'url', isFile: oa.src === 'file', urlCls: oa.src === 'url' ? 'on' : '', fileCls: oa.src === 'file' ? 'on' : '', useUrl: () => po({ src: 'url', loaded: false }), useFile: () => po({ src: 'file', loaded: false }),
        url: oa.url, setUrl: (e) => po({ url: e.target.value, loaded: false }), load: load, loading: !!oa.loading, loaded: !!oa.loaded,
        hasFile: !!oa.hasFile, fileBtn: oa.hasFile ? 'Choose another file…' : 'Choose file…', pickFile: () => { po({ hasFile: true }); load(); },
        summary: oa.src === 'file' ? 'Swagger 2.0 (payments-v2.swagger.json) converted to OpenAPI 3.0.3 · 5 operations · host payments.staging.shop.example → api file payments' : 'OpenAPI 3.0.3 (JSON) · Payments API 2.4.0 · 5 operations · host payments.staging.shop.example → api file payments',
        rows: rows, count: String(count), cantApply: !oa.loaded || !count, apply: () => this.applyOa(), foot: oa.loaded ? count + ' new apis selected; matching ones are left as they are.' : 'Load the spec to see what it contains.' };
    }
    if (imp.tab === 'curl') {
      const m = this.curlModel();
      const pc = (p) => this.patchImp({ cu: Object.assign({}, this.state.imp.cu, p) });
      out.cu = { text: imp.cu.text, setText: (e) => pc({ text: e.target.value }), hasErr: !!m.error, err: m.error || '', ok: !m.error, open: () => this.curlToTab(false), apply: () => this.curlToTab(true),
        method: '', mcls: '', tpl: '', fullId: '', status: '', pillCls: '', filePath: '', params: '', headers: [], body: '', cantImport: true };
      if (!m.error) {
        const mb = this.methodBits(m.p.method);
        Object.assign(out.cu, {
          method: m.p.method, mcls: mb.mcls, tpl: '${var.' + m.service + 'Url}' + m.base + m.tp.path, fullId: m.service + '.' + m.id,
          status: m.match ? 'matches an existing api' : m.file ? 'new api in an existing file' : 'new api file', pillCls: m.match ? '' : 'accent', filePath: 'services/' + m.service + '/' + m.service + '.api.yaml',
          params: m.tp.params.length ? m.tp.params.map((p) => p.recorded + ' → ${var.' + p.name + '}').join(', ') : 'none',
          headers: m.p.headers.map((h) => ({ k: h[0], note: h[0] === 'authorization' ? 'removed: ' + m.service + ' uses auth shopOauth' : h[0] === 'content-type' ? 'set from the body type' : h[1] })),
          body: m.body ? JSON.stringify(m.body) + (typeof m.body === 'object' ? '  →  fields become vars with these defaults' : '') : 'none', cantImport: !!m.match
        });
      }
    }
    if (imp.tab === 'postman') {
      const rows = this.pmModel().map((r) => {
        const mb = this.methodBits(r.method);
        return { name: r.name, folder: r.folder, mshort: mb.mshort, mcls: mb.mcls, tpl: r.tpl, fullId: r.service + '.' + r.id, isMatch: !!r.match, status: r.match ? 'matches, skipped' : 'new api',
          on: !r.match && !imp.pm.off[r.name], toggle: (e) => this.patchImp({ pm: { off: Object.assign({}, imp.pm.off, { [r.name]: !e.target.checked }) } }) };
      });
      const count = rows.filter((r) => r.on).length;
      out.pm = { rows: rows, count: String(count), cantApply: !count, apply: () => this.applyPm(), foot: rows.filter((r) => r.isMatch).length + ' requests match existing apis; ' + count + ' new will be added.' };
    }
    return out;
  }
  vHar(imp) {
    const L = Component.lib();
    const m = this.harModel();
    const step = imp.step;
    const labels = ['Source', 'Filter', 'Map', 'Secrets', 'Also create', 'Review'];
    const reachable = (n) => {
      if (n <= 1) return true;
      if (!imp.fileOn) return false;
      if (n >= 3 && !m.selected.length) return false;
      if (n >= 4 && m.idErrors) return false;
      return true;
    };
    const steps = labels.map((lb, i) => {
      const n = i + 1;
      return { n: String(n), label: lb, cls: n === step ? 'on' : n < step ? 'done' : '', locked: !reachable(n), current: n === step ? 'step' : 'false', go: () => { if (reachable(n)) this.patchImp({ step: n }); } };
    });
    const pf = (p) => this.patchImp({ f: Object.assign({}, this.state.imp.f, p) });
    const rows = m.visible.map((e) => {
      const kind = L.classifyEntry(e);
      const mb = this.methodBits(e.method);
      const on = kind === 'api' ? !imp.excluded[e.idx] : !!imp.f.included[e.idx];
      const u = L.splitUrl(e.url);
      return { method: e.method, mshort: mb.mshort === 'OPTIONS' ? 'OPT' : mb.mshort, mcls: mb.mcls, url: u ? u.host + u.path + (u.query ? '?' + u.query : '') : e.url, status: String(e.status), time: e.time + ' ms',
        kind: { api: 'api', static: 'static asset', thirdParty: 'third party', preflight: 'preflight' }[kind], on: on,
        toggle: (ev) => {
          const c = !!ev.target.checked;
          if (kind === 'api') this.patchImp({ excluded: Object.assign({}, this.state.imp.excluded, { [e.idx]: !c }) });
          else pf({ included: Object.assign({}, this.state.imp.f.included, { [e.idx]: c }) });
        } };
    });
    const map = m.groups.map((g) => ({
      host: g.host, fileId: g.fileId, filePath: g.filePath, fileStatus: g.isNew ? 'new file' : 'existing file', pillCls: g.isNew ? 'accent' : '',
      apis: g.apis.map((a, ai) => {
        const mb = this.methodBits(a.method);
        const pathSegs = a.path.split(/(\$\{[^}]*\})/).filter(Boolean).map((p) => ({ t: p, cls: /^\$\{/.test(p) ? 'y-tpl' : '' }));
        const vars = a.match ? a.extraFields.map((k) => ({ name: k, val: '= ' + L.fmtShort(a.recorded[0].reqBody[k]) })) : Object.keys(a.vars).map((k) => ({ name: k, val: '= ' + L.fmtShort(a.vars[k]) })).concat(a.chained.map((v) => ({ name: v, val: '← chained' })));
        const isSkip = a.action === 'skip';
        const paramsText = a.detected.length ? 'detected ' + a.detected.map((p) => p.recorded + ' → ' + p.name).join(', ') : '';
        let note = '';
        if (a.match && a.extraFields.length) note = 'recorded body adds ' + a.extraFields.join(', ') + (isSkip ? '' : '; update adds it as a var');
        if (Object.keys(a.query).some((k) => /^(api_key|apikey|key|token)$/.test(k))) note = 'query api_key is a secret, see step 4';
        return {
          method: a.method, mshort: mb.mshort, mcls: mb.mcls, recordedPath: a.recordedPath + (Object.keys(a.query).length ? '?' + Object.keys(a.query).map((k) => k + '=…').join('&') : ''), pathSegs: pathSegs,
          hasParams: !!paramsText, paramsText: paramsText, inputId: 'vcMap' + g.fileId + ai,
          id: a.match && isSkip ? a.match.split('.')[1] : a.id, idLocked: !!a.match, setId: (e) => this.patchImp({ ids: Object.assign({}, this.state.imp.ids, { [a.key]: e.target.value }) }),
          fullId: g.fileId + '.' + (a.match ? a.match.split('.')[1] : a.id), idErr: a.action !== 'skip' ? a.idError : '', hasErr: a.action !== 'skip' && !!a.idError, errCls: a.action !== 'skip' && a.idError ? 'err' : '',
          isMatch: !!a.match, isNewApi: !a.match, match: a.match || '', isSkip: isSkip, isUpd: a.action === 'update', skipCls: isSkip ? 'on' : '', updCls: a.action === 'update' ? 'on' : '',
          setSkip: () => this.patchImp({ actions: Object.assign({}, this.state.imp.actions, { [a.key]: 'skip' }) }), setUpdate: () => this.patchImp({ actions: Object.assign({}, this.state.imp.actions, { [a.key]: 'update' }) }),
          hasMerged: a.count > 1, mergedText: '×' + a.count + ' in the recording, merged into one api', hasNote: !!note, note: note, hasVars: vars.length > 0, vars: vars
        };
      })
    }));
    const ps = (v) => this.patchImp({ secretName: v });
    const secrets = m.secrets.map((x) => ({
      kindLabel: { header: 'header', cookie: 'cookies', query: 'query string' }[x.kind], name: x.name, sample: x.sample, where: x.host ? 'on ' + x.host : '', countText: 'in ' + x.count + (x.count === 1 ? ' request' : ' requests'),
      outcome: x.kind === 'query' ? (m.secretOk ? x.outcome : 'Secret names are UPPER_SNAKE_CASE, like PAYMENTS_KEY.') : x.outcome, isQuery: x.kind === 'query', secretName: imp.secretName, setName: (e) => ps(e.target.value), errCls: m.secretOk ? '' : 'err'
    }));
    const flowErr = imp.makeFlow && (!imp.flowName.trim() || !L.validId(m.flowId)) ? 'Type a name; the id is derived from it.' : imp.makeFlow && ['placesOrder', 'appliesCoupon'].indexOf(m.flowId) >= 0 ? 'orders.checkout.' + m.flowId + ' already exists.' : '';
    const fl = {
      on: !!imp.makeFlow, toggle: (e) => this.patchImp({ makeFlow: !!e.target.checked }), name: imp.flowName, setName: (e) => this.patchImp({ flowName: e.target.value }),
      fullId: 'orders.checkout.' + (m.flowId || '…'), hasErr: !!flowErr, err: flowErr, keepReads: !!imp.keepReads, toggleReads: (e) => this.patchImp({ keepReads: !!e.target.checked }),
      yaml: m.flowTest ? L.codeLines(L.testBlockYaml(m.flowTest), 'yaml') : [], chain: L.chainSummary(m.groups).map((c) => ({ name: c.name, from: c.from, to: c.to.length ? c.to.join(', ') : 'not used later' }))
    };
    const files = m.files.files;
    const ri = Math.min(imp.reviewIdx, Math.max(0, files.length - 1));
    const fileRows = files.map((f, i) => {
      const dv = L.diffView(f.before, f.after);
      return { path: f.path, status: f.status, pillCls: f.status === 'new' ? 'accent' : 'flaky', add: String(dv.add), del: String(dv.del), cls: i === ri ? 'on' : '', pick: () => this.patchImp({ reviewIdx: i }) };
    });
    const sel = files[ri];
    const rv = sel ? { path: sel.path, statusText: sel.status === 'new' ? 'new file' : 'changed', hasNote: !!sel.note, note: sel.note || '', rows: L.diffView(sel.before, sel.after).rows } : { path: '', statusText: '', hasNote: false, note: '', rows: [] };
    const nextOff = (step === 1 && !imp.fileOn) || (step === 2 && !m.selected.length) || (step === 3 && m.idErrors > 0) || (step === 4 && !m.secretOk) || (step === 5 && !!flowErr);
    const added = m.groups.reduce((n, g) => n + g.apis.filter((a) => a.action === 'add').length, 0);
    const upd = m.groups.reduce((n, g) => n + g.apis.filter((a) => a.action === 'update').length, 0);
    const foot = [
      imp.fileOn ? '46 entries in checkout-session.har' : 'Choose a HAR file to continue',
      m.selected.length + ' of ' + m.visible.length + ' shown requests selected',
      m.idErrors ? m.idErrors + ' id needs fixing' : added + ' new apis, ' + upd + ' updated, ' + (m.groups.reduce((n, g) => n + g.apis.length, 0) - added - upd) + ' skipped',
      m.secrets.length + ' kinds of secrets found, none will be written',
      imp.makeFlow ? (m.steps ? m.steps.length : 0) + ' steps in the flow test' : 'No flow test',
      files.length + ' files: ' + files.filter((f) => f.status === 'new').length + ' new, ' + files.filter((f) => f.status !== 'new').length + ' changed'
    ][step - 1];
    return {
      steps: steps, s1: step === 1, s2: step === 2, s3: step === 3, s4: step === 4, s5: step === 5, s6: step === 6,
      har: { hasFile: !!imp.fileOn, noFile: !imp.fileOn, removeFile: () => this.patchImp({ fileOn: false }), pickFile: () => this.patchImp({ fileOn: true }),
        hideStatic: !!imp.f.hideStatic, hideThird: !!imp.f.hideThird, hidePre: !!imp.f.hidePreflight,
        toggleStatic: (e) => pf({ hideStatic: !!e.target.checked }), toggleThird: (e) => pf({ hideThird: !!e.target.checked }), togglePre: (e) => pf({ hidePreflight: !!e.target.checked }),
        cStatic: String(m.counts.static), cThird: String(m.counts.thirdParty), cPre: String(m.counts.preflight),
        listTitle: m.visible.length + ' requests', selNote: m.selected.length + ' selected · first-party api calls are selected by default', rows: rows },
      map: map, secrets: secrets, fl: fl, files: fileRows, rv: rv,
      footNote: foot, back: () => this.patchImp({ step: Math.max(1, step - 1) }), backOff: step === 1, next: () => { if (!nextOff) this.patchImp({ step: step + 1 }); }, nextOff: nextOff,
      notLast: step < 6, isLast: step === 6, apply: () => this.applyHar()
    };
  }

  renderVals() {
    const tab = this.tab();
    const env = this.env();
    const t = this.state.toast;
    const collapsed = !!this.state.sideCollapsed;
    return {
      appCls: 'app' + (collapsed ? ' side-collapsed' : ''),
      toggleSide: () => this.setState({ sideCollapsed: !this.state.sideCollapsed }),
      sideToggleLbl: collapsed ? 'Expand sidebar' : 'Collapse sidebar', sideExp: collapsed ? 'false' : 'true', sideChev: collapsed ? 'flip' : '',
      profile: this.state.profile, setProfile: (e) => this.setProfile(e.target.value),
      openImport: () => this.openImport(), newTab: () => this.newTab(),
      side: this.vSide(tab), tabs: this.vTabs(), hasTab: !!tab, noTabs: !tab,
      cur: this.vCur(tab),
      req: tab && tab.mode === 'request' ? this.vReq(tab, env) : {},
      resp: tab && tab.mode === 'request' ? this.vResp(tab) : {},
      flow: tab && tab.mode === 'flow' ? this.vFlow(tab, env) : {},
      dlg: this.vDialog(), imp: this.vImport(),
      toast: { show: !!t, text: t ? t.text : '' }
    };
  }
}
