# ADR 0010: Syntax and Semantics Boundary

## Status

Accepted

## Context

Many PHP validity rules are not pure syntax. Examples include duplicate
parameters, abstract method constraints, attribute target rules, name
resolution, constant expression validation, and type compatibility.

## Decision

The parser accepts syntactically valid forms and records syntax diagnostics. It
does not perform semantic lowering, name resolution, compile-time semantic
checks, VM execution, or runtime value evaluation.

Semantic layers may reject programs that the parser accepts.

The lexer/parser boundary stays lexical-first. The lexer emits the token names
it can determine without parser context. The parser does not ask the lexer to
run PHP's `TOKEN_PARSE` mode or to rebuild tokens. Instead, contextual
identifier positions can consult `TokenSource::current_keyword_context()` and
decide locally whether a reserved/contextual keyword token is accepted as a
name. Examples include member names after `->` and `::`; class declarations,
function declarations, namespace declarations, and type grammar keep their own
syntax-specific expectations.

## Consequences

- Parser behavior stays aligned with `php -l` acceptance where the reference
  classifies errors as syntax errors.
- Later layers can evolve without destabilizing CST construction.
- Known gaps must distinguish syntax mismatches from semantic checks.
- No numeric PHP token values are introduced at the syntax/semantics boundary.
