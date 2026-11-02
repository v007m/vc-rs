import sys
P = '/tmp/claude-0/-home-claude/80b4900a-8223-5c77-874b-c1dda2eeca25/scratchpad/vibmock/project/ReportFailure.dc.html'
s = open(P).read()
def rep(a, b):
    global s
    if a not in s:
        print('MISSING:', a[:140]); sys.exit(1)
    s = s.replace(a, b, 1)

rep('/_blob/1e69a6d642232c67587a282225e98669', '/_blob/81dcd3b549d078b575adb6aed82e1861')
rep('@media (max-width:1100px){.exsplit{grid-template-columns:minmax(0,1fr)}}\n', '')
rep('@media (max-width:860px){.sidefilter{display:none}.tid{font-size:19px}.kn > div{padding-left:10px}}\n', '''.expander{white-space:normal}
.navrow{display:flex;flex-direction:column;gap:2px}
.side-head .navlabel{padding:0 8px}
.runid,.acts{gap:6px}
.narrow-only{display:none}
.morewrap{position:relative}
.morewrap .popover{position:absolute;right:0;top:calc(100% + 6px);z-index:30;min-width:220px}
.morewrap .popover a.opt{text-decoration:none}
.gt .tr > *{min-width:0;overflow-wrap:anywhere}
.loadline{display:flex;align-items:center;gap:10px;font-size:13px;color:var(--ink2);padding:6px 0}
.spin{width:14px;height:14px;border-radius:50%;border:2px solid var(--line2);border-top-color:var(--accent);animation:spin .8s linear infinite;flex:none}
@keyframes spin{to{transform:rotate(360deg)}}
.skel span{display:block;height:10px;border-radius:5px;background:var(--panel3);margin:8px 0}
.about{font-size:12.5px;color:var(--ink2);line-height:1.55}
@container (max-width: 960px){
  .exsplit{grid-template-columns:minmax(0,1fr)}
}
@container (max-width: 760px){
  .tl{min-width:0}
  .tl-axis{display:none}
  .tl-row{grid-template-columns:minmax(0,1fr) auto;grid-template-areas:"nm tm" "tr tr";row-gap:6px}
  .tl-row .tl-name{grid-area:nm}
  .tl-row .track{grid-area:tr}
  .tl-row .tl-time{grid-area:tm}
  .tl-row.sub .tl-name{padding-left:14px}
  .tid{font-size:19px}
}
@container (max-width: 640px){
  .kn > div{border-left:0;padding-left:0}
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
      <span class="tchip">profile staging</span>''', '''    <div class="cluster runid">
      <span class="pill fail"><span class="dot fail"></span>Failed</span>
      <span class="tchip">profile staging</span>''')
rep('''    <div class="row" style="gap: 6px">
      <a class="btn" href="WebEditor.dc.html">Open in web UI</a>
      <a class="btn" href="ctrf.json" download="run-482.ctrf.json">Download CTRF</a>''', '''    <div class="cluster acts">
      <a class="btn" href="WebEditor.dc.html">Open in web UI</a>
      <a class="btn wide-only" href="ctrf.json" download="run-482.ctrf.json">Download CTRF</a>
      <div class="morewrap narrow-only">
        <button class="btn" type="button" aria-expanded="{{moreOpen}}" aria-haspopup="true" onClick="{{toggleMore}}">More<svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" aria-hidden="true"><path d="m3 4.5 3 3 3-3"/></svg></button>
        <sc-if value="{{moreOpen}}" hint-placeholder-val="{{ false }}">
          <div class="popover">
            <a class="opt" href="ctrf.json" download="run-482.ctrf.json">Download CTRF</a>
            <a class="opt" href="Main.dc.html">Back to run #482</a>
          </div>
        </sc-if>
      </div>''')
rep('''    <nav class="side" aria-label="Failure sections">
      <div class="navlabel">On this page</div>
      <sc-for list="{{nav}}" as="n" hint-placeholder-count="10">
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
      <nav class="navrow" aria-label="Failure sections">
      <sc-for list="{{nav}}" as="n" hint-placeholder-count="10">
        <a class="navitem {{n.cls}}" href="{{n.href}}" title="{{n.label}}" onClick="{{n.go}}"><svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true"><path d="{{n.icon}}" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"></path></svg><span class="label">{{n.label}}</span><span class="count">{{n.count}}</span></a>
      </sc-for>
      </nav>
      <div class="ctx">''')
