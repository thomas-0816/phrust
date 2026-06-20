# Token Coverage

Phase 1 treats PHP tokenizer constants as reference data, not Rust numeric
facts. Numeric `T_*` values may differ between PHP builds and must not be
hardcoded in lexer logic.

## Reference Dump

Use the pinned PHP CLI when available:

```bash
nix develop -c just dump-reference-tokens
```

The command emits JSON with:

- `php_version`
- `php_version_id`
- `generated_at`
- name-sorted `tokens`

## Reference Tokenization

Use `token_get_all($code, 0)` for the hard Phase 1 oracle:

```bash
nix develop -c just tokenize-ref tests/fixtures/lexer/example.php
```

The normalized stream records each token as:

- `index`: zero-based token index.
- `kind`: `T_*` name or a single-character symbol.
- `text`: original token text.
- `line`: one-based start line.

`--token-parse` is supported by `scripts/tokenize-reference.php`, but it is not
the Phase 1 hard gate because it can depend on parser context.

Invalid UTF-8 bytes are substituted by the PHP oracle script during JSON
encoding so the JSON remains readable. Byte-exact invalid input coverage must
be documented when such fixtures are added.

## Generated Coverage Matrix

Generate the fixture coverage matrix with:

```bash
nix develop -c scripts/collect-lexer-fixtures.py
```

The script writes `docs/phase-1/token-coverage.generated.md` from the pinned PHP
reference oracle. If no reference PHP binary is available, it reports a skip and
does not claim coverage was computed.

The generated matrix records where each PHP 8.5 `T_*` token appears in lexer
fixtures. It is coverage evidence for fixture inputs, not proof that every token
is fully implemented by the Rust lexer. Tokens marked `not-yet-covered`,
`parser-contextual`, `extension/config-dependent`, or `deprecated/alias` remain
documented work rather than hard Phase 1 gates.

## Covered In Rust

The lexer currently has direct Rust coverage for:

- `T_WHITESPACE`
- `T_COMMENT` for `//`, `#`, and block comments
- `T_DOC_COMMENT`
- `T_BAD_CHARACTER`
- `T_OPEN_TAG`, `T_OPEN_TAG_WITH_ECHO`, `T_CLOSE_TAG`, and `T_INLINE_HTML`
- `T_STRING`, `T_VARIABLE`, common keyword tokens, magic constants, and PHP 8
  namespace-name tokens
- `T_LNUMBER` and `T_DNUMBER` for decimal, hex, binary, octal, explicit octal,
  separators, decimal-point floats, and exponent floats
- longest-match scripting operators and assignment operators including
  comparisons, shifts, coalesce, nullsafe/object access, double colon, double
  arrow, increment/decrement, ellipsis, exponentiation, PHP 8.5 pipe, and
  attributes
- cast tokens for integer, float/double/real, string, array, object,
  bool/boolean, unset, and void casts with internal whitespace
- contextual ampersand tokens and PHP property-hook visibility tokens present in
  the pinned PHP 8.5 reference build
- `T_CONSTANT_ENCAPSED_STRING` for single-quoted strings and double-quoted
  strings without interpolation, with spans including delimiters
- encapsed double-quoted and backtick strings for simple interpolation:
  `T_ENCAPSED_AND_WHITESPACE`, `T_VARIABLE`, `T_OBJECT_OPERATOR`, `T_STRING`,
  `T_NUM_STRING`, `T_CURLY_OPEN`, `T_DOLLAR_OPEN_CURLY_BRACES`, and
  `T_STRING_VARNAME`
- `T_START_HEREDOC` and `T_END_HEREDOC` for basic heredoc and nowdoc labels,
  including indented closing labels and semicolon separation

`?>` inside a line comment ends the comment before the close tag, matching the
reference tokenizer behavior observed in the pinned PHP CLI.

Keyword matching is ASCII case-insensitive for PHP keywords. Non-ASCII bytes
are accepted in identifier tokens as bytes; columns and spans remain
byte-oriented. `TOKEN_PARSE` keyword relaxation is documented as Phase 2 work.

Numeric lexing uses longest-match token-boundary rules without evaluating
values. Invalid forms observed against the reference, such as `1e`, `0x`,
`0b2`, and `1__2`, split into the same lexical prefixes rather than producing
runtime errors in the lexer.

Operator lexing is ordered by longest byte match before falling back to
single-byte symbol tokens. `#[` is recognized before hash comments so attributes
do not get swallowed by line-comment scanning.

Encapsed string support is intentionally lexical and limited to simple
interpolation surfaces. Complex expressions inside interpolation are not parsed
or normalized beyond the tokens listed above.

Heredoc support uses the same simple interpolation surface as double-quoted
strings. Nowdoc bodies do not interpolate variables.
