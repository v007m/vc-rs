import sys
P = '/tmp/claude-0/-home-claude/80b4900a-8223-5c77-874b-c1dda2eeca25/scratchpad/vibmock/project/ReportLoad.dc.html'
s = open(P).read()
def rep(a, b):
    global s
    if a not in s:
        print('MISSING:', a[:140]); sys.exit(1)
    s = s.replace(a, b, 1)

rep('/_blob/1e69a6d642232c67587a282225e98669', '/_blob/81dcd3b549d078b575adb6aed82e1861')
rep('.sr{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap}',
    '.sr{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap}\n.panel{position:relative}\n.panel .sr{left:0;top:0}')
rep('.lg{display:inline-flex;', '.lgb{display:inline-flex;')
s = s.replace('.lg:hover', '.lgb:hover').replace('.lg.off', '.lgb.off').replace('.lg .sw', '.lgb .sw').replace('.lg.static', '.lgb.static')
s = s.replace('class="lg ', 'class="lgb ')
rep('@media (max-width:860px){.sidefilter{display:none}.verdict{font-size:22px}.th > div{padding-left:10px}}\n', '''.navrow{display:flex;flex-direction:column;gap:2px}
.side-head .navlabel{padding:0 8px}
.ctx .seg{flex-wrap:wrap}
.runid,.acts{gap:6px}
.narrow-only{display:none}
.morewrap{position:relative}
.morewrap .popover{position:absolute;right:0;top:calc(100% + 6px);z-index:30;min-width:220px}
.morewrap .popover a.opt{text-decoration:none}
.gt .tr > *{min-width:0;overflow-wrap:anywhere}
.errbtn{border:0;background:transparent;font:inherit;color:inherit;text-align:left;cursor:pointer;padding:0;display:flex;align-items:center;gap:8px;min-width:0}
.errbtn svg.chev{transition:transform .15s;flex:none;color:var(--ink2)}
.errbtn.open svg.chev{transform:rotate(90deg)}
.xp{padding:12px 16px 14px 40px;background:var(--panel2);border-bottom:1px solid var(--line);display:flex;flex-direction:column;gap:8px}
.samp{display:grid;grid-template-columns:90px minmax(130px,.8fr) 64px 80px minmax(0,2fr);gap:4px 14px;font-size:12.5px;align-items:baseline;padding:4px 0;border-bottom:1px solid var(--line)}
.samp:last-child{border-bottom:0}
.samp > *{min-width:0;overflow-wrap:anywhere}
.loadline{display:flex;align-items:center;flex-wrap:wrap;gap:6px 10px;font-size:13px;color:var(--ink2);padding:4px 0}
.loadline .mono{overflow-wrap:anywhere;min-width:0}
.spin{width:14px;height:14px;border-radius:50%;border:2px solid var(--line2);border-top-color:var(--accent);animation:spin .8s linear infinite;flex:none}
@keyframes spin{to{transform:rotate(360deg)}}
.about{font-size:12.5px;color:var(--ink2);line-height:1.55;padding:10px 12px;background:var(--panel2);border-radius:9px}
@container (max-width: 760px){
  .verdict{font-size:22px}
  .lanes{min-width:0}
  .lane-axis{display:none}
  .lane{grid-template-columns:minmax(0,1fr);row-gap:6px}
  .samp{grid-template-columns:auto auto minmax(0,1fr)}
  .samp .sm-body{grid-column:1 / -1}
  .xp{padding-left:16px}
}
@container (max-width: 640px){
  .th > div{border-left:0;padding-left:0}
  .th > div.breach{padding-left:12px}
}
@media (max-width: 860px){
  .app .side{flex-direction:column;overflow:visible;padding-bottom:4px}
  .app .side .navrow{flex-direction:row;overflow-x:auto;gap:4px}
  .app .side .ctx{display:none}
  .app.side-collapsed .side .navitem{width:auto;padding:0 10px;margin:0;height:32px}
}
@media (max-width: 760px){
  .wide-only{display:none}
  .narrow-only{display:inline-flex}
}
''')
rep('<div class="app">', '<div class="{{appCls}}">')
rep('''    <div class="row" style="gap: 6px">
      <span class="pill fail"><span class="dot fail"></span>Breached</span>''', '''    <div class="cluster runid">
      <span class="pill fail"><span class="dot fail"></span>Breached</span>''')
rep('''    <div class="row" style="gap: 6px">
      <a class="btn" href="WebRun.dc.html">Open in web UI</a>
      <a class="btn" href="ctrf.json" download="load-57.ctrf.json">Download CTRF</a>''', '''    <div class="cluster acts">
      <a class="btn" href="WebRun.dc.html">Open in web UI</a>
      <a class="btn wide-only" href="ctrf.json" download="load-57.ctrf.json">Download CTRF</a>
      <div class="morewrap narrow-only">
        <button class="btn" type="button" aria-expanded="{{moreOpen}}" aria-haspopup="true" onClick="{{toggleMore}}">More<svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" aria-hidden="true"><path d="m3 4.5 3 3 3-3"/></svg></button>
        <sc-if value="{{moreOpen}}" hint-placeholder-val="{{ false }}">
          <div class="popover">
            <a class="opt" href="ctrf.json" download="load-57.ctrf.json">Download CTRF</a>
            <a class="opt" href="samples.ndjson.gz" download="load-57.samples.ndjson.gz">Raw samples</a>
          </div>
        </sc-if>
      </div>''')
