# Parser and CST Definition of Done

This milestone adds only the PHP parser and lossless concrete syntax tree
(CST). The fixed reference target remains PHP 8.5.7, tag `php-8.5.7`.

## In Scope

- A Rust parser crate that consumes `php_lexer` tokens.
- A diagnostic CLI for parsing PHP source files.
- A lossless CST that preserves tokens, trivia, inline HTML, PHP tags, string
  structure, and byte spans.
- Error recovery that produces diagnostics and error nodes instead of panics.
- Differential acceptance checks against the reference PHP CLI using `php -l`.
- Curated parser fixtures covering PHP files, statements, expressions,
  declarations, classes, traits, interfaces, enums, attributes, types, strings,
  heredoc/nowdoc, and PHP 8.5 syntax forms.

## Out of Scope

- Name resolution.
- Compile-time semantic checks.
- AST/HIR lowering as a semantic layer.
- Bytecode, IR, VM execution, runtime values, JIT, extensions, or Zend ABI
  emulation.
- Exact matching of PHP parse error text.

## Required Gates

The work is complete only after a central verification command exists and local
checks pass:

```bash
nix develop -c just verify-phase2
```

The verification target must preserve the earlier foundation and lexer checks,
run Rust formatting, clippy, and workspace tests, and include parser-specific
fixture/diff/roundtrip checks once they are available.

`just verify-phase2` currently runs:

- `just verify-phase0`
- `just verify-phase1`
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `just parser-diff`
- `just cst-roundtrip`

Additional focused parser gates:

- `just parser-fixtures`
- `just fuzz-parser-smoke`
- `just bench-parser`

The shorter `just verify` alias points at the same central gate for local use.

## Reference Contract

The primary syntax acceptance oracle is:

```bash
REFERENCE_PHP=third_party/php-src/sapi/cli/php php -l file.php
```

If `REFERENCE_PHP` is explicitly set, reference-dependent parser checks are
strict. If no reference binary is available, those checks must skip with a clear
reason and local Rust checks must still run.

CI must not rely on a checked-in `third_party/php-src`. Reference-dependent
checks skip when no local reference or `REFERENCE_PHP` is available. If CI later
builds a reference PHP binary, that build should be optional and cached rather
than a prerequisite for Rust formatting, linting, and tests.
