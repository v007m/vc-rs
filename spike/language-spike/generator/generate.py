#!/usr/bin/env python3
"""Fixture generator for the Vibranium v2 language spike.

Writes three projects under fixtures/:
  ok/      1,000 YAML files: 200 *.api.yaml (5 apis each), 100 *.given.yaml, 700 *.test.yaml
  broken/  copy of ok/ with: 1 unknown reference (typo), 1 duplicate id, 1 given cycle,
           1 schema violation (invalid enum value)
  proof/   small suite that proves given memoization (singleflight) and `repeat`

Deterministic (seeded) so all three implementations compile identical inputs.
"""
import os
import random
import shutil
import sys

import yaml

ROOT = os.path.abspath(sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(__file__), "..", "fixtures"))
RNG = random.Random(20261022)

NOUNS = ["users", "orders", "invoices", "products", "carts", "payments", "shipments", "customers",
         "accounts", "tickets", "reviews", "coupons", "refunds", "vendors", "stores", "regions",
         "teams", "projects", "tasks", "comments"]
N_GROUPS = 200
N_GIVEN_FILES = 100
N_TEST_FILES = 700

CEL_ENTITY = [
    "size(body.items) > 0 && body.items.all(i, i.price > 0.0)",
    "body.total > 10.0 && body.active",
    "body.items.exists(i, i.sku == 'a')",
    "body.id != '' && size(body.items) == 2",
    "body.items.map(i, i.price).exists(p, p > 5.0)",
    "body.name.startsWith(body.name) && body.total < 1000.0",
]
CEL_LIST = [
    "size(body.items) > 0 && body.items.all(i, i.active)",
    "body.items.all(i, size(i.items) > 0 && i.total > 0.0)",
]

SCHEMA_ITEM = {
    "type": "object",
    "required": ["id", "items"],
    "properties": {
        "id": {"type": "string"},
        "name": {"type": "string"},
        "items": {"type": "array", "minItems": 1,
                  "items": {"type": "object", "required": ["sku", "price"],
                            "properties": {"sku": {"type": "string"}, "price": {"type": "number", "exclusiveMinimum": 0}}}},
        "total": {"type": "number"},
        "active": {"type": "boolean"},
    },
}
SCHEMA_LIST = {"type": "object", "required": ["items", "count"],
               "properties": {"items": {"type": "array"}, "count": {"type": "integer", "minimum": 0}}}


class NoAliasDumper(yaml.SafeDumper):
    def ignore_aliases(self, data):
        return True


def dump(path, data):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f:
        yaml.dump(data, f, Dumper=NoAliasDumper, sort_keys=False, default_flow_style=False, width=120)


def group_name(g):
    return f"{NOUNS[g % len(NOUNS)]}{g // len(NOUNS)}"


def group_dir(base, g):
    return os.path.join(base, f"domain{g % 20:02d}", group_name(g))


def gen_apis(base):
    for g in range(N_GROUPS):
        r = group_name(g)
        guards = {"status": "2xx"}
        apis = [
            {"id": f"{r}.create", "method": "POST", "path": f"/{r}", "inputs": ["name"],
             "body": {"name": "{{ name }}", "kind": r},
             "outputs": {"id": "$.id", "firstPrice": "$.items[0].price"}, "guards": guards},
            {"id": f"{r}.get", "method": "GET", "path": f"/{r}/{{{{ id }}}}", "inputs": ["id"],
             "outputs": {"name": "$.name", "total": "$.total"}, "guards": guards},
            {"id": f"{r}.list", "method": "GET", "path": f"/{r}", "outputs": {"count": "$.count"}, "guards": guards},
            {"id": f"{r}.update", "method": "PUT", "path": f"/{r}/{{{{ id }}}}", "inputs": ["id", "name"],
             "body": {"name": "{{ name }}"}, "outputs": {"name": "$.name"}, "guards": guards},
            {"id": f"{r}.delete", "method": "DELETE", "path": f"/{r}/{{{{ id }}}}", "inputs": ["id"],
             "outputs": {"deleted": "$.deleted"}, "guards": guards},
        ]
        dump(os.path.join(group_dir(base, g), f"{r}.api.yaml"), {"apis": apis})


def gen_givens(base):
    """Returns list of (givenId, exposedField). Givens may use earlier givens (acyclic)."""
    givens = []
    for f in range(N_GIVEN_FILES):
        items = []
        for k in range(3):
            g = RNG.randrange(N_GROUPS)
            r = group_name(g)
            gid = f"seed{f:03d}.{r}_{k}"
            scope = RNG.choices(["run", "file", "test"], weights=[3, 5, 2])[0]
            entry = {"id": gid, "scope": scope, "api": f"{r}.create"}
            if givens and RNG.random() < 0.3:
                dep_id, dep_field = RNG.choice(givens)
                entry["uses"] = {"parent": dep_id}
                entry["with"] = {"name": f"child-of-{{{{ parent.{dep_field} }}}}"}
            else:
                entry["with"] = {"name": f"seed-{r}-{k}"}
            entry["expose"] = {"entityId": "id", "price": "firstPrice"}
            items.append(entry)
            givens.append((gid, "entityId"))
        dump(os.path.join(base, f"domain{f % 20:02d}", "givens", f"seeds{f:03d}.given.yaml"), {"givens": items})
    return givens


