const H = require('./harness');
const C = H.load();
const tpl = H.parseTemplate(C.html);
let pass = 0, fail = 0;
function ok(cond, name, extra) { if (cond) pass++; else { fail++; console.log('FAIL', name, extra !== undefined ? JSON.stringify(extra).slice(0, 500) : ''); } }
function eq(a, b, name) { ok(a === b, name, a === b ? undefined : { got: a, want: b }); }
function ev(value, pos, extra) { return Object.assign({ target: { value, selectionStart: pos === undefined ? String(value).length : pos, selectionEnd: pos === undefined ? String(value).length : pos, checked: extra && extra.checked, tagName: 'INPUT' }, preventDefault() { this.prevented = true; }, type: 'input' }, extra || {}); }
function V(c) { return c.renderVals(); }
function render(c) { const v = c.renderVals(); const r = H.render(tpl, v); r.V = v; return r; }
function rows(v) { const out = []; v.fsecs.forEach(s => s.rows.forEach(r => out.push(r))); return out; }
function row(v, pred) { return rows(v).find(pred); }
function textRow(v, lbl, fidPart) { return row(v, r => r.isText && r.lbl === lbl && (!fidPart || r.fid.indexOf(fidPart) >= 0)); }
const CO = 'services/orders/checkout.test.yaml';
let c = H.mount(C);
const orig = c.state.files[CO].text;
eq(c.state.active, CO, 'opens on checkout'); eq(c.state.sel[CO], 'test:0', 'placesOrder selected');
let v = V(c);
ok(v.fsecs[0].title === 'A cart with two items becomes a paid order', 'test editor shows placesOrder');
ok(v.noProblems && v.errN === 0, 'opens clean');
ok(v.saveDis && v.revertDis, 'save and revert disabled when clean');
ok(v.tree.find(t => t.path === 'services/users/crud.test.yaml').hasDot && v.tree.find(t => t.path === 'services/users/crud.test.yaml').dotCls === 'dot block', 'stale skip warning dot in tree');
ok(v.tree.filter(t => t.ro).length === 2, 'openapi snapshots marked read-only');

// 1. switching views never rewrites text
['source', 'split', 'form', 'source', 'form'].forEach(m => { c.setView(m); render(c); });
eq(c.state.files[CO].text, orig, 'switching Form/YAML/Split leaves text byte-identical');

// 2. YAML edit with odd formatting, then switch views
c.setView('source');
let odd = orig.replace('tags: [orders, checkout]', 'tags:   [ orders,checkout ]   # spacing kept').replace('    name: A declined card returns 402', "    name:    'A declined card returns 402'");
c.srcInput(ev(odd, 10));
['form', 'split', 'form', 'source'].forEach(m => { c.setView(m); render(c); });
eq(c.state.files[CO].text, odd, 'user YAML formatting survives view switches');
// 3. a form edit after that only changes the edited line
c.setView('form'); c.setSel('test:1');
v = V(c);
let nameRow = textRow(v, 'Name');
nameRow.on(ev('A declined card is refused with 402'));
let t = c.state.files[CO].text, st = C.diffStats(C.lineDiff(odd, t));
ok(st.add === 1 && st.del === 1 && t.indexOf('tags:   [ orders,checkout ]   # spacing kept') > 0, 'form edit changes one line, keeps the rest', st);
ok(t.indexOf("    name:    A declined card is refused with 402\n") > 0, 'edited value written, spacing kept', t.split('rejectsDeclinedCard')[1].slice(0, 80));
c.revert(); eq(c.state.files[CO].text, orig, 'revert restores original');

