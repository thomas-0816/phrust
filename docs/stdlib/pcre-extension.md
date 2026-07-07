# Standard library PCRE Extension
Reference target: PHP 8.5.7 (`php-8.5.7`).

Work item implements an ext/pcre MVP backed by PCRE2:

- `preg_match`
- `preg_match_all`
- `preg_replace`
- `preg_replace_callback`
- `preg_split`
- `preg_grep`
- `preg_quote`
- `preg_last_error`
- `preg_last_error_msg`

The runtime uses the Rust `pcre2` crate and the Nix dev shell provides the
native `pcre2` package. This follows The standard-library scope and avoids substituting Rust
`regex` semantics for PCRE behavior.

## Implemented Behavior

- PHP-style delimited patterns with escaped delimiters and character classes.
- Common modifiers: `i`, `m`, `s`, `x`, and `u`.
- Request-local compiled-pattern cache in `BuiltinContext`.
- Capture arrays for `preg_match` and `preg_match_all`.
- `PREG_OFFSET_CAPTURE`, `PREG_SET_ORDER`, `PREG_PATTERN_ORDER`, and
  `PREG_UNMATCHED_AS_NULL` MVP shapes.
- Replacement expansion for `$1` and `\1` style capture references.
- `preg_split` flags for no-empty pieces, delimiter captures, and offsets.
- `preg_grep` including `PREG_GREP_INVERT`.
- `preg_quote` with optional delimiter escaping.
- Invalid patterns return `false` and update `preg_last_error` metadata.

## Known Gaps

The following gaps are tracked in `docs/stdlib/known-gaps.md`:

- `STDLIB-GAP-PCRE-ADVANCED-FLAGS`
- `STDLIB-GAP-PCRE-CALLBACK-DISPATCH`
- `STDLIB-GAP-PCRE-LAST-ERROR-VM-PERSISTENCE`
