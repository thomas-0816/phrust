# tokenizer PHPT coverage

## Implemented slice

- Registers the `tokenizer` extension, `token_get_all`, `token_name`,
  `PhpToken`, `TOKEN_PARSE`, and PHP token constants through `php_std`.
- Executes `token_get_all` and `token_name` through the runtime builtin
  registry using the existing `php_lexer` tokenizer mapping.
- Preserves PHP-visible token array shape for selected tokens:
  `[token_id, token_text, line_number]`.
- Covers open tags, whitespace, echo, variables, integers, punctuation, close
  tags, inline HTML, selected PHP 8.5 tokens, `TOKEN_PARSE`, and an upstream
  invalid-octal overflow fixture.
- Promotes selected upstream `ext/tokenizer` `token_get_all` basic and
  variation rows that pass with the current lexer/token mapping.
- Promotes upstream `PhpToken_constructor.phpt` for constructor public
  properties, `getTokenName()` nullability, `Stringable` instance checks, and
  ignorable open-tag classification.
- Promotes upstream `PhpToken_methods.phpt` and `PhpToken_toString.phpt` for
  PHP-compatible token-name display, `is()` matching/error behavior, typed
  property access errors, and string casting through `__toString()`.

## Known gaps

- The selected fixtures do not prove the full upstream ext/tokenizer PHPT
  corpus yet.
- Numeric token IDs are intentionally engine-owned and compared by names/text,
  not hardcoded Zend token values.
- Parser-internal token names are not exposed.
- Remaining upstream failures cluster around `PhpToken` subclass/finality and
  extension behavior, `PhpToken::tokenize()` `EXPECTF` coverage, legacy token
  aliases such as `T_PAAMAYIM_NEKUDOTAYIM`,
  bad-character token emission, heredoc recovery, and `TOKEN_PARSE`
  context-sensitive keyword reclassification.

## Gates

- `nix develop -c cargo test -p php_runtime tokenizer --no-fail-fast`
- `nix develop -c cargo test -p php_std tokenizer --no-fail-fast`
- `REFERENCE_PHP=$REFERENCE_PHP PHP_SRC_DIR=$PHP_SRC_DIR PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=tokenizer`

Last tokenizer module gate after this promotion: 38 selected PASS, 20 known
failures remain.
