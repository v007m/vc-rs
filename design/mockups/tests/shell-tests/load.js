// Loads an artboard's logic class in Node with a stub DCLogic and fake timers.
const fs = require('fs');
const path = require('path');
const PROJ = path.resolve(__dirname, '../vibmock/project');
function readArtboard(name) { return fs.readFileSync(path.join(PROJ, name), 'utf8'); }
function extractScript(html) {
  const start = html.indexOf('<script type="text/x-dc"');
  const open = html.indexOf('>', start) + 1;
  return html.slice(open, html.indexOf('</script>', open));
}
function makeTimers() {
  let now = 0, seq = 1; const q = new Map();
  return {
    setTimeout(fn, ms) { const id = seq++; q.set(id, { fn, at: now + (ms || 0) }); return id; },
    clearTimeout(id) { q.delete(id); },
    setInterval(fn, ms) { const id = seq++; const tick = () => { q.set(id, { fn: () => { fn(); tick(); }, at: now + ms }); }; tick(); return id; },
    clearInterval(id) { q.delete(id); },
    advance(ms) { const target = now + ms; for (;;) { let next = null; for (const [id, t] of q) if (t.at <= target && (!next || t.at < next.t.at)) next = { id, t }; if (!next) break; q.delete(next.id); now = next.t.at; next.t.fn(); } now = target; },
    flush(max) { this.advance(max || 1e6); }
  };
}
function loadComponent(name) {
  const html = readArtboard(name);
  const src = extractScript(html);
  const timers = makeTimers();
  class DCLogic {
    constructor(props) { this.props = props || {}; this.state = null; }
    setState(p) { const patch = typeof p === 'function' ? p(this.state) : p; this.state = Object.assign({}, this.state, patch); }
    forceUpdate() {}
  }
  const navigator = { clipboard: { writeText: () => Promise.resolve() } };
  const factory = new Function('DCLogic', 'setTimeout', 'clearTimeout', 'setInterval', 'clearInterval', 'navigator', src + '\nreturn Component;');
  const Component = factory(DCLogic, timers.setTimeout.bind(timers), timers.clearTimeout.bind(timers), timers.setInterval.bind(timers), timers.clearInterval.bind(timers), navigator);
  return { Component, timers, html };
}
module.exports = { loadComponent, readArtboard };
