# Generates the v2 shell markup (topbar + side head + global nav) for an artboard.
ICON = {
 'tests': '<path d="m2.5 4.5 1.5 1.5 2.5-3M2.5 11l1.5 1.5 2.5-3M9 5h4.5M9 11.5h4.5"/>',
 'client': '<path d="M2 8h9M8 4.5 11.5 8 8 11.5M14 3v10"/>',
 'editor': '<path d="M10.5 2.5l3 3L6 13H3v-3zM9 4l3 3"/>',
 'runs': '<circle cx="8" cy="8" r="6"/><path d="M6.5 5.5v5l4-2.5z"/>',
 'insights': '<path d="M2.5 13.5h11M4.5 11V8M8 11V4M11.5 11V6.5"/>',
 'agents': '<rect x="3" y="5" width="10" height="8" rx="2"/><path d="M8 2.5V5M6 8.5v1.5M10 8.5v1.5"/>',
 'settings': '<path d="M2.5 4.5h6M11.5 4.5h2M2.5 11.5h2M7.5 11.5h6"/><circle cx="10" cy="4.5" r="1.5"/><circle cx="6" cy="11.5" r="1.5"/>',
}
NAV = [('tests', 'Tests', 'WebExplorer.dc.html'), ('client', 'API client', 'WebClient.dc.html'), ('editor', 'Editor', 'WebEditor.dc.html'),
       ('runs', 'Runs', 'WebRun.dc.html'), ('insights', 'Insights', 'WebInsights.dc.html'), ('agents', 'AI agents', 'WebAgents.dc.html')]
SVG = '<svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">%s</svg>'
def item(key, label, href, on, count):
    cls = 'navitem on' if on else 'navitem'
    cur = ' aria-current="page"' if on else ''
    c = '<span class="count">%s</span>' % count if count else ''
    return '        <a class="%s" href="%s" title="%s"%s>%s<span class="label">%s</span>%s</a>' % (cls, href, label, cur, SVG % ICON[key], label, c)
def topbar(profile_handler='setProfile'):
    return '''  <header class="topbar">
    <a class="brand" href="WebExplorer.dc.html"><svg width="22" height="22" viewBox="0 0 28 28" aria-hidden="true"><path d="M14 2 25 8.5v11L14 26 3 19.5v-11Z" fill="none" stroke="#5440D6" stroke-width="2"/><path d="M9 10.5 14 18l5-7.5" fill="none" stroke="#1B1830" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>Vibranium</a>
    <span class="project" title="Project shop in ~/code/shop, git branch main"><b>shop</b><span class="chip"><svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" aria-hidden="true"><circle cx="5" cy="3.5" r="1.5"/><circle cx="5" cy="12.5" r="1.5"/><circle cx="11" cy="6" r="1.5"/><path d="M5 5v6M11 7.5c0 2.5-3 2.5-5.5 4"/></svg>main</span></span>
    <label class="topsel"><span>Profile</span>
      <select value="{{profile}}" onChange="{{%s}}"><option value="dev">dev</option><option value="staging">staging</option><option value="prod">prod</option></select>
    </label>
    <div class="grow"></div>
    <label class="search"><svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" aria-hidden="true"><circle cx="7" cy="7" r="4.75"/><path d="m10.5 10.5 3.5 3.5"/></svg><span class="sr">Search tests, apis, files</span><input type="search" placeholder="Search tests, apis, files…"SEARCHATTRS><span class="kbd">⌘K</span></label>
  </header>''' % profile_handler
def side(current, counts):
    lines = ['      <div class="side-head"><button class="iconbtn" type="button" aria-label="{{sideLabel}}" title="{{sideLabel}}" aria-expanded="{{sideOpen}}" onClick="{{toggleSide}}"><svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="2" y="2.5" width="12" height="11" rx="2"/><path d="M6 2.5v11"/><path d="{{sideChev}}"/></svg></button></div>',
             '      <nav class="navrow" aria-label="Main">']
    for k, l, h in NAV:
        lines.append(item(k, l, h, k == current, counts.get(k)))
    lines.append('        <span class="navsep" aria-hidden="true"></span>')
    lines.append(item('settings', 'Settings', 'WebSettings.dc.html', current == 'settings', counts.get('settings')))
    lines.append('      </nav>')
    return '\n'.join(lines)
SHELL_CSS = '''/* v2 shell */
.side-head{justify-content:flex-end}
.navrow,.ctx,.ctxsec{display:flex;flex-direction:column;gap:2px}
.navsep{display:block;flex:none;height:1px;background:var(--line2);margin:6px 10px}
.app.side-collapsed .side .navsep{margin:6px 8px}
.navitem .sw{width:16px;display:inline-flex;justify-content:center;flex:none}
.ctxsec .navitem .txt{min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.topsel{display:flex;align-items:center;gap:6px;font-size:12.5px;color:var(--ink2)}
.topsel select{height:30px;border:0;border-radius:8px;background:var(--hover);padding:0 8px;font:500 12.5px var(--font);color:var(--ink)}
@media (max-width:860px){
  .app .side{flex-direction:column;overflow:visible}
  .app .side .navrow,.app .side .ctx{flex-direction:row;overflow-x:auto;gap:4px}
  .app .side .ctxsec{flex-direction:row;flex:none;gap:4px}
  .navsep,.app.side-collapsed .side .navsep{width:1px;height:20px;margin:auto 4px}
  .app.side-collapsed .side .navitem{width:auto;height:32px;padding:0 10px;margin:0;justify-content:flex-start}
  .app.side-collapsed .side .navitem .count{display:inline}
  .app.side-collapsed .side .ctx{display:flex}
}'''
