# Lexer Architecture

The Phase 1 lexer is byte-oriented and lossless. PHP source is lexically
processed as bytes, with line and column used only as derived display or
reference-comparison information.

## Core Design

- `Cursor`: reads bytes, supports lookahead, and guarantees forward progress.
- `Lexer`: owns scanner mode state and emits tokens.
- `Token`: records token kind, byte span, start line, and diagnostics.
- `LexResult`: returns all tokens and diagnostics without panicking on invalid
  input.

Token text is not duplicated in the token. It is recovered from the original
source through the token byte span.

## Cursor Design

The lexer cursor is byte-based and does not decode UTF-8 while scanning. It
exposes:

- `peek()`: current byte.
- `peek_n(n)`: lookahead from the current byte offset.
- `bump()`: consume one byte and advance.
- `starts_with(bytes)`: prefix check from the current byte offset.
- `is_eof()`: end-of-source check.
- `position()`: current byte offset.

These operations are enough for deterministic longest-match token rules without
copying token text into tokens.

## Source Model

`php_source` provides the shared byte-position model:

- `BytePos` is a `usize` byte offset.
- `TextRange` is a half-open `[start, end)` byte range.
- `LineIndex` maps byte offsets to one-based `LineCol` values.
- `SourceText` stores source text and its line index.

Line and column values are one-based for PHP reference comparison. Columns are
byte columns; Unicode grapheme width is outside Phase 1.

## Crate Layout

Phase 1 introduces `crates/php_lexer` as the lexer library and
`crates/php_lexer_cli` as a small JSON output tool for differential testing.
The library depends on `php_source` for the source-layer boundary, but neither
crate defines parser, AST/CST, runtime, VM, JIT, extension, or Zend ABI types.

The initial crate modules are:

- `cursor.rs`: byte cursor primitives.
- `diagnostics.rs`: recoverable lexer diagnostics.
- `lexer.rs`: `Lexer`, `LexerConfig`, `LexResult`, and `lex_all`.
- `modes.rs`: scanner mode names required by PHP tokenization.
- `token.rs`: normalized token names and shared `php_source` byte spans.

`php_lexer_cli` exposes:

```bash
cargo run -p php_lexer_cli -- --file tests/fixtures/lexer/010-tags.php --pretty
```

The CLI emits `engine`, `tokens`, and `diagnostics` JSON fields. Token entries
include zero-based index, normalized kind, original text, start line, start byte,
and end byte. Diagnostics include stable IDs and byte spans.

## Scanner Modes

The lexer needs explicit modes. Internal names may differ, but the model must
cover:

- `InlineHtml`
- `Scripting`
- `DoubleQuote`
- `Backtick`
- `Heredoc`
- `Nowdoc`
- `StringVarOffset`
- `LookingForVarName`

## Matching Rules

Operators and casts use longest-match behavior. For example, `??=` must be
recognized before `??`, and `|>` must normalize as `T_PIPE`, not as `|` then
`>`.

Cast tokens such as `(void)` normalize as `T_VOID_CAST` when they match the
PHP reference behavior.

## Reference Strategy

Differential testing compares normalized Rust tokens against
`token_get_all($code, 0)`. Numeric PHP token values are never part of the Rust
compatibility model.

`TOKEN_PARSE` may be exposed as a future configuration flag, but full
equivalence is parser-contextual and belongs to Phase 2.

`just lexer-diff` and `just lexer-fixtures` both run strict comparison for the
curated fixtures. `scripts/compare-lexer-fixtures.py` still supports an explicit
allowlist option for future temporary exceptions, but Phase 1 does not accept
any curated fixture differences by default.

## Performance Baseline

`just bench-lexer` runs a simple stable benchmark harness over inline HTML,
simple PHP statements, and string/interpolation-heavy input. The numbers are
local baseline data only, not a compatibility or product performance promise.

Phase 1 intentionally avoids `unsafe`, SIMD, interning, arenas, and advanced
scanner table generation. Later optimization candidates include `memchr` for
tag/string searches, a compact operator DFA, and tighter token representation.

## Phase Boundary

The lexer emits tokens. It does not build grammar productions, expressions,
statements, AST nodes, CST nodes, VM opcodes, or runtime values.
