### Compile (1,000 files)

| variant | wall ms (median) | in-process ms | peak RSS MB | CPU s | wall range |
|---|---|---|---|---|---|
| go | 352 | 336 | 68 | 0.63 | 320–391 |
| go (GOMAXPROCS=1) | 612 | 597 | 68 | 0.61 | 562–647 |
| rust | 387 | 362 | 68 | 0.72 | 333–453 |
| rust (1 thread) | 738 | 707 | 68 | 0.73 | 697–785 |
| ts-bun (eager spans) | 1972 | 1866 | 194 | 2.96 | 1837–2118 |
| ts-bun (lazy spans) | 492 | 398 | 92 | 0.77 | 424–605 |
| ts-node (eager spans) | 2665 | 2517 | 190 | 3.11 | 2565–3146 |
| ts-node (lazy spans) | 601 | 466 | 120 | 0.77 | 567–654 |
| go (goccy, GOMAXPROCS=1) | 1499 | 1466 | 263 | 1.50 | 1286–1617 |
| go (goccy/go-yaml v1.19.2 parser, superseded) | 876 | 853 | 246 | 1.64 | 773–939 |

### CEL (1,000,000 evals, compiled once)

| impl | median ns/op | runs |
|---|---|---|
| go | 3368 | 3708, 3518, 3246, 3185, 3368 |
| rust | 4757 | 4757, 4717, 4925, 4070, 4909 |
| ts-bun | 1290 | 1506, 1006, 1302, 1290, 1245 |
| ts-node | 1448 | 1503, 1324, 1144, 1448, 1648 |

### Load

| impl@VUs | req/s | p50 ms | p99 ms | errors | CPU % | peak RSS MB | CPU ms / 1k req | runs |
|---|---|---|---|---|---|---|---|---|
| go@1000 | 17178 | 53.2 | 111.6 | 0 | 98 | 121 | 57.3 | 3 |
| go@5000 | 16579 | 262.1 | 540.7 | 0 | 98 | 534 | 59.4 | 3 |
| rust@1000 | 29769 | 11.5 | 151.6 | 0 | 98 | 72 | 32.9 | 3 |
| rust@5000 | 26899 | 12.7 | 819.2 | 0 | 98 | 296 | 36.6 | 3 |
| ts-bun@1000 | 22181 | 42.0 | 86.0 | 0 | 97 | 81 | 43.5 | 3 |
| ts-bun@5000 | 20558 | 233.5 | 356.4 | 0 | 98 | 124 | 48.0 | 3 |
| ts-node@1000 | 10699 | 85.0 | 241.7 | 0 | 99 | 224 | 93.1 | 1 |
| ts-node@5000 | 7854 | 573.4 | 1441.8 | 0 | 98 | 415 | 128.4 | 1 |

### Startup (--version)

| impl | median ms | stddev ms | peak RSS MB | artifact MB |
|---|---|---|---|---|
| go | 9.5 | 1.8 | 15 | 22.5 |
| rust | 2.1 | 0.2 | 7 | 15.8 |
| ts-bun | 9.3 | 1.2 | 24 | 81.1 |
| ts-node | 96.5 | 9.3 | 56 | 2.1 |

### LOC

| impl | code | comments | blank | files |
|---|---|---|---|---|
| generator | 290 | 6 | 35 | 1 |
| go | 1561 | 49 | 142 | 9 |
| mock-server | 116 | 14 | 14 | 1 |
| rust | 1344 | 38 | 152 | 9 |
| ts | 876 | 47 | 107 | 9 |

### Build

- go: cold_build_s (empty GOCACHE, includes deps+std) = 31.29
- go: incremental_build_s (touch compiler.go) = 0.17
- go: vet_s = 0.37
- rust: cold_release_build_s (cargo clean, includes deps) = 185.33
- rust: incremental_check_s (cargo check, touch) = 0.51
- rust: incremental_debug_build_s (touch) = 1.44
- rust: incremental_release_build_s (touch compiler.rs) = 75.22
- ts: bundle_s (bun build --target=node) = 0.10
- ts: compile_binary_s (bun build --compile) = 0.48
- ts: typecheck_s (tsc 7 --noEmit) = 0.75

### Proof

| impl | pass | file-scope creates (50 tests) | run-scope creates (20 tests) | test-scope creates (repeat 5) | repeat requests | full suite |
|---|---|---|---|---|---|---|
| go | True | 1 | 1 | 5 | 5 | tests: 5668 total, 5668 passed, 0 failed in 920.5 ms (8181 req) |
| rust | True | 1 | 1 | 5 | 5 | tests: 5668 total, 5668 passed, 0 failed in 502.1 ms (8181 req) |
| ts-bun | True | 1 | 1 | 5 | 5 | tests: 5668 total, 5668 passed, 0 failed in 747.9 ms (8181 req) |
| ts-node | True | 1 | 1 | 5 | 5 | tests: 5668 total, 5668 passed, 0 failed in 1593.1 ms (8181 req) |
