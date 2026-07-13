# Standard library Filesystem Path and Stat MVP

Reference target: PHP 8.5.7 (`php-8.5.7`).

The standard library provides PHP-visible path helpers and a capability-gated stat MVP.
The standard library extends the same capability model to file I/O; see
`docs/stdlib/file-io.md`.

## Implemented Functions

- `basename`
- `dirname`
- `pathinfo`
- `realpath`
- `file_exists`
- `is_file`
- `is_dir`
- `is_link`
- `is_readable`
- `is_writable`
- `filesize`
- `filemtime`
- `filetype`
- `stat`
- `lstat`
- `clearstatcache`

## Capability Model

Path-only helpers do not touch the host filesystem. Stat helpers resolve
relative paths against the request `RuntimeContext::cwd` and then check the
normalized path against `RuntimeContext::filesystem`.

Without an explicit allowed root, stat helpers return `false` for local paths.
This keeps ordinary VM execution from reading arbitrary host files. Tests enable
access by constructing `FilesystemCapabilities::with_allowed_roots` over an
isolated temp directory.

## Platform Behavior

Path separator handling uses the target platform's path separators. Unix builds
treat `/` as the path separator; Windows builds treat both `/` and `\` as path
separators.

Symlink behavior uses `std::fs::symlink_metadata` for `lstat` and `is_link`.
Symlink tests are conditional: Unix creates a symlink directly, and Windows skips
the symlink assertion when the platform or privileges reject symlink creation.

## Stat Cache MVP

The current stat MVP does not retain stale positive stat entries across builtin
calls. `clearstatcache()` is implemented as a deterministic no-op returning
`null`. This is intentionally safer than caching host filesystem state without a
request-lifetime invalidation model; later filesystem work items can add a request
stat-cache table if PHP-visible cache edge cases become required.

## Known Gaps

`stat` and `lstat` currently expose stable MVP fields (`mode`, `size`, `mtime`,
`type`, and selected numeric slots) rather than every byte-perfect PHP stat
field. This is tracked as `STDLIB-GAP-STAT-BYTE-PERFECT`.
