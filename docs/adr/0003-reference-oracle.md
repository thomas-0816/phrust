# ADR 0003: Reference Oracle

## Status

Accepted

## Context

PHP language behavior is defined by implementation, tests, manuals, and
historical compatibility. A Rust implementation needs an executable and
source-level oracle before implementing syntax or runtime behavior.

## Decision

Use `php-src` at PHP `8.5.7`, tag `php-8.5.7`, as the primary reference oracle.

The oracle includes:

- Source files such as `Zend/zend_language_scanner.l`,
  `Zend/zend_language_parser.y`, `Zend/zend_vm_def.h`, compiler, AST, and type
  headers.
- The reference CLI behavior.
- `token_get_all()` for token compatibility.
- `php -l` for parser acceptance and parse-error behavior.
- `.phpt` tests for runtime behavior.

The PHP manual and RFCs are useful supporting documentation, but executable
reference behavior is the primary compatibility source.

Documentation can explain intent, but the pinned reference CLI and `.phpt`
behavior are authoritative for compatibility decisions.

## Consequences

- Later phases must compare behavior against the pinned reference.
- Reference metadata and hashes may be stored under `references/`.
- The full `php-src` checkout remains local under `third_party/` and is not
  committed.

## Alternatives

- Use only the PHP manual. Rejected because it is not precise enough for full
  compatibility.
- Use only a grammar file. Rejected because PHP syntax and diagnostics also
  depend on scanner states, compiler checks, and runtime behavior.

## Date

2026-06-19
