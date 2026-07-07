# ADR 0007: Lexer/Parser Boundary

## Status

Accepted

## Context

Some PHP tokenizer behavior is parser-contextual when `TOKEN_PARSE` is used.
Lexer still needs a reliable lexer without implementing grammar, AST, or
runtime behavior.

## Decision

Lexer implements lexer/tokenization behavior only. The hard compatibility
gate is `token_get_all($code, 0)`.

`TOKEN_PARSE` is prepared and documented, but full equivalence is deferred to
Parser, where parser context exists.

## Consequences

- The lexer may expose configuration for parser-aware modes later.
- The lexer must not decide expression or statement grammar.
- Parser/CST work starts in Parser using the Lexer token stream.

## Alternatives

- Implement `TOKEN_PARSE` parity in Lexer. Rejected because it requires
  parser context.
- Start parser and lexer together. Rejected because the Lexer deliverable
  should be independently testable against `token_get_all($code, 0)`.

## Date

2026-06-19
