# Repository Truth Ratchet

This repository treats source layout, generated metadata, and public facade
guidance as executable contracts. `nix develop -c just source-integrity` is the
cheap gate for those contracts.

The source-integrity gate checks:

- Required VM module files exist and are non-empty.
- `crates/php_vm/src/vm/mod.rs` declares every sibling VM submodule exactly once.
- VM submodules use the shared prelude instead of broad `super::*` imports.
- Generated stdlib arginfo is present and consumed by `php_std`.
- New downstream imports use `php_vm::api`, `php_runtime::api`,
  `php_runtime::debug`, or `php_runtime::experimental` unless a scoped allowlist
  entry documents the reason.
- Architecture drift scripts for dependency boundaries and panic/unwrap policy
  are available and run through the same local validation flow.

When a check fails, update the implementation or add a narrow policy exception
with an owner-facing reason. Do not silence failures by broadening public
surfaces or committing generated files from `target/`.