rep('''    <nav class="side" aria-label="Report sections">
      <div class="navlabel">On this page</div>
      <sc-for list="{{nav}}" as="n" hint-placeholder-count="12">
        <a class="navitem {{n.cls}}" href="{{n.href}}" onClick="{{n.go}}"><svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true"><path d="{{n.icon}}" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"></path></svg>{{n.label}}<span class="count">{{n.count}}</span></a>
      </sc-for>
      <div class="sidefilter" style="display: flex; flex-direction: column; gap: 2px">''', '''    <div class="side">
      <div class="side-head">
        <span class="navlabel">On this page</span>
        <button class="iconbtn" type="button" aria-label="{{collapseLabel}}" title="{{collapseLabel}}" aria-expanded="{{sideOpen}}" onClick="{{toggleSide}}">
          <sc-if value="{{sideOpen}}" hint-placeholder-val="{{ true }}"><svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="2" y="3" width="12" height="10" rx="2"/><path d="M6 3v10M10.5 6.5 9 8l1.5 1.5"/></svg></sc-if>
          <sc-if value="{{sideShut}}" hint-placeholder-val="{{ false }}"><svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="2" y="3" width="12" height="10" rx="2"/><path d="M6 3v10M9 6.5 10.5 8 9 9.5"/></svg></sc-if>
        </button>
      </div>
      <nav class="navrow" aria-label="Report sections">
      <sc-for list="{{nav}}" as="n" hint-placeholder-count="12">
        <a class="navitem {{n.cls}}" href="{{n.href}}" title="{{n.label}}" onClick="{{n.go}}"><svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true"><path d="{{n.icon}}" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"></path></svg><span class="label">{{n.label}}</span><span class="count">{{n.count}}</span></a>
      </sc-for>
      </nav>
      <div class="ctx">''')
rep('''<div style="padding: 0 6px"><div class="seg" role="group" aria-label="Group errors by"><button type="button" class="{{errByStatusCls}}" aria-pressed="{{errByStatus}}" onClick="{{showErrStatus}}">Status code</button><button type="button" class="{{errByApiCls}}" aria-pressed="{{errByApi}}" onClick="{{showErrApi}}">Api</button></div></div>
      </div>
    </nav>''', '''<div style="padding: 0 6px"><div class="seg" role="group" aria-label="Group errors by"><button type="button" class="{{errByStatusCls}}" aria-pressed="{{errByStatus}}" onClick="{{showErrStatus}}">Status code</button><button type="button" class="{{errByApiCls}}" aria-pressed="{{errByApi}}" onClick="{{showErrApi}}">Api</button></div></div>
      </div>
    </div>''')
# errors: expandable rows with lazily loaded samples
rep('''<span class="small muted">2,034 failed requests · 30 s buckets</span>''', '''<span class="small muted">2,034 failed requests · 30 s buckets · open a row to load sample requests</span>''')
rep('''                <div class="tr" role="row">
                  <span role="cell" class="row" style="gap: 8px; flex-wrap: nowrap"><span class="sw" style="width: 10px; height: 10px; border-radius: 3px; display: inline-block; flex: none; background: {{er.fill}}"></span><span class="{{er.labelCls}}">{{er.label}}</span></span>''', '''                <div class="tr" role="row">
                  <span role="cell"><button type="button" class="errbtn {{er.openCls}}" aria-expanded="{{er.open}}" onClick="{{er.toggle}}"><svg class="chev" width="12" height="12" viewBox="0 0 16 16" aria-hidden="true"><path d="m6 3.5 4.5 4.5L6 12.5" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"></path></svg><span class="sw" style="width: 10px; height: 10px; border-radius: 3px; display: inline-block; flex: none; background: {{er.fill}}"></span><span class="{{er.labelCls}}">{{er.label}}</span></button></span>''')
rep('''                  <span role="cell" class="cellname"><span class="small">{{er.first}}</span><span class="mono small muted" style="overflow-wrap: anywhere">{{er.sample}}</span></span>
                </div>''', '''                  <span role="cell" class="cellname"><span class="small">{{er.first}}</span><span class="mono small muted" style="overflow-wrap: anywhere">{{er.sample}}</span></span>
                </div>
                <sc-if value="{{er.open}}" hint-placeholder-val="{{ false }}">
                  <div class="xp" role="row">
                    <sc-if value="{{er.loading}}" hint-placeholder-val="{{ false }}"><div class="loadline" role="cell"><span class="spin" aria-hidden="true"></span>Loading sample requests… <span class="mono small">{{er.file}}</span></div></sc-if>
                    <sc-if value="{{er.ready}}" hint-placeholder-val="{{ true }}">
                      <div role="cell" style="display: flex; flex-direction: column; min-width: 0">
                        <div class="small muted" style="padding-bottom: 4px">3 of {{er.count}} kept as samples · bodies up to 4 KB · secrets redacted</div>
                        <sc-for list="{{er.samples}}" as="sp" hint-placeholder-count="3">
                          <div class="samp"><span class="mono">{{sp.t}}</span><span class="mono">{{sp.api}}</span><span class="mono">{{sp.code}}</span><span class="mono">{{sp.ms}}</span><span class="mono muted sm-body">{{sp.body}}</span></div>
                        </sc-for>
                      </div>
                    </sc-if>
                  </div>
                </sc-if>''')
