# Standard library Formatted Output Helpers

Reference target: PHP 8.5.7 (`php-8.5.7`).

The standard library implements a deterministic formatting MVP for `printf`,
`sprintf`, `vprintf`, and `vsprintf`.

Supported format surface:

- Specifiers: `%s`, `%d`, `%u`, `%f`, `%F`, `%x`, `%X`, `%o`, `%c`, and `%%`.
- Width and precision for strings, integers, and fixed-point floats.
- Space, plus, left-align, zero, and custom single-byte padding flags.
- `printf` and `vprintf` write through `OutputBuffer` and return byte counts.
- `sprintf` and `vsprintf` return exact runtime strings.

Known gaps for this scope:

- `STDLIB-GAP-PRINTF-ADVANCED-FORMATS`
- `STDLIB-GAP-FPRINTF-STREAM-RESOURCE`
