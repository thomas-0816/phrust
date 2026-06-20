# Scanner Modes

PHP cannot be lexed accurately with one trivial scanner mode.

## Required Mode Areas

| Area | Why it matters |
| --- | --- |
| PHP/HTML switching | PHP files start in HTML mode. Code starts only after `<?php`, `<?=`, or configured short open tags. |
| Scripting mode | Keywords, variables, numbers, comments, operators, and strings are recognized only in PHP code. |
| Encapsed strings | Double-quoted and backtick strings can contain interpolation. |
| Heredoc | Behaves like an encapsed string with label-based termination. |
| Nowdoc | Label-based string without interpolation. |
| String variable offsets | Interpolation has specialized token behavior for `$arr[0]` and related forms. |
| Looking for variable name | Some interpolation states temporarily scan only for variable names. |

## Problem Zones

- PHP/HTML mode transitions.
- Heredoc and nowdoc labels.
- Modern heredoc indentation rules.
- Encapsed string interpolation.
- Variable variables.
- Close tags inside comments or strings.
- Parser-contextual keyword handling under `TOKEN_PARSE`.

## PHP/HTML Switching

The scanner starts in `InlineHtml` mode.

Recognized open tags:

- `<?php` followed by EOF or one whitespace unit emits `T_OPEN_TAG`. The token
  text includes the tag and that one whitespace unit. CRLF counts as one
  whitespace unit and both bytes are included.
- `<?=` emits `T_OPEN_TAG_WITH_ECHO`.
- `<?` emits `T_OPEN_TAG` only when `LexerConfig.short_open_tag` is true.

The scanner enters `Scripting` mode after an open tag.

In `Scripting` mode, `?>` emits `T_CLOSE_TAG` and returns to `InlineHtml`.
`T_CLOSE_TAG` includes a following LF, CR, or CRLF when present, matching the
reference tokenizer's line behavior.

## Encapsed Strings

Interpolated double-quoted strings and backtick strings enter a dedicated
encapsed mode after emitting the opening delimiter as a symbol token. The
closing delimiter is emitted as a symbol token and returns the scanner to
`Scripting`.

The Phase 1 encapsed mode recognizes simple variable interpolation, object
property interpolation, numeric array offsets, `{$name}`, and `${name}`. Plain
text and escaped non-interpolation sequences are emitted as
`T_ENCAPSED_AND_WHITESPACE`.

Known limitations remain for complex interpolation expressions, nested
expression parsing, and full string-offset syntax. Those are intentionally not
AST work in Phase 1.

## Heredoc And Nowdoc

`<<<LABEL` and quoted-label forms emit `T_START_HEREDOC` including the newline
after the start marker, then enter `Heredoc` or `Nowdoc` mode. Heredoc uses the
same simple interpolation machinery as double-quoted strings. Nowdoc emits its
body as `T_ENCAPSED_AND_WHITESPACE` without interpolation.

Closing labels are recognized at the start of a line, including modern
indentation accepted by the pinned PHP 8.5 reference. `T_END_HEREDOC` includes
the indentation and label, while a following semicolon remains a separate
symbol token.

## Phase 1 Rule

Mode handling is lexer state only. It must not construct an AST, parse
expressions, or evaluate PHP code.
