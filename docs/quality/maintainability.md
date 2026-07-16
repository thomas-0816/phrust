# Maintainability

This document records the current maintainability ownership map, validation
surface, and known risks. The code preserves PHP-visible behavior while keeping
execution ownership, diagnostic formatting, source-integrity checks, known-gap
validation, server boundaries, and quality gates explicit.

## Completed Maintenance Work

- Source-integrity checks fail fast for critical module wiring and generated
  arginfo shape.
- Generated standard-library arginfo is committed, reviewable, and
  drift-checkable against pinned php-src.
- `php_executor` owns normal compile/execute orchestration for CLI-compatible
  and server execution paths.
- `php_vm_cli` keeps process entrypoint code separate from command
  implementation and shared entrypoints.
- VM compile diagnostics stay typed through runtime diagnostic payloads.
- CLI JSON/report status and diagnostics serialization stay typed.
- Server filesystem and PHP execution work run behind explicit blocking
  boundaries.
- Server smoke/verification and CI coverage are part of the validation surface.
- Persistent native artifact and server compiled-script cache boundaries are
  documented separately.
- Runtime fixture execution is manifest-driven and wired into runtime gates.
- Known gaps have machine-readable manifests and validator coverage.
- `php_runtime` and `php_vm` expose API and experimental facades for boundary
  imports.
- Reference/object interior mutability uses checked accessors and a debug GC
  facade.
- `PhpArray` hides packed and mixed private storage variants behind one public
  type.
- `quality-fast`, dependency policy, unused-dependency checks, and docs smoke
  run in CI.
- Crate docs, README links, layer contracts, and architecture docs describe the
  current code.

## Skipped Or Deferred

The following remain explicit deferred work:

- Full Zend SAPI/FPM/CGI/Apache compatibility is out of scope for the integrated server.
- Server execution timeout that safely interrupts long-running PHP VM execution is not implemented.
- Cross-process OPcache-style dependency graph invalidation is not implemented.
- Optional Cranelift fastest-engine rows, persistent-feedback rows, Callgrind, and Miri smoke remain opt-in or platform/toolchain dependent.
- Removed early wiring markers stay removed from public APIs; current tests assert
  real layer behavior and public facades.

## Ownership Map

| Area | Owner | Contract |
| --- | --- | --- |
| Source integrity | `scripts/verify/source_integrity.py`, `just source-integrity` | Fail fast on missing critical module wiring or generated arginfo API shape. |
| Generated arginfo | `scripts/stdlib/generate_arginfo.py`, `scripts/stdlib/verify_generated_arginfo.sh`, `crates/php_std/src/generated/arginfo.rs` | Committed snapshot, strict drift check against pinned PHP 8.5.7. |
| Compile/execute orchestration | `crates/php_executor` | Canonical normal execution path for CLI-compatible and server execution. |
| PHP diagnostic formatting for execution | `crates/php_executor/src/diagnostics.rs` | One PHP-shaped stderr/status mapping path for executor-backed execution. |
| CLI process behavior | `crates/php_vm_cli` | Args, compatibility binaries, native cache policy, debug/report commands. |
| HTTP transport | `crates/php_server` | Routing, static files, request limits, metrics, response mapping, blocking boundaries. |
| Runtime values and request state | `php_runtime::api` | Stable runtime values, context, diagnostics, resources, output, status, builtins. |
| Runtime debug metadata | `php_runtime::debug` / `php_runtime::experimental` | GC graph inspection and test/debug-only weak handles. |
| VM execution | `php_vm::api` | `Vm`, `VmOptions`, `VmResult`, compiled unit and include-loader API. |
| Native execution instrumentation | `php_vm::experimental` | Native counters, compile/cache statistics, transition metadata, and diagnostics. |
| Known gaps | `docs/known_gaps/*.jsonl`, `scripts/known_gaps/validate.py` | One machine-readable validation path for runtime, performance, and PHPT accepted non-green gaps. |
| Server cache | `php_executor::CompiledScriptCache` | Process-local compiled entry-script cache for HTTP requests only. |
| Persistent native cache | `php_jit`, `php_vm`, `php_vm_cli` | Validated restart-persistent PNA2 unit bundles with symbolic helper relocation and W^X publication. |

## Remaining Duplicate Hot Spots

- `crates/php_vm_cli/src/commands.rs` still owns many specialized report/debug
  paths. Normal `run` execution delegates through the executor, but inspection
  commands legitimately need frontend, optimizer, bytecode, and VM internals.
- Persistent native artifacts and server compiled-script caches remain separate.
  This is intentional because they have different lifetimes and trust
  boundaries, but both must preserve the same "miss over unsafe reuse" rule.