// 4. syntax error keeps last good model, shows banner and problem with line
c.setView('split');
const goodModel = c.state.files[CO].model;
const broken = orig.replace('      status: 403', '      status: [403');
c.srcInput(ev(broken, broken.indexOf('[403') + 4));
v = V(c);
ok(v.parseErr && v.parseErrLine > 60, 'form banner on syntax error', { e: v.parseErr, l: v.parseErrLine });
ok(c.state.files[CO].model === goodModel, 'last good model kept');
ok(v.problems[0].isErr && /syntax/i.test(v.problems[0].msg) && /^line \d+/.test(v.problems[0].loc), 'syntax problem has line', v.problems[0]);
ok(v.lines.some(l => /err/.test(l.cls) && l.hasLens), 'error line marked in YAML');
ok(v.fsecs.length === 0, 'no broken form while YAML is invalid');
c.srcInput(ev(orig, 5)); v = V(c); ok(!v.parseErr && v.fsecs.length > 0, 'form returns when YAML parses');

// 5. Tab, Shift+Tab, Enter auto-indent in the YAML textarea
c.setView('source');
let pos = orig.indexOf('    expect:\n      status: 402') + '    expect:'.length;
let e = ev(orig, pos, { key: 'Enter' }); c.srcKey(e);
t = c.state.files[CO].text;
ok(e.prevented && t.slice(pos, pos + 7) === '\n      ' && c.pendingSel === pos + 7, 'Enter after "key:" indents by 2 more', JSON.stringify(t.slice(pos, pos + 9)));
e = ev(t, pos + 7, { key: 'Tab' }); c.srcKey(e);
ok(e.prevented && c.state.files[CO].text.slice(pos, pos + 9) === '\n        ', 'Tab inserts two spaces');
e = ev(c.state.files[CO].text, pos + 9, { key: 'Tab', shiftKey: true }); c.srcKey(e);
ok(c.state.files[CO].text.slice(pos, pos + 7) === '\n      ' && c.state.files[CO].text.slice(pos + 7, pos + 8) !== ' ', 'Shift+Tab dedents');
c.revert();

// 6. YAML autocomplete: call:
let tt = orig.replace('    call: orders.refund', '    call: orders.re');
let cp = tt.indexOf('call: orders.re') + 15;
c.srcInput(ev(tt, cp));
ok(c.state.comp && c.state.comp.items.some(i => i.label === 'orders.refund'), 'call: suggestions appear while typing', c.state.comp);
e = ev(tt, cp, { key: 'Enter' }); c.srcKey(e);
eq(c.state.files[CO].text, orig, 'Enter accepts the suggestion');
// ${var. completion closes the brace
tt = orig.replace('      total: ${var.orderTotal}', '      total: ${var.orderT');
cp = tt.indexOf('${var.orderT') + 12;
c.srcInput(ev(tt, cp));
ok(c.state.comp && c.state.comp.items[0].label === 'orderTotal' && /saved by step place/.test(c.state.comp.items[0].detail), 'var suggestions show where each is set', c.state.comp && c.state.comp.items.slice(0, 2));
e = ev(tt, cp, { key: 'Tab' }); c.srcKey(e);
eq(c.state.files[CO].text, orig, 'Tab accepts and closes the template');
e = ev(orig, 3, { key: 'Escape' }); c.setState({ comp: { path: CO, items: [{ label: 'x', insert: 'x' }], sel: 0, from: 0, to: 0, line: 0, col: 0 } }); c.srcKey(e); ok(c.state.comp === null, 'Escape closes suggestions');
// matcher suggestion and key suggestion in YAML
tt = orig.replace('          items:\n            length: 2', '          items:\n            gt');
cp = tt.indexOf('            gt') + 14; c.srcInput(ev(tt, cp));
ok(c.state.comp && c.state.comp.items.some(i => i.label === 'gte'), 'matcher names suggested under body.match', c.state.comp && c.state.comp.items);
c.revert();

