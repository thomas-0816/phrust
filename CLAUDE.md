# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

`phrust` is a **new, independent (clean-room) PHP 8.5 engine written in Rust** —
its own lexer → parser/CST → typed AST → semantic frontend/HIR → IR → runtime →
VM, plus its **own** performance stack: a conservative optimizer (`php_optimizer`),
quickening and inline caches, a bytecode cache / opcache (`php_bytecode_cache`),
and a JIT (`php_jit`, Cranelift backend, feature-gated/opt-in). These are
phrust's own designs — a *different* opcache and JIT from the original C
php-src, not ports of Zend Opcache or the Zend JIT.

It re-implements PHP *behavior*; it is **not** a reimplementation of the Zend
engine and does **not** emulate the Zend ABI, reuse or link Zend/PHP C
extensions, or provide a php-src-compatible SAPI or extension ABI.

`php-src` is never code in this project. It is used only as: (1) a behavioral
**reference oracle** — real PHP 8.5.7 run to diff output against; (2) a read-only
**PHPT test corpus**; and (3) a source of **metadata** (arginfo/stubs) that is
*extracted*, never copied. Do not vendor php-src, port its C, or depend on Zend
internals — match observable behavior instead.

See also `AGENTS.md` (scope/layer boundaries), `README.md` (layout), `docs/adr/`
(decisions), and `docs/phpt/` (PHPT workflow). Repository docs describe current
architecture, contracts, commands, and compatibility status. Active
implementation tasks belong in issues, PR descriptions, or external task notes;
committed docs should be organized by layer, module, or function.

## Development environment

**Everything runs inside the Nix dev shell.** Bare `cargo`/`just` will miss the
toolchain, the `sccache`/`mold` wiring, and the pinned env vars. Either prefix
each command with `nix develop -c …`, or open `nix develop` once and run inside
it. The shell sets `RUSTC_WRAPPER=sccache`, `CARGO_TARGET_DIR=$PWD/target`, and
`RUSTFLAGS=-C link-arg=-fuse-ld=mold` (Linux). Build outputs land in
`target/debug/` (e.g. `target/debug/php-vm`, used directly by some tests).

Install the versioned git hooks once per checkout:

```bash
nix develop -c just install-hooks
```

The pre-commit hook runs a lightweight fmt + source-integrity gate in one Nix
shell; pre-push runs a bounded local push gate with a default 20 minute timeout.
Full local CI parity remains available as `just ci-local`. `PHRUST_SKIP_GIT_HOOKS=1`
exists only for exceptional, manually-verified cases.

## Common commands

```bash
# format / lint / test the whole workspace
nix develop -c just fmt
nix develop -c cargo clippy --workspace --all-targets -- -D warnings
nix develop -c cargo test --workspace

# one crate, or one test by name
nix develop -c cargo test -p php_runtime
nix develop -c cargo test -p php_runtime <test_name_substring>
nix develop -c cargo test -p php_syntax --test fixture_roundtrip   # one integration test file
# cargo-nextest is available: nix develop -c cargo nextest run -p <crate>

# run a PHP file through the engine (developer VM CLI = php-vm)
nix develop -c cargo run -p php_vm_cli --bin php-vm -- run path/to/file.php
# frontend analysis CLI = php-frontend (analyze | diagnostics | symbols | hir | …)
nix develop -c cargo run -p php_frontend_cli --bin php-frontend -- analyze path/to/file.php --format json

# discover the canonical gates (source of truth for recipe names)
nix develop -c just help
```

Use the **narrowest** relevant gate while iterating, then the matching aggregate
gate before handoff. `just ci-local` mirrors the default CI checks locally.

## Validation gates

Per-layer aggregate gates (run the one for the layer you touched):

```bash
nix develop -c just verify-frontend      # lexer, parser, CST, AST, semantics, frontend snapshots
nix develop -c just verify-runtime       # bytecode, VM, runtime + runtime-semantics fixtures/diff
nix develop -c just verify-stdlib        # stdlib docs/coverage + arginfo/extension diffs
nix develop -c just verify-performance   # optimizer/cache/JIT smoke + perf regression
nix develop -c just verify-phpt          # PHPT foundation, baseline, source integrity
```

Focused gates exist for each step (`just lexer-fixtures`, `parser-fixtures`,
`semantic-fixtures`/`semantic-diff`, `bytecode-snapshots`, `vm-smoke`,
`runtime-fixtures`, `diff-*`, etc.) — see `just help`.

## Architecture

### The single pipeline (do not fork it)

```
php_source → php_lexer → php_syntax (lossless CST) → php_ast (typed views)
          → php_semantics (HIR, scopes, name resolution, diagnostics)
          → php_ir → php_runtime (values, COW arrays, builtins) → php_vm → CLIs
```

This is the **only** input path. The hardest architectural rule in the repo:
never add a second lexer, parser, AST, semantic frontend, or any
source-string-matching execution path. Each layer consumes the previous one's
typed output; lower layers must not re-derive frontend information. Where a fix
belongs (functional ownership):

- Lexing / parsing / CST → `php_lexer`, `php_syntax`
- Typed views & compile-time metadata → `php_ast`, `php_semantics`
- Lowering / bytecode boundary → `php_ir`
- Runtime values, conversions, arrays, resources, builtins → `php_runtime`, `php_std`
- Execution semantics → `php_vm`
- Performance (optimizer, quickening, inline caches, opcache, JIT) →
  `php_optimizer`, `php_bytecode_cache`, `php_jit`, `php_perf` — must preserve
  PHP-visible output, diagnostics, exit status, and side-effect order
