# Refactor Maintainability Audit

Date: 2026-06-28

This audit closes the maintainability refactor prompt pack. The pack preserved
PHP-visible behavior while consolidating execution ownership, diagnostic
formatting, source-integrity checks, known-gap validation, server boundaries,
and quality gates.

## Completed Prompts

| Prompt | Status | Notes |
| ---: | --- | --- |
| 01 | complete | Added fail-fast source-integrity checks for critical module wiring and generated arginfo shape. |
| 02 | complete | Made generated stdlib arginfo committed, reviewable, and drift-checkable against pinned php-src. |
| 03 | complete | Split `php_executor` into focused modules with stable public re-exports. |
| 04 | complete | Made `php_executor` the normal compile/execute owner for CLI-compatible and server execution paths. |
| 05 | complete | Split `php_vm_cli` process entrypoint from command implementation and shared entrypoints. |
| 06 | complete | Kept VM compile diagnostics typed through runtime diagnostic payloads. |
| 07 | complete | Kept CLI JSON/report status and diagnostics serialization typed. |
| 08 | complete | Moved server filesystem and PHP execution work behind explicit blocking boundaries. |
| 09 | complete | Added server smoke/verification and CI coverage. |
| 10 | complete | Documented separate CLI bytecode artifact cache and server compiled-script cache boundaries. |
| 11 | complete | Added manifest-driven runtime fixture runner and runtime gate wiring. |
| 12 | complete | Added machine-readable known-gap manifests and validator. |
| 13 | complete | Added `php_runtime` and `php_vm` API/experimental facades and migrated boundary imports. |
| 14 | complete | Contained reference/object interior mutability with checked accessors and debug GC facade. |
| 15 | complete | Split private array storage into packed and mixed variants behind `PhpArray`. |
| 16 | complete | Promoted `quality-fast`, dependency policy, unused-dependency checks, and docs smoke to CI. |
| 17 | complete | Updated crate docs, README links, layer contracts, and architecture docs. |
| 18 | complete | Ran the final regression sweep and produced this audit. |

## Skipped Or Deferred

No prompt was skipped. The following remain explicit deferred work rather than
prompt failures:

- Full Zend SAPI/FPM/CGI/Apache compatibility is out of scope for the integrated server.
- Server execution timeout that safely interrupts long-running PHP VM execution is not implemented.
- Cross-process OPcache-style dependency graph invalidation is not implemented.
- Optional Cranelift fastest-engine rows, persistent-feedback rows, Callgrind, and Miri smoke remain opt-in or platform/toolchain dependent.
- Historical `todo_*` and `*_skeleton_status()` exports remain as compatibility markers for early wiring tests; current docs state they are not architecture truth.

## Ownership Map

| Area | Owner | Contract |
| --- | --- | --- |
| Source integrity | `scripts/verify/source_integrity.py`, `just source-integrity` | Fail fast on missing critical module wiring or generated arginfo API shape. |
| Generated arginfo | `scripts/stdlib/generate_arginfo.py`, `scripts/stdlib/verify_generated_arginfo.sh`, `crates/php_std/src/generated/arginfo.rs` | Committed snapshot, strict drift check against pinned PHP 8.5.7. |
| Compile/execute orchestration | `crates/php_executor` | Canonical normal execution path for CLI-compatible and server execution. |
| PHP diagnostic formatting for execution | `crates/php_executor/src/diagnostics.rs` | One PHP-shaped stderr/status mapping path for executor-backed execution. |
| CLI process behavior | `crates/php_vm_cli` | Args, compatibility binaries, bytecode disk cache policy, debug/report commands. |
| HTTP transport | `crates/php_server` | Routing, static files, request limits, metrics, response mapping, blocking boundaries. |
| Runtime values and request state | `php_runtime::api` | Stable runtime values, context, diagnostics, resources, output, status, builtins. |
| Runtime debug metadata | `php_runtime::debug` / `php_runtime::experimental` | GC graph inspection and test/debug-only weak handles. |
| VM execution | `php_vm::api` | `Vm`, `VmOptions`, `VmResult`, compiled unit and include-loader API. |
| VM instrumentation | `php_vm::experimental` | Counters, quickening, inline caches, tiering, fallback, deopt, dense bytecode. |
| Known gaps | `docs/known_gaps/*.jsonl`, `scripts/known_gaps/validate.py` | One machine-readable validation path for runtime, performance, and PHPT accepted non-green gaps. |
| Server cache | `php_executor::CompiledScriptCache` | Process-local compiled entry-script cache for HTTP requests only. |
| CLI bytecode cache | `php_bytecode_cache`, `php_vm_cli` | Disk artifact cache for local CLI/performance use only. |

