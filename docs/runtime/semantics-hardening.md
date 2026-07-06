# Runtime semantics hardening

Runtime semantics touches reference cells, Copy-on-Write arrays and strings, GC debug
hooks, destructor queues, generators, fibers, and VM continuation stacks. This
audit records the Rust `unsafe` boundary and the opt-in hardening checks.

## Current Unsafe Inventory

Audit command:

```bash
rg -n "\bunsafe\b" crates scripts docs flake.nix justfile .github --glob '!target/**'
```

As of this Runtime semantics audit, there are no Rust `unsafe` blocks, `unsafe fn`
definitions, or unsafe trait impls under `crates/`. Matches are documentation
text only.

The required hardening lint is:

```bash
nix develop -c just runtime-hardening-lints
```

That target runs Clippy for `php_runtime` and `php_vm` with
`-D warnings -D unsafe-code`, so any newly introduced unsafe code in the
runtime or VM fails `verify-runtime`.

## Clippy Allow Inventory

The remaining runtime/VM Clippy allowances are narrow:

- `crates/php_vm/src/vm/mod.rs` allows `clippy::result_large_err` because VM
  runtime errors currently carry structured diagnostics and values.
- `crates/php_vm/src/vm/mod.rs` allows `clippy::too_many_arguments` for VM helper
  functions that thread execution state explicitly. This is noisy but preferable
  to hiding mutable VM state in broader global abstractions during Runtime semantics.
- `crates/php_ir/src/lower/expressions.rs` and `crates/php_ir/src/lower/statements.rs`
  have `too_many_arguments` helpers from the existing lowering pipeline.

No allow-list suppresses Rust unsafe-code lints.

## DevShell Audit

The reproducible Runtime semantics devshell must expose:

```text
cargo rustc rustfmt cargo-clippy just jq python3 rg shellcheck clang sccache
```

It must also set:

```text
PHP_REF_SERIES=8.5
PHP_REF_VERSION=8.5.7
PHP_REF_TAG=php-8.5.7
CARGO_TARGET_DIR
SCCACHE_DIR
```

`nix develop -c just runtime-toolchain-audit` checks those requirements. The
target intentionally avoids host-specific absolute paths; the shell may use the
developer's `PATH` for optional tools, but required Runtime semantics validation tools
must be available inside `nix develop`.

## Optional Miri

Miri is useful for reference/GC model tests, but it depends on a Miri-capable
Rust toolchain. The Nix devshell uses the pinned stable Rust package and does
not make Miri mandatory.

Run:

```bash
nix develop -c just runtime-miri-smoke
```

The target runs a small `php_runtime` reference model test when `cargo miri` is
usable. It exits successfully with a `[skip]` message when Miri is unavailable
or unusable for the active toolchain.

## Optional Sanitizer

AddressSanitizer requires Linux plus a sanitizer-capable Rust setup. It is not
part of CI or `verify-runtime`.

Run:

```bash
nix develop -c env PHRUST_RUN_SANITIZER=1 just runtime-sanitizer-smoke
```

The target runs one GC smoke test with sanitizer flags when supported and skips
otherwise. If this becomes stable enough for CI, add a separate non-blocking CI
job before making it required.

## Safe Model Tests

work item D keeps the runtime prototype mode unsafe-free by comparing core public
runtime APIs against tiny safe models:

```bash
nix develop -c cargo test -p php_runtime -- model
```

The model tests cover reference-slot alias writes versus by-value copies, and
Copy-on-Write array mutation versus an independent vector model. They are small
unit tests, not a replacement for the Runtime semantics fixture or differential gates.

## CI Artifacts

The Runtime semantics CI workflow uploads fixture and diff reports from `target/runtime-semantics`
and runtime smoke reports from `target/runtime` after the gate. These artifacts
are diagnostic outputs only; the workflow must remain reproducible without PHP
reference downloads, Composer downloads, vendored projects, Miri downloads, or
sanitizer setup.
