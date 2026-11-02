import sys, json
sys.path.insert(0, '/tmp/claude-0/-home-claude/80b4900a-8223-5c77-874b-c1dda2eeca25/scratchpad/shell-tests')
from shellgen import topbar, side, SHELL_CSS
D = '/tmp/claude-0/-home-claude/80b4900a-8223-5c77-874b-c1dda2eeca25/scratchpad/settings-src/'
H = int(sys.argv[1]) if len(sys.argv) > 1 else 1400
tpl = open(D + 'template.html').read().replace('%%TOPBAR%%', topbar('setProfile').replace('SEARCHATTRS', '')).replace('%%SIDE%%', side('settings', {'tests': '412', 'agents': '3'}))
css = open(D + 'style.css').read().replace('%%SHELLCSS%%', SHELL_CSS)
js = open(D + 'logic.js').read()
props = json.dumps({"$preview": {"width": 1440, "height": H}}, separators=(',', ':'))
out = '''<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Vibranium · Settings</title>
<script src="./support.js"></script>
<link rel="stylesheet" href="/_blob/81dcd3b549d078b575adb6aed82e1861">
</head>
<body>
<x-dc>
<helmet>
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Geist:wght@400;500;600;700&amp;family=Geist+Mono:wght@400;500;600&amp;display=swap">
<style>
''' + css + '''</style>
</helmet>
''' + tpl + '''</x-dc>
<script type="text/x-dc" data-dc-script data-props=''' + "'" + props + "'" + '''>
''' + js + '''</script>
</body>
</html>
'''
assert "'" not in props
open('/tmp/claude-0/-home-claude/80b4900a-8223-5c77-874b-c1dda2eeca25/scratchpad/vibmock/project/WebSettings.dc.html', 'w').write(out)
print('built', len(out))