## Remaining Duplicate Hot Spots

- `crates/php_vm_cli/src/commands.rs` still owns many specialized report/debug
  paths. Normal `run` execution delegates through the executor, but inspection
  commands legitimately need frontend, optimizer, bytecode, and VM internals.
- CLI bytecode artifact cache and server compiled-script cache remain separate.
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
| `git status --short` | pass | Dirty worktree contains the prompt-pack changes only; no `target/` artifacts are staged. |
| `nix develop -c cargo fmt --all --check` | pass | Formatting clean. |
| `nix develop -c cargo check --workspace --all-targets` | pass | Workspace compile clean. |
| `nix develop -c cargo test --workspace` | pass | Workspace tests and doctests passed. |
| `nix develop -c just source-integrity` | pass | Source-integrity script passed. |
| `nix develop -c just verify-server` | pass | Executor/server tests and server smoke passed. |
| `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php nix develop -c just verify-runtime` | pass | Runtime semantics diff: 280 total, 230 pass, 0 fail, 0 skip, 50 known gaps. |
| `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php nix develop -c just verify-stdlib` | pass | Stdlib diffs had 0 skips with the pinned PHP 8.5.7 reference. |
| `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php nix develop -c just verify-performance` | pass | Callgrind skipped on Darwin; Miri smoke skipped because cargo-miri is not usable for the active toolchain; optional JIT/persistent-feedback rows remain opt-in. |
| `PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src nix develop -c just verify-phpt` | pass | Verified 21,548 baseline entries, 20,428 known non-green fingerprints, and 24,475 php-src manifest entries. |
| `nix develop -c just quality-fast` | pass | Source integrity, known gaps, dependency policy, unused deps, all-features compile, rustdoc, and doctests passed. |
| `nix develop -c just quality-docs` | pass | Rustdoc warnings denied and doctests passed for Prompt 17 docs changes. |

`quality-fast` reports warning-only duplicate transitive crates in
`cargo-deny`: `bitflags`, `hashbrown`, and `windows-sys`. It also reports
warning-only all-features dead-code warnings for default-off JIT helper
functions in `crates/php_vm/src/vm/mod.rs`.

## Artifact Hygiene

No raw PHPT output, local php-src checkout, local reference binary, or `target/`
artifact is part of the intended commit set. Tracked generated summary docs
updated by verified gates are concise committed summaries, not raw run output:

- `docs/stdlib-function-coverage.md`
- `docs/hotpath-inventory.md`
- `docs/performance-fastest-hotpaths.md`
- `docs/performance-fastest-engine-results.md`

## Known Risks

- `crates/php_vm_cli/src/commands.rs` remains large and should be split by
  command family in a follow-up to reduce review load.
- The server has no safe VM preemption point for per-script execution timeouts.
- Server cache invalidation is entry-script based; include/autoload dependency
  graph invalidation is later work.
- Optional JIT helpers are feature-gated enough for tests but still produce
  all-features dead-code warnings.
- Reference-sensitive gates depend on a local PHP 8.5.7 binary or php-src tree.
  This audit used the sibling checkout under `/Volumes/CrucialMusic/src/phrust/third_party/php-src`.

## Recommended Next Prompt Pack

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
