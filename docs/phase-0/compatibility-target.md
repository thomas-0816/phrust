# Compatibility Target

Phase 0 fixes the compatibility target to PHP `8.5.7`, Git tag
`php-8.5.7`, from:

```text
https://github.com/php/php-src.git
```

The `php-src` checkout is the primary compatibility oracle. It is cloned
locally under:

```text
third_party/php-src
```

The source tree itself is not committed. The resolved tag commit is stored in
`references/php-src.lock.toml` after running:

```bash
nix develop -c just bootstrap-ref
```

An optional minimal reference CLI can be built with:

```bash
nix develop -c just build-ref-php
```

The optional build enables CLI and tokenizer support so later phases can use
`token_get_all()` as a lexer oracle.

## Critical Reference Files

The Phase 0 reference contract tracks these critical files:

| File | Purpose |
| --- | --- |
| `Zend/zend_language_scanner.l` | Scanner rules, PHP/HTML modes, tokenization behavior. |
| `Zend/zend_language_parser.y` | Parser grammar and parser actions. |
| `Zend/zend_vm_def.h` | VM opcode definitions and execution semantics reference. |
| `Zend/zend_ast.h` | AST node kinds and AST-level structures. |
| `Zend/zend_compile.h` | Compiler interfaces and compile-time semantic hooks. |
| `Zend/zend_types.h` | Zend value and type definitions relevant to later runtime work. |

Manual and test references:

- `https://www.php.net/manual/en/tokens.php`
- `https://www.php.net/manual/en/langref.php`
- `.phpt` tests under `Zend/tests` and `tests`

## Update Policy

The target must not automatically move to a newer PHP patch version. Any
change from PHP `8.5.7` / tag `php-8.5.7` requires an ADR update that explains
why the compatibility target changed.

## Phase 0 Boundary

This document pins the reference. It does not define or implement a Rust PHP
lexer, parser, VM, or runtime.
