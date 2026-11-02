import re, sys
sys.path.insert(0, '/tmp/claude-0/-home-claude/80b4900a-8223-5c77-874b-c1dda2eeca25/scratchpad/shell-tests')
from shellgen import topbar, side, SHELL_CSS
P = '/tmp/claude-0/-home-claude/80b4900a-8223-5c77-874b-c1dda2eeca25/scratchpad/vibmock/project/'
CFG = {
 'WebExplorer.dc.html': dict(cur='tests', counts={'tests': '412', 'agents': '3'}, prof='setProfile', search=' value="{{q}}" onInput="{{onQuery}}"'),
 'WebRun.dc.html': dict(cur='runs', counts={'tests': '412', 'runs': '{{navRunCount}}', 'agents': '3'}, prof='setProfile', search=''),
 'WebInsights.dc.html': dict(cur='insights', counts={'tests': '412', 'agents': '3'}, prof='setProfileSel', search=''),
}
for f, c in CFG.items():
    s = open(P + f).read()
    assert '/_blob/1e69a6d642232c67587a282225e98669' in s
    s = s.replace('/_blob/1e69a6d642232c67587a282225e98669', '/_blob/81dcd3b549d078b575adb6aed82e1861')
    a = s.index('<div class="app">'); b = s.index('</nav>', a) + len('</nav>')
    new = '<div class="app {{appCls}}">\n' + topbar(c['prof']).replace('SEARCHATTRS', c['search']) + '\n\n  <div class="frame">\n    <div class="side">\n' + side(c['cur'], c['counts'])
    s = s[:a] + new + s[b:]
    # css: drop rules now in the shell block or no longer used
    drop = [r'^\.navrow,\.ctx,\.ctxsec\{[^\n]*\n', r'^\.navitem \.sw\{[^\n]*\n', r'^\.topsel[^\n]*\n', r'^\.popwrap[^\n]*\n', r'^\.pophead[^\n]*\n', r'^\.popnote[^\n]*\n', r'^\.avatar[^\n]*\n',
            r'^  \.app \.side \{?[^\n]*\n', r'^  \.app \.side \.navrow,\.app \.side \.ctx\{[^\n]*\n', r'^  \.app \.side \.ctxsec\{[^\n]*\n', r'^  \.app \.side\{[^\n]*\n']
    for d in drop: s = re.sub(d, '', s, flags=re.M)
    s = s.replace('.sr{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap}\n', '.sr{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap}\n' + SHELL_CSS + '\n', 1)
    s = re.sub(r'@media \(max-width:860px\)\{\n\}\n', '', s)
    open(P + f, 'w').write(s)
    print(f, 'ok')