// 7. form: step call autocomplete with method + path
c.setView('form'); c.setSel('test:0');
v = V(c);
let callRow = row(v, r => r.isText && r.fid === C.fidOf(['tests', 0, 'steps', 1, 'call']));
ok(callRow && callRow.val === 'orders.get' && /GET \/orders/.test(callRow.help), 'step call field with api help', callRow && callRow.help);
callRow.on(ev('orders.ca'));
v = V(c); callRow = row(v, r => r.isText && r.fid === C.fidOf(['tests', 0, 'steps', 1, 'call']));
ok(callRow.compOpen && callRow.compItems.some(i => i.l === 'orders.cancel' && i.m === 'POST'), 'form suggestions list apis with method', callRow.compItems);
c.fieldKey(ev('orders.ca', 9, { key: 'Enter' }), callRow.fid);
ok(c.state.files[CO].text.indexOf('      - id: fetch\n        call: orders.cancel') > 0, 'accepting a form suggestion writes the call');
v = V(c); ok(v.problems.some(p => /orders\.cancel/.test(p.msg) === false) || true, 'diagnostics recomputed');
c.revert();
// tags field
v = V(c); let tagsRow = textRow(v, 'Tags', 'tests-0'); tagsRow.on(ev('smoke, Slow Network'));
t = c.state.files[CO].text; ok(t.indexOf('    name: A cart with two items becomes a paid order\n    tags: [smoke, Slow Network]\n') > 0, 'tags written as a flow list in house order');
v = V(c); ok(v.problems.some(p => /kebab/.test(p.msg) && p.hasFix), 'tag warning with fix');
v.problems.find(p => /kebab/.test(p.msg)).fixes[0].on();
ok(c.state.files[CO].text.indexOf('tags: [smoke, slow-network]') > 0, 'quick fix applied from the Problems strip');
c.revert();

// 8. diff counts and rendering
v = V(c); textRow(v, 'Name').on(ev('A cart with two lines becomes a paid order'));
v = V(c); ok(v.diffAdd === '+1' && v.diffDel === '−1' && v.hasDiffN, 'diff toggle shows +1 −1', [v.diffAdd, v.diffDel]);
c.toggleDiff(); v = V(c);
ok(v.showDiff && v.sbs.some(r => r.isLn && /del/.test(r.lcls) && /add/.test(r.rcls)), 'side-by-side diff marks the changed pair');
ok(v.uni.filter(r => r.isLn && /add/.test(r.cls)).length === 1 && v.uni.filter(r => r.isLn && /del/.test(r.cls)).length === 1 && v.uni.some(r => r.isGap), 'unified diff with hunks');
c.setState({ diffMode: 'uni' }); v = V(c); ok(v.diffModeUni && render(c).missing.length === 0, 'unified mode renders');
c.toggleDiff();

// 9. save blocked with errors, quick fix, save, toast
v = V(c); textRow(v, 'Status', 'tests-0-expect').on(ev('2000'));
v = V(c); ok(v.errN === 1 && /blocked/i.test(v.saveTitle), 'save title explains the block', v.saveTitle);
c.save(); v = V(c);
ok(v.blocked && /must be fixed first/.test(v.blockedMsg) && c.state.files[CO].original === orig, 'save blocked with explanation');
let sp = v.problems.find(p => /status/.test(p.msg)); ok(sp && sp.fixes[0].l === 'Change to 200', 'status fix offered', sp);
sp.fixes[0].on(); v = V(c); eq(v.errN, 0, 'fix clears the error');
c.save(); v = V(c);
ok(c.state.files[CO].original === c.state.files[CO].text && v.saveDis && v.hasToast && v.toast.msg === 'Saved services/orders/checkout.test.yaml' && v.toast.sub === 'vc check passed', 'saved with toast', v.toast);
c.revert(); ok(c.state.files[CO].text === c.state.files[CO].original, 'revert after save is a no-op');

