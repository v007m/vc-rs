# Usage: python3 shoot.py <shotsdir> [--widths 1440,1180,960,760,390] [--noshot] [name-filter ...]
# Screenshots every rendered state at each width (full page) and reports page overflow,
# elements running off-screen outside a scroll box, clipped text (overflow hidden without ellipsis)
# and truncated ellipsis cells.
import sys, os, glob, json
from playwright.sync_api import sync_playwright
args = sys.argv[1:]
d = os.path.abspath(args.pop(0))
widths = [1440, 1180, 960, 760, 390]
noshot = False
only = []
while args:
    a = args.pop(0)
    if a == '--widths': widths = [int(x) for x in args.pop(0).split(',')]
    elif a == '--noshot': noshot = True
    else: only.append(a)
files = sorted(glob.glob(os.path.join(d, '*.html')))
JS = r"""(W) => {
  const out = {sw: document.documentElement.scrollWidth, sh: document.documentElement.scrollHeight, off: [], clipped: [], ell: 0, btn: []};
  const lab = (e) => (e.tagName.toLowerCase() + (e.className && typeof e.className === 'string' ? '.' + e.className.trim().split(/\s+/).join('.') : '') + ' "' + (e.textContent || '').trim().replace(/\s+/g, ' ').slice(0, 40) + '"');
  document.querySelectorAll('body *').forEach((e) => {
    const r = e.getBoundingClientRect();
    if (r.width === 0 || r.height === 0) return;
    const cs = getComputedStyle(e);
    if (r.right > W + 1) {
      let p = e.parentElement, inScroll = false;
      while (p && p !== document.body) { const pc = getComputedStyle(p); if (pc.overflowX !== 'visible') { const pr = p.getBoundingClientRect(); if (pr.right <= W + 1) { inScroll = true; break; } } p = p.parentElement; }
      if (!inScroll) out.off.push(lab(e) + ' right=' + Math.round(r.right));
    }
    const ox = cs.overflowX;
    if ((ox === 'hidden' || ox === 'clip') && e.clientWidth > 2 && e.scrollWidth > e.clientWidth + 1 && e.children.length < 6 && e.tagName !== 'svg' && !(e.closest && e.closest('svg'))) {
      if (cs.textOverflow === 'ellipsis') out.ell++;
      else out.clipped.push(lab(e) + ' ' + e.scrollWidth + '>' + e.clientWidth);
    }
    if ((e.classList.contains('btn') || e.tagName === 'BUTTON') && e.scrollWidth > e.clientWidth + 1) out.btn.push(lab(e) + ' ' + e.scrollWidth + '>' + e.clientWidth);
  });
  out.off = out.off.slice(0, 12); out.clipped = out.clipped.slice(0, 12); out.btn = out.btn.slice(0, 12);
  return out;
}"""
with sync_playwright() as p:
    b = p.chromium.launch()
    for w in widths:
        pg = b.new_page(viewport={"width": w, "height": 900})
        for f in files:
            name = os.path.basename(f)[:-5]
            if only and not any(o in name for o in only): continue
            pg.goto('file://' + f)
            try: pg.wait_for_load_state('networkidle', timeout=3000)
            except Exception: pass
            pg.wait_for_timeout(120)
            info = pg.evaluate(JS, w)
            bad = info['sw'] > w or info['off'] or info['clipped'] or info['btn']
            print(('BAD ' if bad else 'ok  ') + name, w, json.dumps({k: v for k, v in info.items() if v}))
            if not noshot:
                pg.screenshot(path=os.path.join(d, '%s-%d.png' % (name, w)), full_page=True)
        pg.close()
    b.close()
