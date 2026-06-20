# CST Model

The CST is a concrete tree over the original source text. It is not a semantic
AST and must not discard source bytes.

## Elements

```text
SyntaxKind
  Token(SyntaxTokenKind)
  Node(SyntaxNodeKind)

SyntaxNode
  green: GreenNode

SyntaxElement
  Node(SyntaxNode)
  Token(SyntaxToken)

SyntaxToken
  green: GreenToken

GreenNode
  kind: SyntaxKind
  range: TextRange
  children: SyntaxElement[]

GreenToken
  kind: SyntaxKind
  text: original source slice
  range: TextRange
  line: one-based start line

ParseDiagnostic
  id: stable diagnostic id
  message: human-readable message
  span: TextRange
  expected: stable expected-token or expected-syntax set
  recovery: recovery strategy
  severity: error or parser note
```

`SyntaxKind` is intentionally split into token and node sub-kinds. Token kinds
map directly from `php_lexer::TokenKind`: named `T_*` tokens stay as named PHP
tokens, single-byte punctuators stay as symbol tokens, and optional EOF remains
a synthetic token. Node kinds are parser-owned structural categories such as
`SOURCE_FILE`, `STATEMENT_LIST`, `CLASS_DECL`, `BINARY_EXPR`, `PIPE_EXPR`, and
`HEREDOC`.

Some token families remain generic by design. Single-byte punctuators are kept
as symbol tokens instead of one Rust enum variant per byte. PHP named tokens are
kept as lexer `TokenName` values, which prevents hardcoding numeric PHP token
IDs while still exposing stable `T_*` names.

## Spans

Byte ranges are primary. Line and column values are derived from `php_source`
line-index utilities for display, diagnostics, and future editor integrations.

## Roundtrip Requirement

Every token leaf stores or references its exact source slice. Reconstructing the
file by concatenating all token leaves must produce the original source text.

```text
parse(source).reconstructed_text() == source
```

This includes inline HTML, PHP tags, comments, whitespace, string contents,
heredoc bodies, invalid tokens, and error-recovered regions.

The command-line parser exposes this as:

```bash
cargo run -p php_parser_cli -- --roundtrip-check path/to/file.php
```

All committed parser fixtures are checked by:

```bash
nix develop -c just cst-roundtrip
```

`Parse::debug_tree()` prints a deterministic tree shape intended for small
snapshot tests. It includes syntax kind names, byte ranges, one-based token
start lines, and escaped token text. It does not normalize newlines, encodings,
or whitespace.

## Snapshots

Parser snapshots use checked-in `.snap` files rather than an external snapshot
dependency. The integration tests under `crates/php_syntax/tests/` render:

- CST debug trees,
- diagnostic IDs, byte spans, messages, and expected syntax sets,
- roundtrip status.

Snapshot content uses fixture-relative paths only, so machine-local checkout
paths do not appear in test output. Update snapshots with:

```bash
nix develop -c just parser-snapshots
```

The update command sets `UPDATE_PARSER_SNAPSHOTS=1`, rewrites the snapshot
files, and reruns the snapshot assertions. Normal `cargo test` runs compare
against checked-in snapshots and fail on drift.

## Diagnostics and Error Nodes

Invalid input must produce diagnostics and CST error nodes where possible. The
parser must make progress during recovery and must not panic on malformed input.
