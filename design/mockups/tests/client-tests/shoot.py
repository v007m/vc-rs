# Screenshots every rendered state at the audit widths and runs the layout audit on each.
#   python3 shoot.py [shots-dir] [--widths 1440,390] [name-filter ...]
# shots-dir defaults to ./shots next to this file. PNGs are written as <state>-<width>.png;
# states saved with a "-c" suffix are the same state with the sidebar collapsed.
import sys, os, glob
from playwright.sync_api import sync_playwright
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from overflow import audit, WIDTHS

args = sys.argv[1:]
d = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'shots')
if args and os.path.isdir(args[0]): d = os.path.abspath(args.pop(0))
widths = WIDTHS
if args and args[0] == '--widths': widths = [int(x) for x in args[1].split(',')]; args = args[2:]
only = args
files = sorted(glob.glob(os.path.join(d, '*.html')))
issues = 0
grouped = {}
with sync_playwright() as p:
    b = p.chromium.launch()
    for w in widths:
        pg = b.new_page(viewport={"width": w, "height": 844 if w < 500 else 900})
        for f in files:
            name = os.path.basename(f)[:-5]
            if only and not any(o in name for o in only): continue
            pg.goto('file://' + f)
            pg.evaluate('document.fonts.ready')
            pg.wait_for_timeout(60)
            found = audit(pg)
            issues += len(found)
            for line in found: grouped.setdefault((w, line), []).append(name)
            pg.screenshot(path=os.path.join(d, '%s-%d.png' % (name, w)), full_page=True)
        pg.close()
    b.close()
for (w, line), names in sorted(grouped.items()):
    print(w, line, ' <- ' + (', '.join(names) if len(names) < 5 else names[0] + ' +%d more' % (len(names) - 1)))
print('audit issues:', issues)