- Developer + PHPT target commands → `php_vm_cli`
- PHPT indexing / runner / triage / baselines → `php_phpt_tools`, `scripts/phpt/`

Cross-cutting invariants: byte-based spans are the source of truth (line/column
are derived display data); never hardcode numeric PHP token IDs — compare by
token name/text/diagnostics/position; public lexer/parser APIs must not panic on
invalid input; unsupported behavior produces a deterministic diagnostic or a
documented known gap, never a plausible-but-wrong result.

### Differential testing against a pinned reference

Correctness is defined by matching real PHP **8.5.7** (the version is fixed by
ADR `docs/adr/0001`; do not advance it without a new ADR). Comparison scripts
under `scripts/` resolve a reference `php` in this order: `$REFERENCE_PHP` →
built `third_party/php-src/sapi/cli/php` → system `php`. They **must skip when
the reference is not exactly 8.5.7** (a non-8.5.7 php mis-tokenizes 8.5 syntax
like the `|>` pipe and yields false diffs), and **must be strict** when
`REFERENCE_PHP` is set explicitly. This is why most reference-dependent gates
SKIP in CI (no reference is built there) but run for real locally after
`just bootstrap-ref` + `just build-ref-php`. Never let a check silently use the
wrong php or silently skip — report the exact skip reason.

### PHPT corpus & baseline model (bookkeeping is layered)

The upstream PHPT corpus is the north star. `third_party/php-src/` PHPT files are
**read-only inputs** (never edit them; never vendor php-src). Generated/minimized
fixtures live in `tests/phpt/generated/` with provenance in the manifests; raw run
artifacts go under `target/phpt-work/full-runs/<timestamp>/` and are never
committed. `phrust-php` (in `php_vm_cli`) is the CLI the runner targets.

Bookkeeping has three layers. **The committed manifests are the source of truth;
the module docs are rendered summaries — never hand-edit their counts.**

1. **Corpus + baseline** — the committed source of truth, refreshed by a full run:
   - `tests/phpt/manifests/phpt-corpus.jsonl` — discovered corpus
   - `tests/phpt/manifests/full-baseline-metadata.json` — totals
   - `tests/phpt/manifests/full-known-failures.jsonl` — every known-non-green fingerprint
   - `tests/phpt/manifests/full-baseline-module-counts.jsonl` — per-module PASS/SKIP/FAIL/BORK
     (lets a fresh checkout reproduce module status without `target/`)
   - `tests/phpt/manifests/known-gap-catalog.jsonl`
   - `target/phpt-work/reports/full-baseline.md` — the human report

   **Contract:** if the Markdown report shows any non-green outcome, the machine
   known-failure manifest must contain the matching fingerprints (a non-green
   report can never have an empty machine baseline).

2. **Module projection** — `php-phpt-tools triage` (`just phpt-triage`) maps corpus
   paths + known failures into functional modules and re-renders
   `docs/phpt/modules/{README,<module>}.md`,
   `tests/phpt/manifests/module-priority.json`,
   `tests/phpt/manifests/modules/<module>.{json,selected.jsonl}`,
   `target/phpt-work/reports/triage.md`, `docs/phpt/extension-policy.md`, and
   `docs/phpt/known-gaps.md`. All derived from layer 1 — regenerate, don't edit.

3. **Verification** — `just verify-phpt` runs `php-phpt-tools verify-baseline`
   (`just phpt-verify-baseline`), enforcing: report totals == metadata; corpus
   count == `phpt-corpus.jsonl`; `full-known-failures.jsonl` has exactly
   `known_failure_count` rows; FAIL/BORK == metadata; every
   `primary_missing_feature_guess` and every BORK subclass has a known-gap row;
   module-count sums == metadata; and the non-green/empty-baseline contract above.

Never accept new known failures implicitly — `PHPT_ACCEPT_BASELINE=1` must be
explicit and justified; any new FAIL/BORK fingerprint is otherwise rejected.

```bash
nix develop -c just phpt-triage                          # re-project modules from baseline
nix develop -c just phpt-module MODULE=standard.strings  # module gate (selected set)
PHPT_RUN_FULL=1 nix develop -c just phpt-full-regression # refresh the committed baseline (acceptance)
nix develop -c just verify-phpt                          # enforce the contract above
# focused iteration (only the needed tests): just phpt-dev-build, then
#   just phpt-fast MODULE=<m> FILE=<phpt>   |   just phpt-rerun-failures MODULE=<m>
```

### Builtins from arginfo

Builtin signatures are generated from php-src arginfo/stubs, not hand-written
(`just generate-arginfo`, `just verify-stdlib`). Extract metadata only — never
copy php-src C implementations. Overrides stay separate and documented.

## Conventions

- Conventional commits: `type(scope): description`, imperative, first line < 72
  chars. Do not mention development provenance/tooling in commit messages.
- Bash scripts use `set -euo pipefail` and stay deterministic; mark new scripts
  executable. Update docs alongside tooling changes.
- In a dirty worktree, stage only files you intentionally changed; never revert
  unrelated user changes. Never commit `target/`, local php-src checkouts, or
  reference binaries.
- Every behavior fix needs a focused regression fixture (PHPT or minimized
  fixture with provenance); every known gap needs a stable ID, reference vs.
  current behavior, an example fixture, the owning layer, and a baseline count.
