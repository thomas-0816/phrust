# Phase 1 Definition of Done

Phase 1 builds only the PHP lexer/tokenization layer for the pinned PHP
`8.5.7` reference. It does not implement a parser, AST/CST lowering, VM,
runtime values, JIT, extensions, or Zend ABI emulation.

## Required Outcomes

- A `php_lexer` Rust crate exposes a byte-oriented lexer API.
- Tokens preserve kind, original byte span, start line, and diagnostics.
- Token kinds normalize to PHP reference token names such as `T_OPEN_TAG`,
  `T_VARIABLE`, and `T_PIPE`, never numeric token values.
- Curated fixtures compare against `token_get_all($code, 0)`.
- `TOKEN_PARSE` support is prepared and documented, but not a hard Phase 1
  compatibility gate because it depends on parser context.
- Reference-dependent checks skip clearly when no PHP 8.5.7 reference binary is
  available.
- Diagnostics are emitted for invalid input without panics or infinite loops.
- `T_PIPE` and `T_VOID_CAST` are explicit token surface requirements.
- `php_lexer_cli` emits normalized Rust lexer JSON for comparison tooling.
- Strict fixture diffing exists for curated fixtures.
- Lightweight invariant tests and an optional php-src corpus smoke are
  available.

## Hard Gate

The central Phase 1 validation command is:

```bash
nix develop -c just verify-phase1
```

It must run:

- `nix develop -c just verify-phase0`
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo test -p php_lexer lexer_invariants`

`just test-lexer` runs the focused crate check:

```bash
nix develop -c just test-lexer
```

At the workspace-structure stage, `php_lexer::lex_all` is allowed to use
documented placeholder behavior: no tokens for empty input, or a single
`T_INLINE_HTML` token spanning the full source for non-empty input. Later Phase
1 steps replace that behavior with reference-compatible tokenization.

## Reference Gate

When a reference PHP binary is available through `REFERENCE_PHP` or
`third_party/php-src/sapi/cli/php`, Phase 1 also runs:

```bash
nix develop -c just lexer-fixtures
```

`just lexer-fixtures` and `just lexer-diff` compare against
`token_get_all($code, 0)` without accepting curated fixture differences.

## Non-Goals

- No parser.
- No AST or CST lowering.
- No expression or statement grammar.
- No VM.
- No runtime value model.
- No JIT.
- No extensions.
