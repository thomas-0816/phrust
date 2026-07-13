# Standard library Filesystem File I/O MVP

Reference target: PHP 8.5.7 (`php-8.5.7`).

The standard library provides PHP-visible file and stream-handle operations on top of the
Standard library resource model and capability-gated local file wrapper.

## Implemented Functions

- `fopen`
- `fclose`
- `fread`
- `fwrite`
- `fgets`
- `fgetc`
- `feof`
- `fflush`
- `fseek`
- `ftell`
- `rewind`
- `file_get_contents`
- `file_put_contents`
- `readfile`
- `copy`
- `rename`
- `unlink`
- `mkdir`
- `rmdir`
- `touch`
- `tempnam`
- `tmpfile`

## Mode Coverage

`fopen` supports the required local modes:

- `r`: existing readable stream
- `w`: writable stream, truncating or creating the file
- `a`: writable stream positioned at the end, creating the file if missing
- `x`: exclusive writable create, failing when the file exists
- `c`: writable create without truncating existing contents

The `+` modifier enables read/write streams for these modes. Binary/text
modifiers are accepted as mode text but do not change byte handling because the
runtime stores PHP strings as byte strings.

## Capability Model

All local file operations resolve relative paths against `RuntimeContext::cwd`
and then require the normalized path to be inside
`RuntimeContext::filesystem.allowed_roots`. Without an explicit allowed root,
file reads, writes, metadata changes, and stream opens return PHP-visible
failure values instead of touching the host filesystem.

Remote stream wrappers remain disabled by default. `php://memory` and
`php://temp` stay deterministic in-memory stream buffers.

## Known Gaps

The MVP returns PHP-style failure values (`false` or `-1`) but does not yet emit
byte-perfect warning text for every failed operation. `fseek` supports
`SEEK_SET`, `SEEK_CUR`, and `SEEK_END`, including PHP-style `-1` returns for
invalid negative targets and invalid `whence` values without moving the cursor.
`tmpfile` creates a deterministic temporary file under the first allowed root
and does not yet implement PHP's automatic unlink-on-close lifetime.
