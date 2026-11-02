# Layout audit run inside a rendered page (used by shoot.py; also runnable on its own):
#   python3 overflow.py <shots-dir> [name-filter ...]   -> prints issues at every audit width
# Reports: PAGE (horizontal page scroll), OFFSCREEN (sticks out of the viewport, not inside a scroller),
# CLIPPED (overflow hidden without ellipsis cuts content), ELLIPSIS-NO-TITLE (truncated with no title nearby),
# BTN-OVERFLOW (button text wider than the button), STRANDED (a narrow control alone on the left of a wide row).
import sys, os, glob

AUDIT_JS = r"""() => {
  const W = innerWidth, out = [];
  const de = document.documentElement;
  if (de.scrollWidth > W + 1) out.push('PAGE scrollWidth ' + de.scrollWidth + ' > ' + W);
  const desc = (e) => { let s = e.tagName.toLowerCase(); if (e.id) s += '#' + e.id; const c = typeof e.className === 'string' ? e.className : (e.className && e.className.baseVal) || ''; if (c.trim()) s += '.' + c.trim().split(/\s+/).join('.'); const t = (e.innerText || e.value || '').trim().replace(/\s+/g, ' ').slice(0, 48); return s + (t ? ' "' + t + '"' : ''); };
  const shown = (e) => { const r = e.getBoundingClientRect(); if (r.width < 3 || r.height < 3) return false; const cs = getComputedStyle(e); return cs.visibility !== 'hidden' && cs.display !== 'none'; };
  const titled = (e) => { for (let p = e, i = 0; p && i < 4; i++, p = p.parentElement) if (p.getAttribute && (p.getAttribute('title') || p.getAttribute('aria-label'))) return true; return false; };
  for (const e of document.querySelectorAll('body *')) {
    if (e.closest('svg')) continue;
    if (!shown(e)) continue;
    const cs = getComputedStyle(e), r = e.getBoundingClientRect();
    if (r.right > W + 1 && !e.closest('.scrim')) {
      let p = e.parentElement, contained = false;
      while (p && p !== document.body) { const pc = getComputedStyle(p); if (pc.overflowX !== 'visible') { const pr = p.getBoundingClientRect(); if (pr.right <= W + 1) { contained = true; break; } } p = p.parentElement; }
      if (!contained) out.push('OFFSCREEN ' + desc(e) + ' right=' + Math.round(r.right));
    }
    const clips = cs.overflowX === 'hidden' || cs.overflowX === 'clip';
    if (e.tagName === 'INPUT' && !/checkbox|radio|range/.test(e.type)) { if (e.scrollWidth > e.clientWidth + 1 && e.clientWidth < 90) out.push('NARROW-INPUT ' + desc(e) + ' ' + e.scrollWidth + '>' + e.clientWidth); }
    else if (clips && e.scrollWidth > e.clientWidth + 1 && e.tagName !== 'TEXTAREA' && e.tagName !== 'SELECT') {
      if (cs.textOverflow === 'ellipsis') { if (!titled(e)) out.push('ELLIPSIS-NO-TITLE ' + desc(e)); }
      else out.push('CLIPPED ' + desc(e) + ' ' + e.scrollWidth + '>' + e.clientWidth);
    }
    if ((e.tagName === 'BUTTON' || (e.tagName === 'A' && e.classList.contains('btn'))) && !clips && e.scrollWidth > e.clientWidth + 1) out.push('BTN-OVERFLOW ' + desc(e) + ' ' + e.scrollWidth + '>' + e.clientWidth);
    if ((e.tagName === 'SELECT' || (e.tagName === 'INPUT' && !/checkbox|radio|range/.test(e.type))) && e.closest('.panel,.modal')) {
      let item = e.parentElement && e.parentElement.tagName === 'LABEL' ? e.parentElement : e;
      const row = item.parentElement; if (!row) continue;
      const rr = row.getBoundingClientRect();
      if (rr.width < 320) continue;
      const kids = [...row.children].filter(shown);
      const L = Math.min(...kids.map(k => k.getBoundingClientRect().left)), R = Math.max(...kids.map(k => k.getBoundingClientRect().right));
      if (R - L < rr.width * 0.5 && r.width < rr.width * 0.4) out.push('STRANDED ' + desc(item) + ' uses ' + Math.round(R - L) + 'px of ' + Math.round(rr.width));
    }
  }
  return [...new Set(out)];
}"""

WIDTHS = [1440, 1180, 960, 760, 390]

def audit(pg):
    return pg.evaluate(AUDIT_JS)

if __name__ == '__main__':
    from playwright.sync_api import sync_playwright
    d = os.path.abspath(sys.argv[1])
    only = sys.argv[2:]
    files = sorted(glob.glob(os.path.join(d, '*.html')))
    with sync_playwright() as p:
        b = p.chromium.launch()
        for w in WIDTHS:
            pg = b.new_page(viewport={"width": w, "height": 844 if w < 500 else 900})
            for f in files:
                name = os.path.basename(f)[:-5]
                if only and not any(o in name for o in only): continue
                pg.goto('file://' + f); pg.evaluate('document.fonts.ready')
                for line in audit(pg): print(w, name, line)
            pg.close()
        b.close()
