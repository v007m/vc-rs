// Runs every WebSettings state through the strict hole checker and prints key facts.
const { loadComponent } = require('./load');
const { parse, check } = require('./holes');
const list = require('./settings-states.js').WebSettings;
const ev = (value, checked) => ({ target: { value, checked }, preventDefault() {}, stopPropagation() {} });
let bad = 0;
for (const [name, fn] of list) {
  const { Component, timers, html } = loadComponent('WebSettings.dc.html');
  const tpl = html.slice(html.indexOf('<x-dc>') + 6, html.indexOf('</x-dc>')).replace(/<helmet>[\s\S]*?<\/helmet>/, '');
  const tree = parse(tpl);
  if (tree.errors.length) { console.log('PARSE', tree.errors); bad++; }
  const c = new Component({});
  const v = () => c.renderVals();
  try { fn(c, v, timers, ev); } catch (e) { console.log('!!', name, 'threw', e.stack.split('\n').slice(0, 3).join(' | ')); bad++; continue; }
  const vals = v();
  const report = { err: [], ok: new Set(), seen: new Set(), ifTrue: new Set(), ifFalse: new Set(), forNonEmpty: new Set() };
  check(tree.root, vals, name, report);
  const facts = [vals.pendLabel, vals.hasProblems ? 'PROBLEM: ' + vals.problemText : '', vals.toast].filter(Boolean).join(' | ');
  console.log((report.err.length ? '!! ' : 'ok ') + name.padEnd(20), facts.slice(0, 200));
  if (name.startsWith('auth-test') || name === 'auth-flow' || name === 'wizard-created') console.log('     test:', vals.tst.steps.map((x) => x.stLabel).join(','), '|', vals.tst.summary, '|', vals.tst.fixTitle, '|', vals.tst.headers.map((h) => h.k + ': ' + h.v).join('; '), '|', vals.tst.expiry);
  if (name === 'diff') console.log('     diff:', vals.df.summary, vals.df.lines.filter((l) => l.cls !== 'ctx').map((l) => l.sign + l.segs.map((s) => s.t).join('')).join(' / ').slice(0, 600));
  if (name === 'secrets-checked') console.log('     ', vals.sx.last);
  if (name === 'alerts-sent') console.log('     ', vals.al.channels.map((x) => x.st + ' ' + x.msg).join(' | '));
  if (name === 'agents') console.log('     ', vals.ag.summary);
  report.err.slice(0, 8).forEach((e) => console.log('      ', e));
  if (report.err.length) bad++;
}
process.exitCode = bad ? 1 : 0;
