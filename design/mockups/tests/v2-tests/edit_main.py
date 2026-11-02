import re, sys
P = '/tmp/claude-0/-home-claude/80b4900a-8223-5c77-874b-c1dda2eeca25/scratchpad/vibmock/project/Main.dc.html'
s = open(P).read()
def rep(a, b, count=1):
    global s
    if a not in s:
        print('MISSING:', a[:120]); sys.exit(1)
    s = s.replace(a, b, count)

rep('/_blob/1e69a6d642232c67587a282225e98669', '/_blob/81dcd3b549d078b575adb6aed82e1861')

# ---- CSS
style_start = s.index('<style>'); style_end = s.index('</style>')
css = s[style_start:style_end]
css = re.sub(r'\.chip\b', '.fchip', css)
css = css.replace('.sfield{display:flex;align-items:center;gap:8px;height:34px;border:1px solid var(--line2);border-radius:9px;padding:0 10px;flex:0 1 330px;min-width:220px;color:var(--ink2)}',
                  '.sfield{display:flex;align-items:center;gap:8px;height:34px;border:1px solid var(--line2);border-radius:9px;padding:0 10px;flex:1 1 240px;min-width:0;color:var(--ink2)}')
css = css.replace('.xgrid{display:grid;grid-template-columns:minmax(0,1.3fr) minmax(0,1.2fr) minmax(0,.9fr);gap:24px}',
                  '.xgrid{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(240px,100%),1fr));gap:18px 24px}')
css = css.replace('@media (max-width:980px){.cause{grid-template-columns:minmax(0,1fr);gap:14px}.xgrid{grid-template-columns:minmax(0,1fr)}.xp{padding-left:16px}}\n', '')
css = css.replace('@media (max-width:860px){.sidefilter{display:none}.verdict{font-size:22px}.stat{padding-left:10px}}\n', '')
css += '''.topbar .tchip{max-width:100%}
.runid{gap:6px}
.acts{gap:6px}
.narrow-only{display:none}
.morewrap{position:relative}
.morewrap .popover{position:absolute;right:0;top:calc(100% + 6px);z-index:30;min-width:220px}
.morewrap .popover a.opt{text-decoration:none}
.side-head .navlabel{padding:0 8px}
.ctx .navitem,.ctx .navcheck{font-size:13px}
.t-tests .c-du::before,.t-tests .c-re::before,.t-tests .c-in::before{content:attr(data-l);color:var(--ink2);font-size:12px;margin-right:4px}
.t-tests .c-du::before,.t-tests .c-re::before,.t-tests .c-in::before{display:none}
.loadline{display:flex;align-items:center;gap:10px;font-size:13px;color:var(--ink2);padding:6px 0}
.spin{width:14px;height:14px;border-radius:50%;border:2px solid var(--line2);border-top-color:var(--accent);animation:spin .8s linear infinite;flex:none}
@keyframes spin{to{transform:rotate(360deg)}}
.skel{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(240px,100%),1fr));gap:18px 24px}
.skel span{display:block;height:10px;border-radius:5px;background:var(--panel3);margin:6px 0}
.morebar{display:flex;align-items:center;gap:10px;flex-wrap:wrap}
.about{font-size:12.5px;color:var(--ink2);line-height:1.55;padding:10px 12px;background:var(--panel2);border-radius:9px}
@container (max-width: 900px){
  .cause{grid-template-columns:minmax(0,1fr);gap:14px}
}
@container (max-width: 820px){
  .t-tests{min-width:0}
  .t-tests .hd{display:none}
  .t-tests .tr{grid-template-columns:30px auto auto minmax(0,1fr);grid-template-areas:"x st nm nm" "x fl fl fl" "x du re in";row-gap:4px;column-gap:10px}
  .t-tests .c-x{grid-area:x;align-self:start}
  .t-tests .c-st{grid-area:st;align-self:start}
  .t-tests .c-nm{grid-area:nm}
  .t-tests .c-fl{grid-area:fl}
  .t-tests .c-du{grid-area:du;text-align:left}
  .t-tests .c-re{grid-area:re;text-align:left}
  .t-tests .c-in{grid-area:in}
  .t-tests .c-du::before,.t-tests .c-re::before,.t-tests .c-in::before{display:inline}
  .t-tests .c-du,.t-tests .c-re,.t-tests .c-in{font-size:12px}
  .xp{padding-left:16px}
  .verdict{font-size:22px}
}
@media (max-width: 860px){
  .ctx{display:none}
}
@media (max-width: 760px){
  .wide-only{display:none}
  .narrow-only{display:inline-flex}
}
'''
s = s[:style_start] + css + s[style_end:]

# ---- topbar
rep('''    <div class="row" style="gap: 6px">
      <span class="pill fail"><span class="dot fail"></span>Failed</span>''', '''    <div class="cluster runid">
      <span class="pill fail"><span class="dot fail"></span>Failed</span>''')
rep('''    <div class="row" style="gap: 6px">
      <a class="btn" href="WebRun.dc.html">Open in web UI</a>
      <a class="btn" href="ctrf.json" download="run-482.ctrf.json">Download CTRF</a>
      <a class="btn" href="junit.xml" download="run-482.junit.xml">JUnit XML</a>''', '''    <div class="cluster acts">
      <a class="btn" href="WebRun.dc.html">Open in web UI</a>
      <a class="btn wide-only" href="ctrf.json" download="run-482.ctrf.json">Download CTRF</a>
      <a class="btn wide-only" href="junit.xml" download="run-482.junit.xml">JUnit XML</a>
      <div class="morewrap narrow-only">
        <button class="btn" type="button" aria-expanded="{{moreOpen}}" aria-haspopup="true" onClick="{{toggleMore}}">More<svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" aria-hidden="true"><path d="m3 4.5 3 3 3-3"/></svg></button>
        <sc-if value="{{moreOpen}}" hint-placeholder-val="{{ false }}">
          <div class="popover">
            <a class="opt" href="ctrf.json" download="run-482.ctrf.json">Download CTRF</a>
            <a class="opt" href="junit.xml" download="run-482.junit.xml">JUnit XML</a>
          </div>
        </sc-if>
      </div>''')