def gen_tests(base, givens):
    for t in range(N_TEST_FILES):
        g = t % N_GROUPS
        r = group_name(g)
        tests = []
        n = RNG.randint(5, 10)
        for k in range(n):
            tid = f"{r}.t{t:03d}_{k}"
            op = RNG.choices(["get", "list", "create", "update", "delete"], weights=[5, 2, 2, 1, 1])[0]
            test = {"id": tid}
            uses_given = RNG.random() < 0.7 and op in ("get", "update", "delete")
            if uses_given:
                gid, field = RNG.choice(givens)
                test["uses"] = {"entity": gid}
                ident = f"{{{{ entity.{field} }}}}"
            else:
                ident = str(RNG.randint(1, 10_000))
            test["api"] = f"{r}.{op}"
            if op == "get":
                test["with"] = {"id": ident}
                expect = {"status": 200, "match": {"$.id": ident}, "schema": SCHEMA_ITEM}
            elif op == "list":
                expect = {"status": 200, "schema": SCHEMA_LIST}
            elif op == "create":
                test["with"] = {"name": f"created-{t}-{k}"}
                expect = {"status": 201, "match": {"$.name": f"created-{t}-{k}"}, "schema": SCHEMA_ITEM}
            elif op == "update":
                test["with"] = {"id": ident, "name": f"renamed-{k}"}
                expect = {"status": 200, "match": {"$.name": f"renamed-{k}"}}
            else:
                test["with"] = {"id": ident}
                expect = {"status": 200, "match": {"$.deleted": True}}
            if op in ("get", "list", "create") and RNG.random() < 0.8:
                expect["cel"] = RNG.choice(CEL_LIST if op == "list" else CEL_ENTITY)
            if RNG.random() < 0.05:
                test["repeat"] = RNG.randint(2, 3)
            test["expect"] = expect
            tests.append(test)
        dump(os.path.join(group_dir(base, g), "tests", f"t{t:03d}.test.yaml"), {"tests": tests})


def replace_in(path, old, new, count=1):
    with open(path) as f:
        s = f.read()
    assert old in s, (path, old)
    with open(path, "w") as f:
        f.write(s.replace(old, new, count))


def make_broken(ok, broken):
    shutil.copytree(ok, broken)
    # 1. unknown reference (typo): first test in t010 -> api typo
    t010 = os.path.join(group_dir(broken, 10), "tests", "t010.test.yaml")
    with open(t010) as f:
        doc = yaml.safe_load(f)
    api = doc["tests"][0]["api"]
    replace_in(t010, f"api: {api}", f"api: {api[:-2]}{api[-1]}{api[-2]}")  # swap last two chars
    # 2. duplicate id: t020 first test gets id of t220's first test (same group 20)
    t020 = os.path.join(group_dir(broken, 20), "tests", "t020.test.yaml")
    t220 = os.path.join(group_dir(broken, 20), "tests", "t220.test.yaml")
    with open(t220) as f:
        dup = yaml.safe_load(f)["tests"][0]["id"]
    with open(t020) as f:
        orig = yaml.safe_load(f)["tests"][0]["id"]
    replace_in(t020, f"id: {orig}\n", f"id: {dup}\n")
    # 3. cycle across two given files: seed050.*_0 -> seed051.*_0 -> seed050.*_0
    f50 = os.path.join(broken, "domain10", "givens", "seeds050.given.yaml")
    f51 = os.path.join(broken, "domain11", "givens", "seeds051.given.yaml")
    with open(f50) as f:
        a = yaml.safe_load(f)
    with open(f51) as f:
        b = yaml.safe_load(f)
    a["givens"][0]["uses"] = {"parent": b["givens"][0]["id"]}
    a["givens"][0]["with"] = {"name": "{{ parent.entityId }}"}
    b["givens"][0]["uses"] = {"parent": a["givens"][0]["id"]}
    b["givens"][0]["with"] = {"name": "{{ parent.entityId }}"}
    dump(f50, a)
    dump(f51, b)
    # 4. schema violation: typo'd key `exepct:` in a test file (tests are never referenced -> no cascade)
    t030 = os.path.join(group_dir(broken, 30), "tests", "t030.test.yaml")
    replace_in(t030, "  expect:\n", "  exepct:\n")


