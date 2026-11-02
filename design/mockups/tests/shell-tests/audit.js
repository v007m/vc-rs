() => {
  const W = innerWidth;
  const desc = (e) => e.tagName.toLowerCase() + (e.className && typeof e.className === 'string' ? '.' + e.className.trim().replace(/\s+/g, '.') : '') + ' "' + (e.textContent || '').trim().replace(/\s+/g, ' ').slice(0, 50) + '"';
  const out = { hscroll: document.documentElement.scrollWidth - W, h: document.documentElement.scrollHeight, off: [], clip: [], ell: [] };
  const seen = new Set();
  for (const e of document.querySelectorAll('body *')) {
    const r = e.getBoundingClientRect(); if (!r.width) continue;
    if (r.right > W + 1) {
      let p = e.parentElement, clipped = false;
      while (p && p !== document.body) { const cs = getComputedStyle(p); if (cs.overflowX !== 'visible') { const pr = p.getBoundingClientRect(); if (pr.right <= W + 1) { clipped = true; break; } } p = p.parentElement; }
      if (!clipped && !seen.has(e.parentElement)) { out.off.push(desc(e) + ' right=' + Math.round(r.right)); seen.add(e); }
    }
    const cs = getComputedStyle(e);
    if (e.children.length === 0 && (e.textContent || '').trim()) {
      let p = e.parentElement;
      while (p && p !== document.body) { const ps = getComputedStyle(p); if (ps.overflowX !== 'visible') break; p = p.parentElement; }
      if (p && p !== document.body) { const ps = getComputedStyle(p); if ((ps.overflowX === 'hidden' || ps.overflowX === 'clip') && ps.textOverflow !== 'ellipsis' && !p.classList.contains('sr')) { const pr = p.getBoundingClientRect(); if (r.right > pr.right + 1 && r.left < pr.right) out.clip.push('CUT ' + desc(e) + ' by ' + desc(p).slice(0, 40)); } }
    }
    if (e.tagName === 'svg' || e.closest('svg')) continue;
    if ((cs.overflowX === 'hidden' || cs.overflowX === 'clip') && e.scrollWidth > e.clientWidth + 1 && (e.textContent || '').trim()) {
      if (cs.textOverflow === 'ellipsis') out.ell.push(desc(e) + (e.title || e.closest('[title]') ? ' [title]' : ' [NO TITLE]'));
      else if (!e.classList.contains('scrollx') && !e.classList.contains('sr')) out.clip.push(desc(e) + ' sw' + e.scrollWidth + ' cw' + e.clientWidth);
    }
    if ((e.tagName === 'BUTTON' || (e.classList && e.classList.contains('btn'))) && e.scrollWidth > e.clientWidth + 1 && cs.textOverflow !== 'ellipsis') out.clip.push('BTN ' + desc(e) + ' sw' + e.scrollWidth + ' cw' + e.clientWidth);
    if ((cs.overflowY === 'hidden' || cs.overflowY === 'clip') && e.scrollHeight > e.clientHeight + 2 && (e.textContent || '').trim() && cs.display !== 'inline' && !e.classList.contains('sr') && e.clientHeight > 0 && e.clientHeight < 60) out.clip.push('VERT ' + desc(e) + ' sh' + e.scrollHeight + ' ch' + e.clientHeight);
  }
  return out;
}
