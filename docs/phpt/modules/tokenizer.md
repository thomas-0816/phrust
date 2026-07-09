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
- Registers the legacy `T_PAAMAYIM_NEKUDOTAYIM` constant alias with the same
  engine-owned tokenizer ID as `T_DOUBLE_COLON`, so `token_name()` returns
  `T_DOUBLE_COLON` for both names.
- Registers `T_NS_SEPARATOR` and preserves standalone namespace separators
  while keeping grouped qualified, fully qualified, and relative name tokens.
- Returns `T_BAD_CHARACTER` tokens for unexpected control characters instead
  of failing the tokenizer call.
- Reclassifies `TOKEN_PARSE` semi-reserved words to `T_STRING` for selected
  PHP-compatible member-access names, class constant names, and trait
  `namespace as` aliases.
- Validates `TOKEN_PARSE` source through `php_syntax` and reports syntax
  diagnostics as catchable `ParseError` instances for selected tokenizer rows.
- Validates selected `TOKEN_PARSE` legacy-octal numeric literals and unicode
  codepoint escapes before token reclassification, matching PHP-visible
  `ParseError` messages for invalid numeric literals, malformed `\u{...}`
  bodies, and too-large codepoints.
- Validates selected `TOKEN_PARSE` heredoc/nowdoc failures, including
  unterminated literals, mixed indentation, underindented bodies, and
  catchable `ParseError::getLine()` reporting against the tokenized string.
- Recovers unterminated heredoc/nowdoc lexer diagnostics without
  `TOKEN_PARSE`, so `token_get_all()` can return PHP-compatible partial token
  streams for selected upstream cases.
- Strips flexible nowdoc/heredoc indentation while lowering fixture literal
  bodies before those strings are passed into tokenizer builtins.
- Recognizes heredoc and nowdoc end markers immediately before expression
  punctuation so tokenizer PHPT fixture bodies such as `PHP));` execute through
  the runtime parser without splitting inline HTML incorrectly.
- Promotes selected upstream `ext/tokenizer` `token_get_all` basic and
  variation rows that pass with the current lexer/token mapping.
- Promotes upstream `PhpToken_constructor.phpt` for constructor public
  properties, `getTokenName()` nullability, `Stringable` instance checks, and
  ignorable open-tag classification.
- Promotes upstream `PhpToken_methods.phpt` and `PhpToken_toString.phpt` for
  PHP-compatible token-name display, `is()` matching/error behavior, typed
  property access errors, string casting through `__toString()`, and direct
  runtime method dispatch on `PhpToken::tokenize()` results.
- Promotes upstream `PhpToken_extension.phpt` for subclass
  `PhpToken::tokenize()` allocation, inherited public shape, userland method
  dispatch, and subclass default property initialization.
- Promotes upstream `PhpToken_extension_errors.phpt` for catchable
  `Error` behavior when subclass token allocation hits an undefined property
  default constant or an abstract `PhpToken` subclass.
- Promotes upstream `PhpToken_final_constructor.phpt` for PHP-compatible
  final-constructor override diagnostics on `PhpToken` subclasses.
- Promotes upstream `namespaced_names.phpt` for `PhpToken::tokenize()`
  namespace-name grouping and standalone `T_NS_SEPARATOR` output.
- Promotes upstream `bad_character.phpt` for `token_get_all()` bad-character
  token emission.
- Promotes upstream `bug80462.phpt` and `bug81342.phpt` for `PhpToken` method
  dispatch over TOKEN_PARSE/nullsafe and ampersand token streams.
- Promotes upstream `gh19507_eval.phpt` and `gh19507_throw.phpt` for
  `PhpToken::tokenize(..., TOKEN_PARSE)` non-canonical cast deprecations,
  recursive tokenizer calls through `eval()`, and PHP-compatible fatal traces
  when a user error handler throws during deprecation dispatch.
