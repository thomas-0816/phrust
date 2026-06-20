# ADR 0008: Lexer Parser Boundary

## Status

Accepted

## Context

Some PHP tokenizer behavior is parser-contextual when `TOKEN_PARSE` is used.
Phase 1 still needs a reliable lexer without implementing grammar, AST, or
runtime behavior.

## Decision

Phase 1 implements lexer/tokenization behavior only. The hard compatibility
gate is `token_get_all($code, 0)`.

`TOKEN_PARSE` is prepared and documented, but full equivalence is deferred to
Phase 2, where parser context exists.

## Consequences

- The lexer may expose configuration for parser-aware modes later.
- The lexer must not decide expression or statement grammar.
- Parser/CST work starts in Phase 2 using the Phase 1 token stream.

## Alternatives

- Implement `TOKEN_PARSE` parity in Phase 1. Rejected because it requires
  parser context.
- Start parser and lexer together. Rejected because the Phase 1 deliverable
  should be independently testable against `token_get_all($code, 0)`.

## Date

2026-06-19