PROOF_API = """apis:
  - id: proof.create_file_seed
    method: POST
    path: /slow/file-seeds
    inputs: [name]
    body: { name: "{{ name }}" }
    outputs: { id: $.id }
    guards: { status: 2xx }
  - id: proof.create_run_seed
    method: POST
    path: /slow/run-seeds
    inputs: [name]
    body: { name: "{{ name }}" }
    outputs: { id: $.id }
    guards: { status: 2xx }
  - id: proof.create_test_seed
    method: POST
    path: /slow/test-seeds
    inputs: [name]
    body: { name: "{{ name }}" }
    outputs: { id: $.id }
    guards: { status: 2xx }
  - id: proof.get_item
    method: GET
    path: /proof-items/{{ id }}
    inputs: [id]
    outputs: { name: $.name }
    guards: { status: 2xx }
  - id: proof.ping
    method: POST
    path: /proof-repeat
    inputs: [ref]
    body: { name: "{{ ref }}" }
    guards: { status: 2xx }
"""

PROOF_GIVEN = """givens:
  - id: proof.file_seed
    scope: file
    api: proof.create_file_seed
    with: { name: shared-file-seed }
    expose: { seedId: id }
  - id: proof.run_seed
    scope: run
    api: proof.create_run_seed
    with: { name: shared-run-seed }
    expose: { seedId: id }
  - id: proof.test_seed
    scope: test
    api: proof.create_test_seed
    with: { name: per-test-seed }
    expose: { seedId: id }
"""


def gen_proof(base):
    os.makedirs(base, exist_ok=True)
    with open(os.path.join(base, "proof.api.yaml"), "w") as f:
        f.write(PROOF_API)
    with open(os.path.join(base, "proof.given.yaml"), "w") as f:
        f.write(PROOF_GIVEN)
    a = []
    for i in range(50):
        t = {"id": f"proof.a{i:02d}", "uses": {"seed": "proof.file_seed"}, "api": "proof.get_item",
             "with": {"id": "{{ seed.seedId }}"},
             "expect": {"status": 200, "match": {"$.id": "{{ seed.seedId }}"},
                        "cel": "size(body.items) > 0 && body.items.all(i, i.price > 0.0)"}}
        if i < 10:
            t["uses"]["run"] = "proof.run_seed"
        a.append(t)
    # guards vs assertions: api guard says 2xx, test asserts 404 and must pass
    a.append({"id": "proof.missing_is_404", "api": "proof.get_item", "with": {"id": "missing"},
              "expect": {"status": 404}})
    dump(os.path.join(base, "a", "file_a.test.yaml"), {"tests": a})
    b = [{"id": f"proof.b{i:02d}", "uses": {"run": "proof.run_seed"}, "api": "proof.get_item",
          "with": {"id": "{{ run.seedId }}"}, "expect": {"status": 200, "match": {"$.id": "{{ run.seedId }}"}}}
         for i in range(10)]
    b.append({"id": "proof.repeat5", "repeat": 5, "uses": {"seed": "proof.test_seed"}, "api": "proof.ping",
              "with": {"ref": "{{ seed.seedId }}"}, "expect": {"status": 201}})
    dump(os.path.join(base, "b", "file_b.test.yaml"), {"tests": b})


LOAD_API = """apis:
  - id: load.create
    method: POST
    path: /load
    inputs: [name]
    body: { name: "{{ name }}" }
    outputs: { id: $.id }
    guards: { status: 2xx }
  - id: load.get
    method: GET
    path: /load/{{ id }}
    inputs: [id]
    outputs: { name: $.name }
    guards: { status: 2xx }
"""

BENCH_BODY = {"id": "bench-1", "name": "bench", "active": True, "total": 101.5,
              "items": [{"sku": s, "price": p} for s, p in
                        [("a", 9.5), ("b", 12.25), ("c", 30.0), ("d", 0.5), ("e", 49.25)]]}


def main():
    if os.path.exists(ROOT):
        shutil.rmtree(ROOT)
    ok = os.path.join(ROOT, "ok")
    gen_apis(ok)
    givens = gen_givens(ok)
    gen_tests(ok, givens)
    make_broken(ok, os.path.join(ROOT, "broken"))
    gen_proof(os.path.join(ROOT, "proof"))
    os.makedirs(os.path.join(ROOT, "load"))
    with open(os.path.join(ROOT, "load", "load.api.yaml"), "w") as f:
        f.write(LOAD_API)
    import json
    with open(os.path.join(ROOT, "bench-body.json"), "w") as f:
        json.dump(BENCH_BODY, f)
    counts = {}
    for dp, _, fs in os.walk(ok):
        for fn in fs:
            k = fn.split(".")[-2]
            counts[k] = counts.get(k, 0) + 1
    print("generated", ROOT, counts)


if __name__ == "__main__":
    main()
