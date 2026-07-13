# Standard library Stream Functions and Contexts MVP

Reference target: PHP 8.5.7 (`php-8.5.7`).

The standard library provides Composer-facing stream metadata, stream copy/content helpers,
stream contexts, include-path resolution, and local/TTY probes.

## Implemented Functions

- `stream_get_wrappers`
- `stream_get_meta_data`
- `stream_get_contents`
- `stream_copy_to_stream`
- `stream_context_create`
- `stream_context_get_options`
- `stream_context_set_option`
- `stream_resolve_include_path`
- `stream_is_local`
- `stream_isatty`

## Contexts and Options

Stream contexts are request-local resources stored in `ResourceTable`.
`stream_context_set_option` supports both the array form and the
`wrapper, option, value` form. Unknown wrappers and options are preserved in the
context options array rather than rejected.

## Include Path

`stream_resolve_include_path` uses `BuiltinContext::include_path`, which VM
dispatch fills from the same request INI include path used by include/require.
Candidates are resolved relative to request CWD, normalized, capability-checked,
and returned only when the target exists.

## Determinism

`stream_get_wrappers` reports the implemented local wrappers: `file` and `php`.
`stream_isatty` returns deterministic `false` for Standard library streams; host TTY
probing remains outside the default capability model.

`stream_get_meta_data` exposes stable MVP fields:

- `wrapper_type`
- `stream_type`
- `mode`
- `uri`
- `seekable`
- `eof`
- `timed_out`
- `blocked`
