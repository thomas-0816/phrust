# Token Model

Phase 1 tokens are normalized for comparison with PHP `8.5.7`
`token_get_all($code, 0)`.

## Rust Model

The intended API shape is:

```text
Token {
  kind: TokenKind,
  range: TextRange,
  line: u32,
}

TokenKind {
  Named(TokenName),
  Symbol(SymbolKind),
  Eof,
}

TokenName {
  InlineHtml,
  OpenTag,
  OpenTagWithEcho,
  CloseTag,
  Whitespace,
  Comment,
  DocComment,
  BadCharacter,
  String,
  If,
  Else,
  Function,
  Class,
  Interface,
  Trait,
  Enum,
  Namespace,
  Use,
  Match,
  Readonly,
  Fn,
  Yield,
  YieldFrom,
  Echo,
  Return,
  Extends,
  Implements,
  Variable,
  NameFullyQualified,
  NameQualified,
  NameRelative,
  LNumber,
  DNumber,
  ConstantEncapsedString,
  EncapsedAndWhitespace,
  StartHeredoc,
  EndHeredoc,
  Line,
  File,
  Dir,
  ClassC,
  TraitC,
  MethodC,
  FuncC,
  NamespaceC,
  PropertyC,
  Pipe,
  VoidCast,
}

SymbolKind {
  Char(u8),
}
```

Token text is recovered through the source span. The token does not own a copy
of its text. `Token::text(source)` returns the source slice when the range is
valid for the supplied source.

## Span Model

`php_source` owns the shared source primitives used by the lexer:

- `BytePos`: a `usize` byte offset in the original source.
- `TextRange`: a half-open byte range `[start, end)`.
- `LineCol`: a one-based display line and one-based byte column.
- `LineIndex`: maps byte positions to `LineCol`.
- `SourceText`: an owned source string plus its `LineIndex`.

`usize` is used for byte positions because Rust slices and strings are indexed
with `usize`. Columns are byte columns. The project does not compute Unicode
grapheme columns for PHP compatibility checks.

`TextRange::new(start, end)` always returns a valid range by clamping invalid
ordering to an empty range at `start`. Callers that need to reject invalid
bounds can use `TextRange::try_new(start, end)`.

## Reference Normalization

`token_get_all()` returns:

- Three-item arrays for named `T_*` tokens: token number, text, line.
- Strings for single-character symbol tokens.

The project normalizes both forms into:

```json
{
  "kind": "T_OPEN_TAG",
  "text": "<?php ",
  "line": 1
}
```

or:

```json
{
  "kind": ";",
  "text": ";",
  "line": 1
}
```

Numeric token values are not stable and must not be hardcoded in Rust logic.
`TokenKind::reference_name()` is the stable comparison surface for both named
tokens and symbol tokens.

The reference oracle scripts are:

- `scripts/dump-reference-tokens.php`: dumps the current PHP build's `T_*`
  constants as JSON for inspection.
- `scripts/tokenize-reference.php`: normalizes `token_get_all($code, 0)` or
  optional `TOKEN_PARSE` output into JSON.

Rust test utilities read these JSON formats through
`php_testkit::lexer_reference`. The lexer crate does not spawn PHP processes.

## Required PHP 8.5 Surface

The Phase 1 token surface must explicitly include:

- `T_PIPE`
- `T_VOID_CAST`

It must also include normal lexer tokens such as `T_INLINE_HTML`,
`T_OPEN_TAG`, `T_OPEN_TAG_WITH_ECHO`, `T_CLOSE_TAG`, `T_WHITESPACE`,
`T_COMMENT`, `T_DOC_COMMENT`, `T_STRING`, `T_VARIABLE`, `T_LNUMBER`,
`T_DNUMBER`, `T_CONSTANT_ENCAPSED_STRING`, `T_ENCAPSED_AND_WHITESPACE`,
`T_START_HEREDOC`, and `T_END_HEREDOC`.

The scripting word surface includes keyword and magic-constant names such as
`T_IF`, `T_ELSE`, `T_FUNCTION`, `T_CLASS`, `T_NAMESPACE`, `T_USE`, `T_MATCH`,
`T_ENUM`, `T_READONLY`, `T_FN`, `T_YIELD`, `T_YIELD_FROM`, `T_LINE`,
`T_FILE`, `T_DIR`, `T_CLASS_C`, `T_TRAIT_C`, `T_METHOD_C`, `T_FUNC_C`,
`T_NAMESPACE_C`, and PHP 8.5.7 `T_PROPERTY_C`.

Namespace names are lexical tokens where PHP 8 reports them as
`T_NAME_FULLY_QUALIFIED`, `T_NAME_QUALIFIED`, or `T_NAME_RELATIVE`.

## TOKEN_PARSE Boundary

`TOKEN_PARSE` can change tokenization for parser-contextual cases. Phase 1
prepares the interface but only hard-gates `token_get_all($code, 0)`.
Reserved words that become `T_STRING` only under `TOKEN_PARSE` are Phase 2
parser-context work, not Phase 1 hard compatibility.
