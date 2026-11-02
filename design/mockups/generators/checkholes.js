// Usage: node checkholes.js file.dc.html [...]
// Verifies every {{hole}} in the <x-dc> template resolves against renderVals(),
// with <sc-for list="{{x}}" as="y"> scopes; calls every handler once and re-renders.
const fs = require('fs');
let failed = false;
for (const file of process.argv.slice(2)) {
  const src = fs.readFileSync(file, 'utf8');
  const tpl = src.slice(src.indexOf('<x-dc>'), src.indexOf('</x-dc>'));
  const m = src.match(/<script type="text\/x-dc" data-dc-script[^>]*>([\s\S]*?)<\/script>/);
  const props = JSON.parse(src.match(/data-props='([^']*)'/)[1].replace(/&amp;/g, '&').replace(/&#39;/g, "'"));
  class DCLogic {
    constructor(p) { this.props = p || {}; this.state = {}; }
    setState(u) { Object.assign(this.state, typeof u === 'function' ? u(this.state) : u); }
    forceUpdate() {}
  }
  const Component = new Function('DCLogic', 'navigator', m[1] + '\nreturn Component;')(DCLogic, { clipboard: { writeText: () => Promise.resolve() } });
  const c = new Component({});
  let vals = c.renderVals();
  const problems = [];
  // tokenize tags & holes
  const re = /<(\/?)(sc-for|sc-if)\b([^>]*)>|\{\{\s*([^}]+?)\s*\}\}/g;
  const scope = []; // {as, list(path)}
  let mm, holes = 0;
  const get = (obj, path) => path.split('.').reduce((o, k) => (o == null ? undefined : o[k]), obj);
  function resolveAll(path, sc) {
    // returns array of candidate values
    const segs = path.split('.');
    if (segs[0] === '$index') return [0];
    if (/^(true|false|null|-?\d+(\.\d+)?)$/.test(path)) return [path];
    for (let i = sc.length - 1; i >= 0; i--) {
      if (sc[i].as === segs[0]) {
        const lists = resolveAll(sc[i].list, sc.slice(0, i));
        const items = [].concat(...lists.filter(Array.isArray));
        if (!lists.length || lists.some((l) => !Array.isArray(l))) return [undefined];
        if (!items.length) return ['__empty__'];
        return items.map((it) => (segs.length === 1 ? it : get(it, segs.slice(1).join('.'))));
      }
    }
    return [get(vals, path)];
  }
  while ((mm = re.exec(tpl))) {
    if (mm[2]) {
      if (mm[1]) { if (mm[2] === 'sc-for') scope.pop(); continue; }
      if (mm[2] === 'sc-for') {
        const list = (mm[3].match(/list="\{\{\s*([^}]+?)\s*\}\}"/) || [])[1];
        const as = (mm[3].match(/as="([^"]+)"/) || [])[1];
        if (!list || !as) problems.push('sc-for without list/as: ' + mm[0]);
        const r = resolveAll(list, scope);
        if (r.some((x) => x !== '__empty__' && !Array.isArray(x))) problems.push(`sc-for list not an array: ${list}`);
        scope.push({ as, list });
      }
      const v = (mm[3].match(/value="\{\{\s*([^}]+?)\s*\}\}"/) || [])[1];
      if (mm[2] === 'sc-if') {
        holes++;
        const r = resolveAll(v, scope);
        if (r.every((x) => x === undefined)) problems.push(`sc-if value undefined: ${v}`);
      }
      continue;
    }
    holes++;
    const path = mm[4];
    if (/[^\w.$]/.test(path)) { problems.push('expression in hole: ' + path); continue; }
    const r = resolveAll(path, scope);
    if (r.includes('__empty__')) continue;
    const undef = r.filter((x) => x === undefined).length;
    if (undef === r.length) problems.push('undefined: ' + path);
    else if (undef) problems.push(`undefined in ${undef}/${r.length} items: ${path}`);
  }
  if (scope.length) problems.push('unbalanced sc-for');
  // exercise handlers
  let calls = 0;
  const ev = { target: { value: 'order', checked: true }, preventDefault() {}, stopPropagation() {} };
  function walk(o, depth) {
    if (depth > 4 || o == null) return;
    if (typeof o === 'function') { try { o(ev); calls++; vals = c.renderVals(); } catch (e) { problems.push('handler threw: ' + e.message); } return; }
    if (Array.isArray(o)) { o.slice(0, 3).forEach((x) => walk(x, depth + 1)); return; }
    if (typeof o === 'object') for (const k of Object.keys(o)) walk(o[k], depth + 1);
  }
  walk(c.renderVals(), 0);
  try { c.renderVals(); } catch (e) { problems.push('renderVals threw after handlers: ' + e.message); }
  // tag balance (rough): count opens/closes of common containers
  const bal = {};
  for (const t of ['div', 'section', 'span', 'a', 'button', 'svg', 'sc-if', 'sc-for', 'label', 'ul', 'li', 'p', 'nav', 'header', 'main', 'pre', 'table', 'tr', 'td', 'th', 'thead', 'tbody', 'dl', 'g', 'text', 'h1', 'h2', 'h3', 'code', 'figure', 'b']) {
    const o = (tpl.match(new RegExp('<' + t + '[\\s>]', 'g')) || []).length;
    const cl = (tpl.match(new RegExp('</' + t + '>', 'g')) || []).length;
    if (o !== cl) bal[t] = `${o} open / ${cl} close`;
  }
  if (Object.keys(bal).length) problems.push('tag balance: ' + JSON.stringify(bal));
  const head = src.slice(0, src.indexOf('</head>'));
  if (!head.includes('<script src="./support.js"></script>')) problems.push('support.js line missing');
  if (/[\u{1F300}-\u{1FAFF}✅❌✓✕✗]/u.test(tpl)) problems.push('emoji/check glyphs in template');
  console.log(`${file.split('/').pop()}: ${holes} holes, ${calls} handlers exercised, preview ${props.$preview.width}x${props.$preview.height}` + (problems.length ? `\n  PROBLEMS:\n  - ${problems.join('\n  - ')}` : ' — OK'));
  if (problems.length) failed = true;
}
process.exit(failed ? 1 : 0);
