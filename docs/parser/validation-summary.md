# Parser and CST Validation Summary

The parser/CST layer owns PHP syntax parsing over the existing lexer token
stream. It provides a lossless CST, parser diagnostics, fixture comparison
against the pinned PHP 8.5.7 reference, and CST roundtrip checks.

## Current Scope

- `crates/php_syntax` consumes `php_lexer` tokens through `TokenSource`.
- `php_parser_cli` exposes parser diagnostics and CST output for local
  inspection.
- Parser fixtures compare PHP lint acceptance against the Rust parser.
- CST roundtrip checks reconstruct committed parser fixtures from CST tokens.
- Parser diagnostics stay separate from semantic diagnostics.

The parser layer does not implement AST/HIR lowering, name resolution,
compile-time semantic validation, runtime values, VM execution, JIT behavior,
extensions, or Zend ABI emulation.

## Validation

Use the parser/frontend gate when changing parser or CST behavior:

```bash
nix develop -c just verify-frontend
```

Focused checks include:

```bash
nix develop -c just parser-fixtures
nix develop -c just parser-diff
nix develop -c just cst-roundtrip
```

Optional php-src corpus smoke checks are exploratory. Any syntax issue found
there should be reduced into a committed fixture before it becomes part of the
strict parser contract.

## Current Gaps

- Complex interpolation internals remain shallow in selected CST shapes, while
  the parser still preserves and groups encapsed string and heredoc tokens
  losslessly.
- Incremental reparsing has byte ranges and source identity support, but no
  stable node identity or subtree reuse implementation.

The authoritative parser gap list is
`docs/parser/parser-known-gaps.md`.