rep('''                <dt>Artifacts</dt><dd class="mono">.vibranium/reports/load-57/ctrf.json · samples.ndjson.gz</dd>
              </dl>''', '''                <dt>Artifacts</dt><dd class="mono">.vibranium/reports/load-57/ctrf.json · samples.ndjson.gz</dd>
              </dl>
              <div class="about"><b style="color: #1B1830; font-weight: 600">This report:</b> index 96 KB · 10 s aggregates for every chart · 3 sample requests kept per error group, loaded when you open the row · 968,412 raw samples stay in <span class="mono">samples.ndjson.gz</span> and are never loaded by this page.</div>''')

# JS
rep("this.state = { show: { users: true, rps: true, p50: true, p95: true, p99: true }, errBy: 'status', sortBy: 'requests', stage: null, sec: 'summary', copied: false };",
    "this.state = { show: { users: true, rps: true, p50: true, p95: true, p99: true }, errBy: 'status', sortBy: 'requests', stage: null, sec: 'summary', copied: false, sideCollapsed: false, moreOpen: false, errOpen: {}, errLoaded: {}, errLoading: {} };")
rep('''      errRows: errSeries.map(([label, fill, vals, split, first, sample]) => {
        const total = vals.reduce((a, b) => a + b, 0);
        return { label, fill, labelCls: s.errBy === 'api' ? 'mono' : '', count: total.toLocaleString('en-US'), share: ((total / errTotal) * 100).toFixed(1) + '%', split, first: 'first at ' + first, sample };
      }),''', '''      errRows: errSeries.map(([label, fill, vals, split, first, sample]) => {
        const total = vals.reduce((a, b) => a + b, 0);
        const key = s.errBy + ':' + label;
        const open = !!(s.errOpen || {})[key];
        const loading = !!(s.errLoading || {})[key];
        const toggle = () => {
          const st = this.state; const opening = !st.errOpen[key];
          const patch = { errOpen: Object.assign({}, st.errOpen, { [key]: opening }) };
          if (opening && !st.errLoaded[key]) {
            patch.errLoading = Object.assign({}, st.errLoading, { [key]: true });
            setTimeout(() => this.setState({ errLoaded: Object.assign({}, this.state.errLoaded, { [key]: true }), errLoading: Object.assign({}, this.state.errLoading, { [key]: false }) }), 450);
          }
          this.setState(patch);
        };
        return { label, fill, labelCls: s.errBy === 'api' ? 'mono' : '', count: total.toLocaleString('en-US'), share: ((total / errTotal) * 100).toFixed(1) + '%', split, first: 'first at ' + first, sample,
          open, openCls: open ? 'open' : '', loading, ready: !loading, toggle,
          file: 'samples/errors/' + label.split(' ')[0].replace(/\\./g, '-') + '.json · 3 KB',
          samples: sampleRows(label, first, sample) };
      }),
      appCls: 'app' + (s.sideCollapsed ? ' side-collapsed' : ''), sideOpen: !s.sideCollapsed, sideShut: !!s.sideCollapsed,
      collapseLabel: s.sideCollapsed ? 'Expand sidebar' : 'Collapse sidebar', toggleSide: () => this.setState({ sideCollapsed: !this.state.sideCollapsed }),
      moreOpen: !!s.moreOpen, toggleMore: () => this.setState({ moreOpen: !this.state.moreOpen }),''')
rep('''function stackPaths(series) {''', '''const SAMPLE_API = { '503': 'orders.place', '504': 'catalog.search', '429': 'users.get', '500': 'catalog.search' };
function sampleRows(label, first, sample) {
  const code = /^\\d{3}/.test(label) ? label.slice(0, 3) : (sample.match(/^\\d{3}/) || ['504'])[0];
  const api = /^\\d{3}/.test(label) ? SAMPLE_API[code] : label;
  const m = first.match(/(\\d)m(\\d\\d)s/); const base = m ? (+m[1]) * 60 + (+m[2]) : 120;
  const body = sample.replace(/^\\d{3} /, '');
  return [0, 71, 133].map((d, i) => {
    const t = base + d; const mm = Math.floor(t / 60); const ss = String(t % 60).padStart(2, '0');
    return { t: mm + 'm' + ss + 's', api, code, ms: (code === '504' ? [5004, 5002, 5001] : code === '429' ? [3, 4, 3] : [212, 188, 341])[i].toLocaleString('en-US') + ' ms', body: body };
  });
}
function stackPaths(series) {''')
open(P, 'w').write(s)
print('ok')
