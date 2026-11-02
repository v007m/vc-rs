# Screenshot rendered states at several widths and report overflow / clipped text.
# usage: python3 shoot.py <filter-substring>... [--widths 1440,390] [--noshot]
import sys, os, glob, json
from playwright.sync_api import sync_playwright
d = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'shots')
args = [a for a in sys.argv[1:] if not a.startswith('--')]
widths = [1440, 1180, 960, 760, 390]
for a in sys.argv[1:]:
    if a.startswith('--widths='): widths = [int(x) for x in a.split('=')[1].split(',')]
noshot = '--noshot' in sys.argv
files = sorted(glob.glob(os.path.join(d, '*.html')))
JS = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'audit.js')).read()
with sync_playwright() as p:
    b = p.chromium.launch()
    for w in widths:
        pg = b.new_page(viewport={"width": w, "height": 900})
        for f in files:
            name = os.path.basename(f)[:-5]
            if args and not any(o in name for o in args): continue
            pg.goto('file://' + f)
            pg.evaluate("document.fonts.ready")
            pg.wait_for_timeout(120)
            r = pg.evaluate(JS)
            bad = r['hscroll'] > 0 or r['off'] or r['clip']
            print(('!! ' if bad else 'ok ') + name, w, 'h=%d' % r['h'], ('hscroll=%d' % r['hscroll']) if r['hscroll'] > 0 else '', 'ellipsis=%d' % len(r['ell']))
            for k in ('off', 'clip'):
                for x in r[k][:8]: print('     ', k, x)
            nt = [x for x in r['ell'] if 'NO TITLE' in x]
            if nt: print('      ell-no-title %d e.g.' % len(nt), '; '.join(nt[:3]))
            if not noshot:
                pg.screenshot(path=os.path.join(d, '%s-%d.png' % (name, w)), full_page=True)
        pg.close()
    b.close()
