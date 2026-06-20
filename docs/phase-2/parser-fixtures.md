# Parser Fixtures

Parser fixtures are curated, small syntax examples. They are not framework
corpora and should emphasize one syntax area per file.

The optional php-src corpus smoke is separate from curated fixtures. It extracts
temporary files into `target/parser-corpus-smoke/` and must not commit generated
corpus files.

## Layout

- `fixtures/parser/valid/`: accepted by PHP 8.5.7 and by the Rust parser.
- `fixtures/parser/invalid/`: rejected by PHP 8.5.7 and expected to produce
  Rust diagnostics. Each file starts with a short `// invalid:` comment.
- `fixtures/parser/recovery/`: malformed or boundary-focused inputs used to
  exercise recovery and lossless CST reconstruction.
- `fixtures/parser/php85/` and `fixtures/parser/valid/php85/`: PHP 8.5 syntax
  forms kept separate for quick reference.
- `fixtures/parser/known_gaps.toml`: executable allowlist for accepted
  parser/reference differences. It is empty when there are no accepted gaps.
- `target/parser-corpus-smoke/`: generated, optional corpus extraction and JSON
  smoke report. This directory is ignored and regenerated on demand.

## Coverage Matrix

| Category | Status | Primary fixtures |
| --- | --- | --- |
| PHP/HTML modes | implemented | `valid/pure_html.php`, `valid/inline_html.php`, `valid/multiple_php_blocks.php`, `valid/short_echo_tag.php`, `valid/php_html_modes.php` |
| Statements | implemented | `valid/statements_basic.php`, `valid/control_flow.php`, `valid/alternative_syntax.php`, `valid/try_catch_finally.php`, `valid/declare.php`, `valid/statements_misc.php` |
| Expressions | implemented | `valid/expressions_basic.php`, `valid/operator_groups.php`, `valid/expressions_assignment_ternary.php`, `valid/expressions_postfix.php`, `valid/match_expression.php`, `valid/generators_yield.php` |
| Functions | implemented | `valid/functions.php`, `valid/closures.php`, `valid/arrow_functions.php` |
| OOP | implemented | `valid/classes_basic.php`, `valid/interfaces_traits.php`, `valid/enums.php`, `valid/class_members.php`, `valid/property_hooks.php`, `valid/promoted_properties.php`, `valid/attributes.php` |
| Types | implemented | `valid/types.php`, `valid/dnf_types.php` |
| Strings | partial | `valid/strings.php`, `valid/encapsed_strings.php`, `valid/heredoc_nowdoc.php` |
| PHP 8.5 | implemented | `valid/php85/pipe_operator.php`, `valid/php85/clone_with.php`, `valid/php85/void_cast.php`, `valid/php85/constant_expressions.php`, `php85/syntax_matrix.php` |
| Recovery | implemented | `recovery/bad_php_block.php`, `recovery/bad_attribute.php`, `recovery/missing_expression.php`, `recovery/unclosed_delimiter.php` |

## Known Gaps

There are no accepted parser fixture gaps. If a fixture mismatch is accepted
temporarily, add it to `fixtures/parser/known_gaps.toml` and document it in
`docs/phase-2/parser-known-gaps.md`.

## Checks

```bash
nix develop -c just parser-fixtures
nix develop -c just parser-diff
nix develop -c just cst-roundtrip
nix develop -c just extract-parser-corpus
nix develop -c just parser-corpus-smoke
```

`parser-fixtures` runs the PHP lint oracle. `parser-diff` compares Rust parser
acceptance, diagnostic count summary, and roundtrip status against the pinned
PHP 8.5.7 reference when available.

`cst-roundtrip` parses every committed parser fixture and asserts exact source
reconstruction through the Rust CST.

`parser-corpus-smoke` discovers a local php-src checkout from `PHP_SRC_DIR` or
`third_party/php-src`, extracts a small deterministic sample from `Zend/tests`,
`tests/lang`, and syntax-focused Zend subdirectories, and parses only PHP code
sections. For `.phpt` inputs it uses `--FILE--` or `--FILEEOF--`; metadata and
expected-output sections are not parsed as PHP. If no source checkout or PHP
8.5.7 reference binary is available, the command prints a `[skip]` line and
exits successfully.

The corpus smoke is intentionally not a hard CI gate. It reports checked file
count, deviation count, top deviations, and writes
`target/parser-corpus-smoke/parser-corpus-smoke-report.json` for local analysis.
Use curated fixtures plus `fixtures/parser/known_gaps.toml` for versioned,
executable acceptance gaps.

## Corpus Smoke Observation

A local run on 2026-06-20 extracted 50 syntax-focused files from the pinned
`php-src` checkout and reported 11 exploratory deviations. These are not
accepted fixture gaps. The dominant category was PHP lint rejecting
semantic class/member modifier cases that the syntax parser intentionally
accepts; a smaller category was richer expression-heavy corpus files needing
reduced fixtures before promotion.