rep('<div class="app">', '<div class="{{appCls}}">')

# ---- side
rep('''    <nav class="side" aria-label="Report sections">
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
      <nav class="navrow" aria-label="Report sections" style="display: flex; flex-direction: column; gap: 2px">
      <sc-for list="{{nav}}" as="n" hint-placeholder-count="10">
        <a class="navitem {{n.cls}}" href="{{n.href}}" title="{{n.label}}" onClick="{{n.go}}"><svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true"><path d="{{n.icon}}" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"></path></svg><span class="label">{{n.label}}</span><span class="count">{{n.count}}</span></a>
      </sc-for>
      </nav>
      <div class="ctx">''')
rep('''<span class="dot {{f.dot}}" style="margin: 0 4px"></span>{{f.label}}<span class="count">''', '''<span class="dot {{f.dot}}" style="margin: 0 4px"></span><span class="label">{{f.label}}</span><span class="count">''')
rep('''        <label class="navcheck"><input type="checkbox" checked="{{groupByFile}}" onChange="{{toggleGroup}}">Group by file</label>
      </div>
    </nav>''', '''        <label class="navcheck"><input type="checkbox" checked="{{groupByFile}}" onChange="{{toggleGroup}}">Group by file</label>
      </div>
    </div>''')

# ---- tests section
rep('''<button type="button" class="chip {{c.cls}}"''', '''<button type="button" class="fchip {{c.cls}}"''')
rep('''<div class="sh" style="align-items: center"><h2 id="h-tests" class="h2">All tests</h2><span class="small muted">{{shownText}}</span>''', '''<div class="sh" style="align-items: center"><h2 id="h-tests" class="h2">All tests</h2><span class="small muted">{{shownText}} · checks and exchanges load when you open a test</span>''')
rep('''<span role="cell"><button type="button" class="xbtn''', '''<span role="cell" class="c-x"><button type="button" class="xbtn''')
rep('''<span role="cell"><span class="pill {{t.cls}}"><span class="dot {{t.cls}}"></span>{{t.label}}</span></span>
                    <div class="cellname" role="cell">''', '''<span role="cell" class="c-st"><span class="pill {{t.cls}}"><span class="dot {{t.cls}}"></span>{{t.label}}</span></span>
                    <div class="cellname c-nm" role="cell">''')
rep('''<span role="cell" class="mono small muted" style="overflow-wrap: anywhere">{{t.file}}</span>
                    <span role="cell" class="num">{{t.dur}}</span>
                    <span role="cell" class="num">{{t.retries}}</span>
                    <span role="cell" class="small">{{t.inst}}</span>''', '''<span role="cell" class="mono small muted c-fl" style="overflow-wrap: anywhere">{{t.file}}</span>
                    <span role="cell" class="num c-du" data-l="Duration">{{t.dur}}</span>
                    <span role="cell" class="num c-re" data-l="Retries">{{t.retries}}</span>
                    <span role="cell" class="small c-in" data-l="Instances">{{t.inst}}</span>''')
rep('''                      <div class="xgrid" role="cell">''', '''                      <sc-if value="{{t.loading}}" hint-placeholder-val="{{ false }}">
                        <div role="cell" style="display: flex; flex-direction: column; gap: 6px">
                          <div class="loadline" role="status"><span class="spin" aria-hidden="true"></span>Loading checks and exchanges… <span class="mono small">{{t.page}}</span></div>
                          <div class="skel" aria-hidden="true"><div><span style="width: 60%"></span><span style="width: 85%"></span><span style="width: 70%"></span></div><div><span style="width: 75%"></span><span style="width: 55%"></span></div><div><span style="width: 50%"></span><span style="width: 65%"></span></div></div>
                        </div>
                      </sc-if>
                      <sc-if value="{{t.ready}}" hint-placeholder-val="{{ true }}">
                      <div class="xgrid" role="cell">''')
rep('''                          <div class="small mono">{{t.tags}}</div>
                        </div>
                      </div>''', '''                          <div class="small mono">{{t.tags}}</div>
                        </div>
                      </div>
                      </sc-if>''')
rep('''          <div class="small muted">{{moreText}}</div>''', '''          <div class="morebar">
            <span class="small muted">{{pageText}}</span>
            <sc-if value="{{hasMore}}" hint-placeholder-val="{{ true }}"><button type="button" class="btn sm" onClick="{{showMore}}">Show {{nextCount}} more</button><button type="button" class="btn sm ghost" onClick="{{showAll}}">Show all {{shownCount}}</button></sc-if>
          </div>''')

# ---- reproduce: about this report
rep('''                <dt>Artifacts</dt><dd class="mono">.vibranium/reports/482/ctrf.json · junit.xml</dd>
              </dl>''', '''                <dt>Artifacts</dt><dd class="mono">.vibranium/reports/482/ctrf.json · junit.xml</dd>
              </dl>
              <div class="about"><b style="color: #1B1830; font-weight: 600">This report:</b> index 180 KB · 412 test pages · bodies stored for failures only. The overview loads only the summary and the test list; a test’s checks, exchanges and bodies load when you open its row or its page, so big runs stay fast.</div>''')

open(P, 'w').write(s)
print('template ok')
