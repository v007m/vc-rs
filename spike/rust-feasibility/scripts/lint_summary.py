"""Summarize cargo --message-format json diagnostics: one row per (file:line, lint)."""
import json
import sys

filt = sys.argv[1] if len(sys.argv) > 1 else ""
seen = set()
rows = []
for line in sys.stdin:
    try:
        m = json.loads(line)
    except ValueError:
        continue
    if m.get("reason") != "compiler-message":
        continue
    msg = m["message"]
    spans = [s for s in msg.get("spans", []) if s["is_primary"]]
    if not spans or filt not in spans[0]["file_name"]:
        continue
    sp = spans[0]
    code = (msg.get("code") or {}).get("code", "")
    key = (sp["file_name"], sp["line_start"], code)
    if key in seen:
        continue
    seen.add(key)
    rows.append((sp["file_name"], sp["line_start"], code, msg["message"][:90]))
for f, l, c, t in sorted(rows):
    print(f"{f}:{l}\t{c}\t{t}")
