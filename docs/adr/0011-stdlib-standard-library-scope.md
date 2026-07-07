# ADR 0011: Standard Library Scope

## Status

Accepted for the standard-library layer.

## Context

Runtime semantics produced a runtime capable of executing a meaningful PHP 8.5.7 subset,
but framework and Composer compatibility now depends on standard-library,
extension metadata, streams, SPL, Reflection, and diagnostics breadth.

## Decision

`php_std` and VM-owned builtins implement a deterministic offline MVP of PHP 8.5.7 standard-library
behavior. The required path covers core functions, JSON, PCRE, Date/Time, SPL,
Reflection, tokenizer, streams, filesystem, Composer-local autoloading, and
Composer platform checks.

## Consequences

The VM and runtime may receive small integration hooks, but Standard library does not
rewrite lexer, parser, HIR, IR, VM, or existing runtime contracts.
