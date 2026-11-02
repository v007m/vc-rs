// Loads the shipped artboard's logic class in Node with a stub DCLogic and fake timers.
const fs = require('fs');
const path = require('path');
const ART = path.resolve(__dirname, '../vibmock/project/WebClient.dc.html');

function readArtboard() { return fs.readFileSync(ART, 'utf8'); }
function extractScript(html) {
  const start = html.indexOf('<script type="text/x-dc"');
  const open = html.indexOf('>', start) + 1;
  const end = html.indexOf('</script>', open);
  return html.slice(open, end);
}
function extractTemplate(html) {
  const a = html.indexOf('<x-dc>') + '<x-dc>'.length;
  const b = html.indexOf('</x-dc>');
  let t = html.slice(a, b);
  t = t.replace(/<helmet>[\s\S]*?<\/helmet>/, '');
  return t;
}

function makeTimers() {
  let now = 0, seq = 1;
  const q = new Map();
  return {
    setTimeout(fn, ms) { const id = seq++; q.set(id, { fn, at: now + (ms || 0) }); return id; },
    clearTimeout(id) { q.delete(id); },
    pending() { return q.size; },
    advance(ms) {
      const target = now + ms;
      for (;;) {
        let next = null;
        for (const [id, t] of q) if (t.at <= target && (!next || t.at < next.t.at)) next = { id, t };
        if (!next) break;
        q.delete(next.id); now = next.t.at; next.t.fn();
      }
      now = target;
    },
    flush() { this.advance(1e9); }
  };
}

function loadComponent() {
  const html = readArtboard();
  const src = extractScript(html);
  const timers = makeTimers();
  class DCLogic {
    constructor(props) { this.props = props || {}; this.state = {}; this.renders = 0; }
    setState(p) { const patch = typeof p === 'function' ? p(this.state) : p; this.state = Object.assign({}, this.state, patch); }
    forceUpdate() {}
  }
  const navigator = { clipboard: { writeText: () => Promise.resolve() } };
  const factory = new Function('DCLogic', 'setTimeout', 'clearTimeout', 'navigator', src + '\nreturn Component;');
  const Component = factory(DCLogic, timers.setTimeout.bind(timers), timers.clearTimeout.bind(timers), navigator);
  return { Component, timers, html, template: extractTemplate(html) };
}
module.exports = { loadComponent, readArtboard, extractScript, extractTemplate };