rep('''        <a class="navitem" href="Main.dc.html"><span class="dot fail" style="margin: 0 4px"></span><span class="mono" style="font-size: 12px">reordersPreviousCart</span></a>
      </div>
    </nav>''', '''        <a class="navitem" href="Main.dc.html"><span class="dot fail" style="margin: 0 4px"></span><span class="mono" style="font-size: 12px">reordersPreviousCart</span></a>
      </div>
    </div>''')
# exchange loading
rep('''          <sc-if value="{{ex.live}}" hint-placeholder-val="{{ true }}">
            <div class="exhead">''', '''          <sc-if value="{{exLoading}}" hint-placeholder-val="{{ false }}">
            <div class="loadline" role="status"><span class="spin" aria-hidden="true"></span>Loading exchanges… <span class="mono small">{{exFile}}</span></div>
            <div class="skel" aria-hidden="true" style="max-width: 720px"><span style="width: 45%"></span><span style="width: 90%"></span><span style="width: 75%"></span><span style="width: 60%"></span></div>
          </sc-if>
          <sc-if value="{{exReady}}" hint-placeholder-val="{{ true }}">
            <div class="exhead">''')
rep('''<span role="cell" class="mono small">{{sm.check}}</span>''', '''<span role="cell" class="mono small">{{sm.check}}</span>''')
rep('''          <span>Generated by Vibranium 2.0.0 from <span class="mono">.vibranium/reports/482/ctrf.json</span> · exchanges stored with bodies up to 64 KB</span>''',
    '''          <span>Generated by Vibranium 2.0.0 from <span class="mono">.vibranium/reports/482/ctrf.json</span> · this page: <span class="mono">tests/orders.checkout.placesOrder.json</span> 19 KB, loaded when opened · exchange bodies stored because the test failed (up to 64 KB each)</span>''')

# JS
rep("this.state = { sel: 'p6', tab: 'response', onlyFailed: false, showLife: true, showPolls: true, showClean: true, headCollapsed: true, hl: 41, run: 482, sec: 'summary', copied: '' };",
    "this.state = { sel: 'p6', tab: 'response', onlyFailed: false, showLife: true, showPolls: true, showClean: true, headCollapsed: true, hl: 41, run: 482, sec: 'summary', copied: '', sideCollapsed: false, moreOpen: false, loaded: { p6: true, catalog: true }, loadingEx: '' };")
rep('''    const select = (k) => () => this.setState({ sel: k });''', '''    const select = (k) => () => {
      const st = this.state;
      if ((st.loaded || {})[k]) { this.setState({ sel: k, loadingEx: '' }); return; }
      this.setState({ sel: k, loadingEx: k });
      setTimeout(() => { if (this.state.loadingEx === k) this.setState({ loadingEx: '', loaded: Object.assign({}, this.state.loaded, { [k]: true }) }); }, 400);
    };''')
rep('''      copyLabel: s.copied === 'run' ? 'Copied' : 'Copy rerun command',''', '''      copyLabel: s.copied === 'run' ? 'Copied' : 'Copy rerun command',
      exLoading: !!s.loadingEx && s.loadingEx === s.sel && !ex.cached, exReady: !!ex.live && !(s.loadingEx && s.loadingEx === s.sel),
      exFile: 'tests/orders.checkout.placesOrder/' + s.sel + '.json',
      appCls: 'app' + (s.sideCollapsed ? ' side-collapsed' : ''), sideOpen: !s.sideCollapsed, sideShut: !!s.sideCollapsed,
      collapseLabel: s.sideCollapsed ? 'Expand sidebar' : 'Collapse sidebar', toggleSide: () => this.setState({ sideCollapsed: !this.state.sideCollapsed }),
      moreOpen: !!s.moreOpen, toggleMore: () => this.setState({ moreOpen: !this.state.moreOpen }),''')
open(P, 'w').write(s)
print('ok')
