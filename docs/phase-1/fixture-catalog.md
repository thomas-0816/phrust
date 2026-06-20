# Fixture Catalog

Phase 1 lexer fixtures live in `tests/fixtures/lexer`.

## Fixtures

- `000-inline-html.php`: inline HTML before a PHP open tag and after a close
  tag, used to verify HTML/PHP mode switching.
- `010-tags.php`: normal `<?php ... ?>` tags and `<?= ... ?>` echo tags,
  including reference behavior where close tags consume a following newline.
- `020-comments-whitespace.php`: spaces, tabs, LF, CRLF, line comments, hash
  comments, block comments, and doc comments.
- `030-keywords-names-vars.php`: keywords, interfaces, traits, enums,
  namespaces, fully qualified names, variables, variable variables, magic
  constants, `yield from`, `fn`, `match`, and `readonly`.
- `040-numbers.php`: decimal integers, prefixed integers, explicit octal,
  numeric separators, decimal-point floats, exponent floats, and invalid
  boundary cases.
- `050-operators-casts.php`: longest-match operators, casts, attributes,
  contextual ampersands, pipe, ellipsis, and logical word operators.
- `060-strings-basic.php`: single-quoted strings, non-interpolated
  double-quoted strings, escaped delimiters, escaped backslashes, multiline
  constant strings, and a deferred interpolated double-quoted string.
- `070-encapsed-strings.php`: double-quoted and backtick encapsed strings with
  simple variables, object property access, numeric offsets, braced variables,
  dollar-open curly variables, and escaped non-interpolation sequences.
- `080-heredoc-nowdoc.php`: heredoc content, heredoc variable interpolation,
  nowdoc non-interpolation, indented end markers, and semicolon separation.
- `090-php85-tokens.php`: PHP 8.5-specific coverage for pipe,
  `__PROPERTY__`, and property-hook visibility tokens present in the pinned
  reference build.
- `100-token-surface.php`: compact PHP token-surface fixture covering broad
  keyword, declaration, alternative-control-flow, include, trait adaptation,
  enum, and halt-compiler tokens without requiring semantic execution.
- `980-invalid-block-comment.php`: unterminated block comment recovery.
- `981-invalid-encapsed-string.php`: unterminated encapsed string recovery.
- `999-invalid-recovery.php`: invalid/recovery-oriented fixture for
  unterminated heredoc recovery.

## Harness Status

`scripts/compare-lexer-fixtures.py` tokenizes every fixture with the PHP
reference oracle and the Rust `php_lexer_cli` JSON output.

Commands:

- `just lexer-diff`: strict token count/kind/text/line comparison.
- `just lexer-fixtures`: same strict comparison used by the Phase 1 gate.
- `just lexer-diff-report`: writes `target/lexer-diff-report.json`.

No curated fixture differences are currently accepted. Future temporary
differences must be recorded in `docs/phase-1/known-lexer-differences.md` and
passed through an explicit comparator allowlist option.

## Optional php-src Corpus

`just lexer-corpus-smoke` extracts `--FILE--` sections from the local
`third_party/php-src` checkout into `target/php-src-lexer-corpus/` and runs the
Rust lexer CLI over a small sample. It does not commit extracted `.phpt`
content, and it skips cleanly when the local reference checkout is absent.

Short-open-tag behavior such as `<? echo 1; ?>` is intentionally not a hard
fixture yet. It belongs behind `LexerConfig.short_open_tag`.
