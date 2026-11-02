import os, re, sys
d = os.path.dirname(os.path.abspath(__file__))
lib = open(os.path.join(d, 'lib.js')).read()
comp = open(os.path.join(d, 'component.js')).read()
tpl = open(os.path.join(d, 'template.html')).read()
assert '`' not in comp, 'template literal in component'
lib_ind = '\n'.join(('      ' + l if l.strip() else '') for l in lib.rstrip('\n').split('\n'))
comp = comp.replace('/*__LIB__*/', lib_ind)
out = tpl.replace('/*__CLASS__*/', comp.rstrip('\n'))
assert '/*__' not in out
script = out[out.index('<script type="text/x-dc"'):]
assert '</script' not in script[script.index('>')+1:script.rindex('</script>')], 'nested script end'
assert '{{' not in script, 'double brace in script'
dst = os.path.join(d, '..', 'vibmock', 'project', 'WebClient.dc.html')
open(dst, 'w').write(out)
print('wrote', os.path.normpath(dst), len(out), 'bytes')
