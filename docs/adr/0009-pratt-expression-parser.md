# ADR 0009: Pratt Expression Parser

## Status

Accepted

## Context

PHP expressions include many precedence groups, postfix chains, assignment
forms, ternary/elvis, coalesce, yield, and PHP 8.5 additions such as pipe
syntax. Encoding expression precedence as deeply nested recursive functions is
hard to maintain and easy to make inconsistent.

## Decision

Expression parsing will use a Pratt/precedence-climbing parser. Prefix,
postfix, infix, and special forms are modeled with explicit binding powers.

The initial implementation covers primary expressions, prefix expressions, and
the common binary operator table documented in
`docs/phase-2/expression-precedence.md`. `**` is right-associative. Simple
assignment is parsed as a low-precedence right-associative expression so valid
statement fixtures remain accepted; compound assignment and ternary are the next
extension points.

## Consequences

- Precedence is centralized and reviewable.
- Expression parsing can grow incrementally across syntax forms.
- Special forms such as ternary and yield still need dedicated handling where
  PHP grammar requires it.
- The parser can stop expression parsing at caller-provided recovery sets,
  allowing statement recovery to remain bounded.