- Some diagnostics formatting remains command-specific for non-execution
  reports. PHP-shaped program execution should continue to route through
  `php_executor::diagnostics`.
- Performance summary docs are generated from target artifacts and committed as
  concise summaries. Raw JSON, counters, captures, and benchmark outputs remain
  under `target/` and are not committed.

## Shell-Heavy Gates

These gates are still shell/Python-script heavy and should stay deterministic:

- `scripts/server/smoke.sh`
- `scripts/phpt/verify_foundation.sh`
- `scripts/phpt/verify_source_integrity.sh`
- `scripts/performance/*.sh`
- `scripts/stdlib-docs.sh`
- `scripts/stdlib-coverage.sh`
- `scripts/stdlib/verify_generated_arginfo.sh`

They currently report clear pass/skip/fail messages. Future work should prefer
moving complex parsing and policy logic into typed Rust or Python tools, leaving
shell scripts as thin orchestration wrappers.

## Gate Matrix

| Command | Result | Notes |
| --- | --- | --- |
| `git status --short` | pass | Dirty worktree contains the intended docs and tooling changes only; no `target/` artifacts are staged. |
| `nix develop -c cargo fmt --all --check` | pass | Formatting clean. |
| `nix develop -c cargo check --workspace --all-targets` | pass | Workspace compile clean. |
| `nix develop -c cargo test --workspace` | pass | Workspace tests and doctests passed. |
| `nix develop -c just source-integrity` | pass | Source-integrity script passed. |
| `nix develop -c just verify-server` | pass | Executor/server tests and server smoke passed. |
| `REFERENCE_PHP=$REFERENCE_PHP nix develop -c just verify-runtime` | pass | Runtime semantics diff: 280 total, 230 pass, 0 fail, 0 skip, 50 known gaps. |
| `REFERENCE_PHP=$REFERENCE_PHP nix develop -c just verify-stdlib` | pass | Stdlib diffs had 0 skips with the pinned PHP 8.5.7 reference. |
| `REFERENCE_PHP=$REFERENCE_PHP nix develop -c just verify-performance` | pass | Callgrind skipped on Darwin; Miri smoke skipped because cargo-miri is not usable for the active toolchain; optional JIT/persistent-feedback rows remain opt-in. |
| `PHP_SRC_DIR=$PHP_SRC_DIR nix develop -c just verify-phpt` | pass | Verified 21,548 baseline entries, 20,428 known non-green fingerprints, and 24,475 php-src manifest entries. |
| `nix develop -c just quality-fast` | pass | Source integrity, known gaps, dependency policy, unused deps, all-features compile, rustdoc, and doctests passed. |
| `nix develop -c just quality-docs` | pass | Rustdoc warnings denied and doctests passed for docs changes. |

`quality-fast` reports warning-only duplicate transitive crates in
`cargo-deny`: `bitflags`, `hashbrown`, and `windows-sys`. It also reports
warning-only all-features dead-code warnings for default-off JIT helper
functions in `crates/php_vm/src/vm/mod.rs`.

## Artifact Hygiene

No raw PHPT output, local php-src checkout, local reference binary, or `target/`
artifact is part of the intended commit set. Tracked generated summary docs
updated by verified gates are concise committed summaries, not raw run output:

- `docs/stdlib/function-coverage.md`
- `target/performance/hotpath-inventory.md`
- `target/performance/fastest/hotpath-report.md`
- `target/performance/fastest/matrix.md`

## Known Risks

- `crates/php_vm_cli/src/commands.rs` remains large and should be split by
  command family in a follow-up to reduce review load.
- The server has no safe VM preemption point for per-script execution timeouts.
- Server cache invalidation is entry-script based; include/autoload dependency
  graph invalidation is later work.
- Optional JIT helpers are feature-gated enough for tests but still produce
  all-features dead-code warnings.
- Reference-sensitive gates depend on a local PHP 8.5.7 binary or php-src tree.
  This audit used the sibling checkout under `$PHP_SRC_DIR`.

## Recommended Next Work

1. Split `php_vm_cli/src/commands.rs` into command-family modules without
   changing stdout, stderr, JSON, or exit status.
2. Clean up default-off JIT helper cfg boundaries so `quality-fast` is warning
   free under `--all-features`.
3. Add a shared reference-discovery helper for `REFERENCE_PHP`, `PHP_SRC_DIR`,
   branch-local `third_party/php-src`, and approved sibling checkouts.
4. Convert shell-heavy PHPT and performance orchestration into typed tool
   commands where the policy is currently encoded in shell glue.
5. Add server dependency-graph cache invalidation metadata for includes and
   autoload-sensitive requests.
6. Add a focused execution-timeout design for VM requests before widening server
   load or production-readiness claims.
