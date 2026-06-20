# Phase 1 Final Audit

- Date: 2026-06-20
- Reference target: PHP `8.5.7` / tag `php-8.5.7`
- Reference commit: `35eab8c08bc590758d05813b0ff7a3d8c3e67b79`

## Hard Checks

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo test -p php_lexer lexer_invariants`
- `just verify-phase1`

## Reference-Dependent Checks

- `just lexer-fixtures` runs when `REFERENCE_PHP` or
  `third_party/php-src/sapi/cli/php` is available.
- The check compares Rust CLI JSON with `token_get_all($code, 0)` without
  accepting curated fixture differences.
- `just lexer-diff` is the same strict parity command.
- `just lexer-corpus-smoke` is optional and extracts php-src `.phpt` `--FILE--`
  sections into `target/php-src-lexer-corpus/`.

## Known Differences

See `docs/phase-1/known-lexer-differences.md`. No curated fixture differences
are currently accepted.

## Coverage Gaps

The generated matrix currently records uncovered or non-gated token constants:

- `T_BAD_CHARACTER` is covered by Rust unit tests but not by byte-exact fixture
  matrix input.
- `T_FMT`
- `T_FMT_AMPM`
- `T_NS_SEPARATOR`
- `T_PAAMAYIM_NEKUDOTAYIM` is a deprecated alias.

## Phase Boundary

Phase 1 introduced lexer/tokenization code only. It does not introduce a parser,
AST/CST lowering, VM, runtime values, JIT, extensions, or Zend ABI emulation.
