#!/usr/bin/env python3
"""Weighted scoring + sensitivity analysis. Writes results into raw.json["scoring"]."""
import itertools, json, os
ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
W = {"quality": 30, "libraries": 25, "extensibility": 20, "compile": 10, "cel": 10, "load": 5}
S = {
    "typescript": {"quality": 4, "libraries": 4, "extensibility": 5, "compile": 4, "cel": 5, "load": 4},
    "go":         {"quality": 3, "libraries": 5, "extensibility": 3, "compile": 5, "cel": 3, "load": 3},
    "rust":       {"quality": 5, "libraries": 4, "extensibility": 3, "compile": 5, "cel": 2, "load": 5},
}
def total(s, w): return sum(s[k] / 5 * w[k] for k in w)
totals = {l: total(s, W) for l, s in S.items()}
# Sensitivity 1: move 10 points between any two criteria.
flips = []
for a, b in itertools.permutations(W, 2):
    w = dict(W); d = min(10, w[b]); w[a] += d; w[b] -= d
    t = {l: total(s, w) for l, s in S.items()}
    win = max(t, key=t.get)
    if win != "typescript": flips.append({"shift": f"+{d} {a}, -{d} {b}", "winner": win, "totals": {k: round(v, 1) for k, v in t.items()}})
# Sensitivity 2: change any one score by +/-1.
score_flips = []
for lang, crit, delta in itertools.product(S, W, (-1, 1)):
    s2 = json.loads(json.dumps(S)); s2[lang][crit] = max(1, min(5, s2[lang][crit] + delta))
    t = {l: total(s, W) for l, s in s2.items()}
    win = max(t, key=t.get)
    if win != "typescript": score_flips.append({"change": f"{lang}.{crit} {delta:+d}", "winner": win, "totals": {k: round(v, 1) for k, v in t.items()}})
out = {"weights": W, "scores": S, "totals": totals, "weight_shift_flips": flips, "single_score_flips": score_flips,
       "weight_shifts_tested": len(list(itertools.permutations(W, 2))), "score_changes_tested": len(S) * len(W) * 2}
raw = json.load(open(f"{ROOT}/results/raw.json")); raw["scoring"] = out
json.dump(raw, open(f"{ROOT}/results/raw.json", "w"), indent=2, sort_keys=True)
print(json.dumps({"totals": totals, "weight_shift_flips": flips, "single_score_flips": score_flips}, indent=1))
