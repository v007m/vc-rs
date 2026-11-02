# clip.py file.html width selector out.png [pad_before] [height]
import sys, os
from playwright.sync_api import sync_playwright
f, w, sel, out = os.path.abspath(sys.argv[1]), int(sys.argv[2]), sys.argv[3], sys.argv[4]
pb = int(sys.argv[5]) if len(sys.argv) > 5 else 200; h = int(sys.argv[6]) if len(sys.argv) > 6 else 700
with sync_playwright() as p:
    b = p.chromium.launch(); pg = b.new_page(viewport={"width": w, "height": 900}); pg.goto('file://' + f); pg.wait_for_timeout(400)
    y = pg.evaluate("(s)=>{const e=document.querySelector(s); return e? e.getBoundingClientRect().top+scrollY : -1}", sel)
    print('y', y)
    pg.screenshot(path=out, full_page=True, clip={"x": 0, "y": max(0, y - pb), "width": w, "height": h})
    b.close()
