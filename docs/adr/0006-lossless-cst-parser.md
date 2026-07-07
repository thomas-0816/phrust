# ADR 0006: Lossless CST Parser

## Status

Accepted

## Context

The engine needs a parser that supports PHP syntax while preserving the exact
source text for diagnostics, tooling, and future transformations. PHP files can
mix inline HTML, PHP tags, comments, whitespace, strings, heredoc bodies, and
malformed regions that still need useful recovery.

## Decision

The parser will produce a lossless concrete syntax tree. CST tokens retain exact
source slices and byte ranges. Concatenating token leaves must reconstruct the
original source text.

The parser consumes tokens from the existing lexer and does not implement a
second lexer.

The implementation uses a small in-repository green-tree structure instead of
`rowan` for now. The current parser needs a compact immutable node/token store,
stable debug output, and exact token text retention, but does not yet need
incremental reparsing, typed syntax wrappers, or shared arena interning.

Nodes and tokens expose byte `TextRange` values. Parse results may carry
optional caller-owned source identity metadata, but the parser does not own a
global file table or singleton source registry. Each parse produces an
independent CST.

## Consequences

- CST nodes may be more verbose than semantic AST nodes.
- Formatting trivia, comments, PHP tags, and inline HTML remain available.
- Later semantic layers must build typed views or lowered forms over the CST
  instead of expecting the parser tree to be semantic.
- If editor-grade incremental parsing becomes a requirement, this decision can
  be revisited and migrated to `rowan` behind the same public CST surface.
- Future incremental parsing should preserve the current lossless surface and
  may add stable node identity around statement, class-member, and function-body
  boundaries without changing the parser into a semantic AST builder.