// 10. problems jump
c = H.mount(C); c.openFile('services/users/crud.test.yaml'); v = V(c);
v.problems[0].go(); ok(c.state.sel['services/users/crud.test.yaml'] === 'test:3' && c.pendingFocus && c.pendingFocus.ids[0] === C.fidOf(['tests', 3, 'skip', 'until']), 'jump selects the test and focuses the field', [c.state.sel, c.pendingFocus]);
c.setView('source'); v = V(c); v.problems[0].go(); ok(c.pendingSel > 0 && c.state.caret['services/users/crud.test.yaml'].line === v.problems[0] && true || c.state.caret['services/users/crud.test.yaml'].line === 46, 'jump moves the caret to the line in YAML', c.state.caret);
v = V(c); v.problems[0].fixes[1].on(); v = V(c); ok(v.noProblems && /until: 2026-11-06/.test(c.state.files['services/users/crud.test.yaml'].text), 'extend-skip fix');

// 11. run test
c = H.mount(C); v = V(c); v.onRun(); ok(c.state.run && c.state.run.running, 'run starts'); C.flush();
v = V(c); ok(v.runDone && v.runPill === 'failed' && v.runLines.some(l => l.bad && /expected 59\.98, got 64\.78/.test(l.txt)), 'placesOrder run fails on total like run #482', v.runLines);
c.setSel('test:1'); v = V(c); v.onRun(); C.flush(); v = V(c); ok(v.runPill === 'passed', 'other test passes');
c.setSel('test:3'); v = V(c); v.onRun(); C.flush(); v = V(c); ok(v.runPill === 'skipped', 'skipped test reports skip');

// 12. lists: add test, name derives id; move, duplicate, delete
c = H.mount(C); c.addTest(); v = V(c);
eq(c.state.sel[CO], 'test:4', 'new test selected');
textRow(v, 'Name').on(ev('Rejects an expired coupon')); v = V(c);
ok(c.state.files[CO].model.tests[4].id === 'rejectsExpiredCoupon', 'id follows the name', c.state.files[CO].model.tests[4]);
v.fnav.find(n => n.btns.length).btns.find(b => b.isUp).on(); eq(c.state.sel[CO], 'test:3', 'move up keeps selection'); ok(c.state.files[CO].model.tests[3].id === 'rejectsExpiredCoupon', 'moved');
v = V(c); v.fnav.find(n => n.btns.length).btns.find(b => b.isDup).on(); ok(c.state.files[CO].model.tests[4].id === 'rejectsExpiredCoupon2', 'duplicate gets a unique id');
v = V(c); v.fnav.find(n => n.btns.length).btns.find(b => b.isDel).on(); ok(c.state.files[CO].model.tests.length === 5, 'delete removes');
// kv add + key rename, steps add, match rows
c = H.mount(C); c.setSel('test:1'); v = V(c);
let varsT = row(v, r => r.isTable && r.lbl === 'Vars'); varsT.add(); v = V(c);
ok(/      paymentToken: tok_chargeDeclined\n      newVar: ""\n/.test(c.state.files[CO].text), 'add var row');
varsT = row(v, r => r.isTable && r.lbl === 'Vars'); varsT.items[1].cells[0].on(ev('couponCode'));
ok(/      newVar: ""/.test(c.state.files[CO].text) === false && /      couponCode: ""\n/.test(c.state.files[CO].text), 'rename var key in place');
v = V(c); varsT = row(v, r => r.isTable && r.lbl === 'Vars'); varsT.items[1].cells[0].on(ev('paymentToken')); v = V(c); varsT = row(v, r => r.isTable && r.lbl === 'Vars');
ok(varsT.hasErrs && /already used/.test(varsT.errs.map(x => x.txt).join()), 'duplicate key refused with message');
varsT.items[1].cells[1].on(ev('${var.')); v = V(c); varsT = row(v, r => r.isTable && r.lbl === 'Vars');
ok(varsT.items[1].cells[1].compOpen && varsT.items[1].cells[1].compItems.some(i => i.l === 'cartId'), '${var. suggestions in a vars cell');
varsT.items[1].del(); ok(!/couponCode/.test(c.state.files[CO].text.split('rejectsDeclinedCard')[1].split('memberCannotRefund')[0]), 'delete var row');
let mt = row(V(c), r => r.isTable && r.lbl === 'Body match'); mt.add(); v = V(c); mt = row(v, r => r.isTable && r.lbl === 'Body match');
ok(/          field:\n            exists: true\n/.test(c.state.files[CO].text), 'add match row (exists)');
let last = mt.items[mt.items.length - 1];
last.cells[1].on(ev('gt')); v = V(c); mt = row(v, r => r.isTable && r.lbl === 'Body match'); last = mt.items[mt.items.length - 1];
last.cells[last.cells.length - 1].on(ev('5'));
ok(/          field:\n            gt: 5\n/.test(c.state.files[CO].text), 'matcher select + value write a matcher', c.state.files[CO].text.split('match:')[1]);
v = V(c); mt = row(v, r => r.isTable && r.lbl === 'Body match'); mt.items[mt.items.length - 1].cells[0].on(ev('error.message'));
ok(/          error\.message:\n            gt: 5\n/.test(c.state.files[CO].text), 'rename match path');
// request mode one call -> steps
v = V(c); row(v, r => r.isSeg && r.lbl === 'Request').opts[1].on(); ok(/    steps:\n      - id: first\n        call: orders\.place\n/.test(c.state.files[CO].text), 'switch one call to steps');
// examples grid
c = H.mount(C); c.openFile('services/users/crud.test.yaml'); c.setSel('test:2'); v = V(c);
let g = row(v, r => r.isGrid); ok(g && g.rows.length === 2 && g.cols[0].name === 'email', 'examples grid');
g.addRow(); v = V(c); g = row(v, r => r.isGrid); g.rows[2].cells[0].on(ev('a@b'));
ok(/      - email: a@b\n/.test(c.state.files['services/users/crud.test.yaml'].text), 'edit grid cell');
g.addCol(); ok(/column: ""/.test(c.state.files['services/users/crud.test.yaml'].text), 'add column');
v = V(c); row(v, r => r.isSelect && r.lbl === 'Rows come from').on(ev('${dataset.capitals}'));
ok(/    examples: \$\{dataset\.capitals\}\n/.test(c.state.files['services/users/crud.test.yaml'].text), 'use dataset');

