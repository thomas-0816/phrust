# ADR 0031: Token Oracle Normalization

## Status

Accepted

## Context

PHP exposes tokenizer constants as numeric values, but those values are not the
right compatibility surface. The stable comparison target for this project is
the token name and original token text.

## Decision

Normalize reference tokens from `token_get_all($code, 0)` into JSON using
token names such as `T_OPEN_TAG`, `T_VARIABLE`, and `T_PIPE`. Single-character
tokens are normalized to their character string.

## Consequences

- Rust logic must not hardcode PHP numeric token values.
- Differential tests compare token kind, text, and line.
- `TOKEN_PARSE` is documented and prepared, but strict parser-contextual parity
  is deferred to Parser.

## Alternatives

- Compare numeric token values. Rejected because numeric values are an
  implementation detail.
- Compare only token text. Rejected because token names distinguish PHP lexical
  categories.

## Date

2026-06-19
