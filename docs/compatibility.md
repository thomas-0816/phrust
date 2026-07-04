# Compatibility

Phrust targets PHP 8.5.7 behavior where implemented.

## Reference Target

- PHP series: `8.5`
- PHP version: `8.5.7`
- Git tag: `php-8.5.7`
- Repository: `https://github.com/php/php-src.git`

The target version is fixed by ADR and should not change without a new ADR.

## Implemented Surfaces

The repository currently contains:

- byte-oriented source maps and spans;
- PHP lexer/tokenization;
- lossless parser and CST;
- typed AST views;
- semantic frontend, HIR, symbols, and diagnostics;
- bytecode/IR boundary;
- runtime values and builtins;
- interpreter VM;
- PHP-compatible `phrust-php` CLI and developer `php-vm` CLI;
- integrated HTTP server;
- PHPT indexing, execution, and reporting tools.

See [PHP user interface matrix](php-user-interface-matrix.md) and
[Switching from PHP](switching-from-php.md) for the current command-line and
built-in-server surfaces.

## Explicit Non-Goals

Phrust currently does not provide:

- Zend extension ABI compatibility;
- FPM, FastCGI, CGI, Apache module, or phpdbg support;
- a production SAPI;
- an Opcache replacement;
- a production JIT;
- full PHP standard library or extension parity.

## Known Gaps

Known gaps are documented explicitly so unsupported behavior is not mistaken for
implemented compatibility.

- [Runtime known gaps](runtime-known-gaps.md)
- [Runtime semantics known gaps](runtime-semantics-known-gaps.md)
- [Standard library known gaps](stdlib-known-gaps.md)
- [Server known gaps](server-known-gaps.md)
- [Performance known gaps](performance-known-gaps.md)
- [PHPT known gaps](phpt/known-gaps.md)
- [Known-gap manifests](known_gaps/README.md)

Reference-dependent checks skip clearly when no reference PHP binary is
available. If `REFERENCE_PHP` is set explicitly, the check is strict.