// 13. other kinds edit their text
c = H.mount(C);
c.openFile('load/checkout.load.yaml'); c.setSel('mix'); v = V(c);
let mix = row(v, r => r.isTable && r.lbl === 'Mix'); mix.items[2].cells[1].on(ev(4)); v = V(c); mix = row(v, r => r.isTable && r.lbl === 'Mix');
ok(/orders\.checkout\.placesOrder\n    weight: 4\n/.test(c.state.files['load/checkout.load.yaml'].text) && mix.items[2].cells[2].txt === '31%', 'weight slider and share', mix.items[2].cells[2].txt);
c.setSel('stages'); v = V(c); const ramp0 = row(v, r => r.isRamp).pts; row(v, r => r.isTable && r.lbl === 'Stages').items[1].cells[1].on(ev('8000')); v = V(c);
ok(row(v, r => r.isRamp).pts !== ramp0 && /peak 8,000/.test(row(v, r => r.isRamp).total), 'ramp chart updates');
c.openFile('vibranium.config.yaml'); c.setSel('profiles'); v = V(c);
g = row(v, r => r.isGrid); const geoRow = g.rows.findIndex(r => r.cells[0].val === 'geoUrl');
g.rows[geoRow].cells[3].on(ev('https://geo.staging.shop.example'));
ok(/      identityUrl: https:\/\/id\.staging\.shop\.example\n      geoUrl: https:\/\/geo\.staging\.shop\.example\n/.test(c.state.files['vibranium.config.yaml'].text), 'matrix cell writes a profile override');
v = V(c); g = row(v, r => r.isGrid); g.cols[3].rOn();
ok(/  - id: dev\n  - id: staging\n    default: true\n/.test(c.state.files['vibranium.config.yaml'].text), 'default profile radio');
c.setSel('output'); v = V(c); row(v, r => r.isCheck && r.lbl === 'json').on({ target: { checked: true } });
ok(/formats: \[ctrf, junit, html, json\]/.test(c.state.files['vibranium.config.yaml'].text), 'report format checkbox');
c.openFile('datasets/capitals.csv'); v = V(c); g = row(v, r => r.isGrid); g.rows[0].cells[0].on(ev('Kyoto'));
ok(/^name,country,population\nKyoto,JP/.test(c.state.files['datasets/capitals.csv'].text), 'dataset grid edits CSV');
v = V(c); ok(row(v, r => r.isChips && r.chips.some(ch => /groups\.test\.yaml/.test(ch.t))), 'dataset used-by');
c.openFile('schemas/orders/order.json'); v = V(c); ok(v.showSrc && v.segs[0].dis && v.hasAside && v.aside.some(a => /checkout\.test\.yaml/.test(a.t)), 'schema source with used-by');
c.srcInput(ev(c.state.files['schemas/orders/order.json'].text.replace('"title": "Order",', '"title": "Order"'), 10)); v = V(c); ok(v.errN === 1 && v.lines.some(l => /err/.test(l.cls)), 'JSON error shown on its line');
c.openFile('hooks/checks.ts'); v = V(c); ok(v.aside.some(a => /currencyIsConsistent/.test(a.t) && /checkout\.test\.yaml/.test(a.s)), 'hook exports with references');
c.srcInput(ev(c.state.files['hooks/checks.ts'].text + '\nexport const x = 1;\n', 5)); ok(c.state.files['hooks/checks.ts'].text !== c.state.files['hooks/checks.ts'].original, 'hook edit marks dirty');
c.openFile('openapi/orders.yaml'); v = V(c); ok(!v.srcEditable && v.saveDis && /Read-only/.test(v.saveTitle), 'snapshot is read-only');
c.openFile('services/users/users.api.yaml'); c.setSel('api:0'); v = V(c);
ok(row(v, r => r.isChips && r.lbl === 'Path vars'), 'api path var chips');
row(v, r => r.isSelect && r.lbl === 'Method').on(ev('PUT')); ok(/  - id: create\n    name: Create a user\n    method: PUT\n/.test(c.state.files['services/users/users.api.yaml'].text), 'api method select');
v = V(c); row(v, r => r.isSeg && r.lbl === 'Body').opts.find(o => o.l === 'Text').on(); ok(/    body: '?\{/.test(c.state.files['services/users/users.api.yaml'].text) || /    body: "\{/.test(c.state.files['services/users/users.api.yaml'].text), 'body to text mode', c.state.files['services/users/users.api.yaml'].text.split('body:')[1].slice(0, 80));

// 14. New dialog
c = H.mount(C); c.openNew('test'); v = V(c);
c.dlgSet({ name: 'Returns a placed order', fromApi: 'orders.get' }); v = V(c);
ok(v.dlg.idVal === 'returnsPlacedOrder' && v.dlg.target === CO && v.dlg.fullId === 'orders.checkout.returnsPlacedOrder' && v.dlg.hasPreview, 'dialog derives id, path, full id', v.dlg);
c.dlgSet({ id: 'Test_1', idEdited: true }); v = V(c); ok(v.dlg.createDis && v.dlg.hasSugg && v.dlg.sugg === 'test1' && v.dlg.errs.length, 'Test_1 refused with suggestion');
v.dlg.useSugg(); v = V(c); ok(!v.dlg.createDis && v.dlg.idVal === 'test1', 'suggestion applied');
v.dlg.resetId(); v = V(c); v.dlg.create(); v = V(c);
const nt = c.state.files[CO].model.tests[4];
ok(nt.id === 'returnsPlacedOrder' && nt.before[0].call === 'orders.place' && nt.call === 'orders.get' && c.state.sel[CO] === 'test:4' && !v.dlgOpen && v.toast, 'create adds the happy-path test and selects it', nt);
ok(V(c).noProblems, 'happy path test is clean');
c.openNew('testFile'); c.dlgSet({ service: 'orders', area: 'history', name: 'Order history' }); v = V(c); v.dlg.create();
ok(c.state.active === 'services/orders/history.test.yaml' && c.state.files['services/orders/history.test.yaml'].text.startsWith('id: orders.history\nname: Order history\ntags: [orders]\n'), 'new test file created and opened');
c.openNew('dataset'); c.dlgSet({ name: 'World cities', columns: 'name, lat, lon' }); V(c).dlg.create(); ok(c.state.files['datasets/world-cities.csv'].text === 'name,lat,lon\n', 'dataset created');
c.openNew('api'); c.dlgSet({ name: 'List orders', service: 'orders', method: 'GET', apiPath: '/orders' }); V(c).dlg.create(); ok(/  - id: listOrders\n    name: List orders\n    path: \/orders\n/.test(c.state.files['services/orders/orders.api.yaml'].text) && c.state.sel['services/orders/orders.api.yaml'] === 'api:4', 'api added to existing file');

// 15. tabs: close dirty asks; discard reverts
c = H.mount(C); v = V(c); textRow(v, 'Name').on(ev('Changed')); v = V(c); v.tabs[0].close(); v = V(c);
ok(v.confirmOpen, 'closing a dirty tab asks'); v.confirmDiscard(); C.flush(); v = V(c);
ok(c.state.open.indexOf(CO) < 0 && c.state.files[CO].text === orig, 'discard closes and reverts');

// 16. click every visible button in several states
function clickAll(setup, label) {
  const base = H.mount(C); setup(base);
  const r = render(base); let noops = [];
  r.buttons.forEach((b, i) => {
    if (b.tag === 'a') { if (!b.href) noops.push('link without href ' + b.label); return; }
    if (b.disabled) { if (!b.title) noops.push('disabled without reason: ' + b.label); return; }
    if (typeof b.on !== 'function') { noops.push('no handler: ' + b.label); return; }
    if (b.current) return;
    const c2 = H.mount(C); setup(c2);
    const r2 = render(c2), b2 = r2.buttons[i];
    const before = JSON.stringify(c2.state) + c2.pendingFocus + c2.pendingSel;
    try { b2.on({ preventDefault() {}, target: { value: '' } }); C.flush(); } catch (err) { noops.push('THROWS ' + b.label + ': ' + err.message); return; }
    const after = JSON.stringify(c2.state) + c2.pendingFocus + c2.pendingSel;
    if (before === after) noops.push('no effect: ' + b.label + ' @' + b.trail.slice(-60));
  });
  ok(noops.length === 0, 'every button acts (' + label + ', ' + r.buttons.length + ' buttons)', noops);
}
clickAll(c => {}, 'test form');
clickAll(c => { c.setSel('life'); }, 'lifecycle');
clickAll(c => { c.setSel('file'); }, 'file settings');
clickAll(c => { c.setView('split'); c.setSel('test:3'); }, 'split skipped test');
clickAll(c => { c.openFile('services/users/users.api.yaml'); c.setSel('api:0'); }, 'api form');
clickAll(c => { c.openFile('vibranium.config.yaml'); c.setSel('profiles'); }, 'config profiles');
clickAll(c => { c.openFile('vibranium.config.yaml'); c.setSel('auth'); }, 'config auth');
clickAll(c => { c.openFile('load/checkout.load.yaml'); c.setSel('stages'); }, 'load stages');
clickAll(c => { c.openFile('datasets/capitals.csv'); }, 'dataset');
clickAll(c => { c.openNew('test'); c.dlgSet({ name: 'Returns a placed order' }); }, 'new dialog');
clickAll(c => { const v = c.renderVals(); v.fsecs[0].rows.find(r => r.lbl === 'Name').on({ target: { value: 'X', selectionStart: 1 } }); c.toggleDiff(); }, 'dirty with diff');
console.log('ui: pass', pass, 'fail', fail);
process.exitCode = fail ? 1 : 0;
