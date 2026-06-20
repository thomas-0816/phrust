# Phase 1 Risk Register

| Risk | Impact | Mitigation |
| --- | --- | --- |
| Accidentally implementing parser behavior in the lexer. | Phase boundary becomes unclear and `TOKEN_PARSE` parity is misrepresented. | Keep hard gate to `token_get_all($code, 0)` and document parser-contextual cases for Phase 2. |
| Numeric PHP token values are treated as stable. | Compatibility breaks across PHP builds or versions. | Normalize by token name and token text only. |
| Byte positions are confused with Unicode scalar positions. | Spans diverge from PHP source offsets. | Make source positions byte-oriented and document display-only line/column mapping. |
| Scanner modes are oversimplified. | Tags, strings, heredoc/nowdoc, and interpolation diverge from Zend. | Model modes explicitly and compare curated fixtures to the reference. |
| Invalid input panics or loops forever. | Lexer is unsafe for tooling and fuzzing. | Require progress tests and diagnostics for invalid input. |
| Reference PHP is unavailable. | Differential checks cannot run locally. | Skip reference-dependent checks clearly while still running Rust tests. |
| `TOKEN_PARSE` behavior is overclaimed. | Lexer claims parser-contextual compatibility it cannot know. | Prepare the flag but defer strict `TOKEN_PARSE` parity to Phase 2. |
| Numeric edge cases split differently after PHP scanner changes. | Differential lexer parity can regress around invalid forms and separators. | Keep pinned-reference fixtures for `1e`, `0x`, `0b2`, and `1__2`; do not evaluate numeric values in the lexer. |

This register should be updated whenever fixture comparison finds a known
deviation or a scanner mode is intentionally incomplete.
