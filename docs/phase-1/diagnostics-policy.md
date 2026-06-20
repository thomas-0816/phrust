# Diagnostics Policy

The Phase 1 lexer must treat invalid input as recoverable scanner state.

## Rules

- Public lexer APIs must not panic for malformed source.
- The scanner must always make forward progress.
- Invalid bytes, unterminated literals, malformed heredoc/nowdoc state, and
  unsupported scanner transitions should produce `LexDiagnostic` entries.
- Unterminated block comments emit a comment token through EOF and produce
  `LexDiagnosticKind::UnterminatedBlockComment`.
- Unterminated non-interpolated quoted strings emit
  `T_CONSTANT_ENCAPSED_STRING` through EOF and produce
  `LexDiagnosticKind::UnterminatedString`.
- Unterminated encapsed strings produce `LexDiagnosticKind::UnterminatedString`.
- Unterminated heredoc and nowdoc blocks produce
  `LexDiagnosticKind::UnterminatedHeredoc`.
- Bad scripting-mode control bytes emit `T_BAD_CHARACTER` and produce
  `LexDiagnosticKind::BadCharacter`.
- Diagnostics use byte ranges from `php_source::TextRange`.
- Diagnostics do not imply parser recovery, AST nodes, runtime values, or VM
  behavior.

`just fuzz-lexer-smoke` runs deterministic invariant tests over malformed and
edge-case inputs. The smoke asserts that `lex_all()` does not panic, token spans
stay inside the source, non-EOF tokens make progress, and token ranges do not
overlap.

Current limitation: the public lexer API accepts Rust `str`, so byte sequences
that are not valid UTF-8 are rejected at file-reading boundaries by the CLI
rather than represented as byte-exact lexer input. Byte-exact invalid input
belongs to a future source API change before Phase 2 relies on it.

Later scanner rules must add diagnostics at the point where invalid input is
detected.
