import sys, os
from playwright.sync_api import sync_playwright
f, w = os.path.abspath(sys.argv[1]), int(sys.argv[2])
sel = sys.argv[3] if len(sys.argv) > 3 else 'main.panel'
with sync_playwright() as p:
    b = p.chromium.launch(); pg = b.new_page(viewport={"width": w, "height": 900}); pg.goto('file://' + f); pg.wait_for_timeout(300)
    print(pg.evaluate("""(sel) => { const P=document.querySelector(sel).getBoundingClientRect(); const out=[]; document.querySelectorAll(sel+' *').forEach(e=>{const r=e.getBoundingClientRect(); if(r.right>P.right+1 && r.width>0){ let p=e.parentElement, sc=false; while(p && !p.matches(sel)){ const o=getComputedStyle(p).overflowX; if(o!=='visible'){sc=true;break;} p=p.parentElement;} if(!sc) out.push(e.tagName+'.'+e.className+' "'+e.textContent.trim().slice(0,50)+'" '+Math.round(r.right)); }}); return out.slice(0,20); }""", sel))
    b.close()