- Promotes upstream `PhpToken_tokenize.phpt` for `PhpToken::tokenize()` object
  dumps, token object property shape, and PHP-compatible object handle reuse
  order across consecutive tokenization results.
- Promotes upstream `bug54089.phpt` for bare `__halt_compiler` tokenization
  fallback and inline HTML remainder handling.
- Promotes upstream `bug60097.phpt` for nested heredoc tokenization inside
  braced interpolation expressions.
- Promotes upstream `token_get_all_TOKEN_PARSE_001.phpt`,
  `token_get_all_TOKEN_PARSE_002.phpt`, and `bug77966.phpt` for contextual
  `TOKEN_PARSE` semi-reserved keyword handling.
- Promotes upstream `token_get_all_TOKEN_PARSE_000.phpt` for parser-backed
  `TOKEN_PARSE` syntax validation and `catch (ParseError $e)` behavior.
- Promotes upstream `parse_errors.phpt` for `TOKEN_PARSE` invalid numeric
  literal and unicode codepoint escape `ParseError` behavior.
- Promotes upstream `token_get_all_heredoc_nowdoc.phpt` for heredoc/nowdoc
  token streams, non-parse recovery, `TOKEN_PARSE` parse diagnostics,
  indentation diagnostics, and `ParseError::getLine()` behavior.
- Promotes upstream `no_inline_html_split.phpt` for PHP-compatible non-splitting
  of `Foo<?phpBar` inline HTML under the default `short_open_tag=0` setting.

## Known gaps

- The selected fixtures do not prove the full upstream ext/tokenizer PHPT
  corpus yet.
- Numeric token IDs are intentionally engine-owned and compared by names/text,
  not hardcoded Zend token values.
- Parser-internal token names are not exposed.
- No known failures remain in the selected upstream tokenizer PHPT set.

## Gates

- `nix develop -c cargo test -p php_runtime tokenizer --no-fail-fast`
- `nix develop -c cargo test -p php_std tokenizer --no-fail-fast`
- `REFERENCE_PHP=$REFERENCE_PHP PHP_SRC_DIR=$PHP_SRC_DIR PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=tokenizer`
- `REFERENCE_PHP=$REFERENCE_PHP nix develop -c just composer-smoke`
- `REFERENCE_PHP=$REFERENCE_PHP nix develop -c just parser-fixtures`

Last tokenizer target gate after this promotion: 58 selected PASS, 0 non-green.
Focused promotion probe:
`PHPT_REQUIRE_FOCUS=1 PHPT_SKIP_BUILD=1 PHPT_REUSE_LAST=0 PHPT_TIMEOUT_SECONDS=5 scripts/phpt/module_target.sh MODULE=tokenizer FILE=ext/tokenizer/tests/token_get_all_TOKEN_PARSE_001.phpt`
and matching focused probes for `token_get_all_TOKEN_PARSE_002.phpt` and
`bug77966.phpt` each reported 1 PASS, 0 non-green. A focused probe for
`no_inline_html_split.phpt` also reported 1 PASS, 0 non-green. A focused probe
for `token_get_all_TOKEN_PARSE_000.phpt` reported 1 PASS, 0 non-green. A
focused probe for `parse_errors.phpt` reported 1 PASS, 0 non-green. A focused
probe for `token_get_all_heredoc_nowdoc.phpt` reported 1 PASS, 0 non-green.
Focused probes for `gh19507_eval.phpt` and `gh19507_throw.phpt` each reported
1 PASS, 0 non-green.
Focused probes for `PhpToken_tokenize.phpt`, `bug54089.phpt`, and
`bug60097.phpt` each reported 1 PASS, 0 non-green.
The selected tokenizer module gate reported 58 PHPT tests, 0 non-green for
both reference and target, and verified 24475 php-src manifest entries.
`composer-smoke` reported total=5 pass=5 fail=0 skip=0 known_gap=0 with the
php-src oracle. `parser-fixtures` used the same oracle and checked 67 parser
fixtures with exit status 0.
