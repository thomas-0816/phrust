# Performance Experimental Release Build Profiles

The performance layer is intentionally optional. The default debug/dev workflow and
standard Cargo profiles stay unchanged; release-profile experiments are local
measurement recipes only.

## Candidate Settings

Settings to evaluate for release binaries:

| Setting | Candidate | Reason | Risk |
| --- | --- | --- | --- |
| Link-time optimization | `-C lto=fat` | Allows whole-program inlining across crates | Slower builds, larger memory use |
| Codegen units | `-C codegen-units=1` | Gives LLVM more cross-module visibility | Slower incremental rebuilds |
| PGO generate | `-C profile-generate=target/performance/pgo-data` | Collects workload profile data | Profile data is host/workload-specific |
| PGO use | `-C profile-use=target/performance/pgo.profdata` | Guides branch layout and inlining | Stale profile can degrade performance |

Do not commit `target/performance/pgo-data`, `*.profraw`, `*.profdata`, or generated
benchmark reports.

## Local Plan

Print the local checklist:

```bash
nix develop -c just release-profile-plan
```

The before/after comparison must use the same benchmark matrix, repetitions,
warmups, feature flags, and machine state. A valid local comparison has:

1. `nix develop -c just perf-baseline`
2. one experimental release build
3. `scripts/performance/bench_matrix.py` pointed at that release binary
4. `scripts/performance/compare_perf_json.py` comparing baseline and candidate JSON

## CI Policy

LTO/PGO is not required in CI for Performance. A future release pipeline may add a
separate package once build time and artifact reproducibility are understood.
