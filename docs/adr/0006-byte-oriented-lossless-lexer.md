# ADR 0006: Byte-Oriented Lossless Lexer

## Status

Accepted

## Context

PHP source tokenization is byte-sensitive. A lexer that indexes Unicode scalar
values or grapheme clusters would report spans that do not match PHP source
bytes and would make differential testing harder.

## Decision

The Phase 1 lexer is byte-oriented and lossless. Tokens record byte spans into
the original source and expose line information as derived comparison data.

## Consequences

- Token text is recovered from the original source span.
- Line/column display is derived from byte positions.
- Non-ASCII UTF-8 input is handled as bytes, not as semantic Unicode
  identifiers.
- Later parser work can build on stable token spans.

## Alternatives

- Character-oriented lexer. Rejected because PHP source compatibility is byte
  based.
- Token text copied into every token. Rejected because spans are enough and
  cheaper for Phase 1.

## Date

2026-06-19
