"""Count borrow-checker-related constructs in Rust sources, per crate.

Comments, string literals and char literals are removed first so that only code is counted.
Usage: python3 -I code_metrics.py <crates_dir> [--json out.json]
"""
import json
import re
import sys
from pathlib import Path

LIFETIME = re.compile(r"'([A-Za-z_][A-Za-z0-9_]*)(?!')")


def strip(src: str) -> str:
    out, i, n = [], 0, len(src)
    while i < n:
        c = src[i]
        if src.startswith("//", i):
            j = src.find("\n", i)
            i = n if j < 0 else j
        elif src.startswith("/*", i):
            depth, i = 1, i + 2
            while i < n and depth:
                if src.startswith("/*", i):
                    depth, i = depth + 1, i + 2
                elif src.startswith("*/", i):
                    depth, i = depth - 1, i + 2
                else:
                    i += 1
        elif re.match(r'b?r#*"', src[i:i + 8]):
            m = re.match(r'b?r(#*)"', src[i:])
            end = '"' + m.group(1)
            j = src.find(end, i + m.end())
            out.append('""')
            i = n if j < 0 else j + len(end)
        elif c == '"' or src.startswith('b"', i):
            i += 2 if c == "b" else 1
            while i < n and src[i] != '"':
                i += 2 if src[i] == "\\" else 1
            out.append('""')
            i += 1
        elif c == "'" and re.match(r"'(\\.|\\u\{[0-9a-fA-F]+\}|[^\\'])'", src[i:i + 12]):
            m = re.match(r"'(\\.|\\u\{[0-9a-fA-F]+\}|[^\\'])'", src[i:i + 12])
            out.append("' '")
            i += m.end()
        else:
            out.append(c)
            i += 1
    return "".join(out)


def is_test_path(p: Path) -> bool:
    return "tests" in p.parts or p.name == "tests.rs"


def split_cfg_test(code: str) -> tuple[str, str]:
    """Split off `#[cfg(test)] mod tests { ... }` blocks (brace-matched)."""
    prod, test, i = [], [], 0
    for m in re.finditer(r"#\[cfg\(test\)\]\s*mod\s+\w+\s*\{", code):
        if m.start() < i:
            continue
        prod.append(code[i:m.start()])
        depth, j = 1, m.end()
        while j < len(code) and depth:
            depth += {"{": 1, "}": -1}.get(code[j], 0)
            j += 1
        test.append(code[m.start():j])
        i = j
    prod.append(code[i:])
    return "".join(prod), "".join(test)


def count(code: str) -> dict:
    lifetimes = LIFETIME.findall(code)
    pub_api_lifetimes = [
        line.strip() for line in code.splitlines()
        if re.match(r"\s*pub\s+(fn|struct|enum|trait|type|async fn|const fn)\b", line) and LIFETIME.search(line)
    ]
    return {
        "code_lines": sum(1 for l in code.splitlines() if l.strip()),
        "explicit_lifetimes": len(lifetimes),
        "lifetime_names": sorted(set(lifetimes)),
        "pub_api_lines_with_lifetimes": pub_api_lines if (pub_api_lines := pub_api_lifetimes) else [],
        "clone_calls": len(re.findall(r"\.clone\(\)", code)),
        "arc_clone_calls": len(re.findall(r"\b(Arc|Rc)::clone\(", code)),
        "arc_mutex_or_rwlock": len(re.findall(r"\bArc\s*<\s*(std::sync::|tokio::sync::)?(Mutex|RwLock)\b", code)),
        "mutex_or_rwlock_mentions": len(re.findall(r"\b(Mutex|RwLock)\s*<", code)),
        "refcell_mentions": len(re.findall(r"\bRefCell\s*<", code)),
        "rc_mentions": len(re.findall(r"\bRc\s*<", code)),
        "unsafe_blocks": len(re.findall(r"\bunsafe\s*\{", code)),
        "static_items": len(re.findall(r"^\s*(pub\s+)?static\s", code, re.M)),
    }


def merge(a: dict, b: dict) -> dict:
    out = {}
    for k in a:
        if isinstance(a[k], int):
            out[k] = a[k] + b[k]
        else:
            out[k] = sorted(set(a[k]) | set(b[k])) if k == "lifetime_names" else a[k] + b[k]
    return out


def main() -> None:
    root = Path(sys.argv[1])
    result = {}
    for crate in sorted(p for p in root.iterdir() if (p / "Cargo.toml").exists()):
        prod_total, test_total = None, None
        for f in sorted(crate.rglob("*.rs")):
            if "target" in f.parts:
                continue
            code = strip(f.read_text())
            prod, test = ("", code) if is_test_path(f.relative_to(crate)) else split_cfg_test(code)
            p, t = count(prod), count(test)
            prod_total = p if prod_total is None else merge(prod_total, p)
            test_total = t if test_total is None else merge(test_total, t)
        result[crate.name] = {"production": prod_total, "tests": test_total}
    if "--json" in sys.argv:
        Path(sys.argv[sys.argv.index("--json") + 1]).write_text(json.dumps(result, indent=2))
    keys = ["code_lines", "explicit_lifetimes", "clone_calls", "arc_clone_calls", "arc_mutex_or_rwlock",
            "mutex_or_rwlock_mentions", "refcell_mentions", "unsafe_blocks", "static_items"]
    print("crate".ljust(16) + "".join(k[:14].rjust(15) for k in keys))
    for name, r in result.items():
        for kind in ("production", "tests"):
            row = r[kind]
            if row["code_lines"] == 0:
                continue
            print(f"{name}/{kind[:4]}".ljust(16) + "".join(str(row[k]).rjust(15) for k in keys))
    for name, r in result.items():
        if r["production"]["lifetime_names"]:
            print(f"{name}: lifetimes {r['production']['lifetime_names']}; pub API lines: {r['production']['pub_api_lines_with_lifetimes']}")


main()
