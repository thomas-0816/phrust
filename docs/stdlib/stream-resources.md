# Standard library Stream Resources

Reference target: PHP 8.5.7 (`php-8.5.7`).

The standard library provides the runtime resource handle model used by later stream and
filesystem work items. The standard library provides the bounded wrapper registry for local
file streams and PHP pseudo streams. The model lives in `php_runtime` so the VM
and standard builtins can share one request-local representation without
introducing another parser, lexer, or semantic layer.

## Runtime Model

- `ResourceId` is a stable request-local numeric handle. IDs start at `1` and
  are never reused within a `ResourceTable`.
- `ResourceKind` distinguishes open stream resources from closed handles.
- `StreamFlags` records readable, writable, and seekable capabilities.
- `StreamMetadata` records wrapper type, stream type, mode, and URI.
- `ResourceRef` is a reference-counted handle stored as `Value::Resource`.
- `ResourceTable` owns request-local handle allocation and finalization.
- `StreamWrapperRegistry` opens resources through `file://`, the implicit local
  file wrapper, and `php://` pseudo wrappers.

`ResourceRef::close()` and `ResourceTable::close()` are idempotent. The first
close transitions the handle to `ResourceKind::Closed` and returns `true`.
Repeated closes return `false` without panicking or changing the resource ID.

## PHP-Visible Helpers

The standard registry and VM builtin registry expose:

- `is_resource($value)`
- `get_resource_id($resource)`
- `get_resource_type($resource)`

Non-resource arguments to `get_resource_id` and `get_resource_type` return
`false` in the current deterministic MVP. Closed resources keep their stable ID
and report resource type `Unknown`.

## Finalization and GC

Resources are request-local runtime handles, not PHP objects. PHP object
destructors are therefore not invoked when a resource is closed or finalized.
The Standard library GC tracks arrays, objects, references, closures, fibers, and
generators for cycle diagnostics; resources do not add graph edges and are
finalized through `ResourceTable::finalize_all()`.

Later file, directory, and wrapper work items should call `finalize_all()` during
request shutdown and should close individual handles through `ResourceTable`
rather than relying on object destructor behavior.

## Wrapper Capabilities

The wrapper MVP supports:

- implicit local file paths
- explicit `file://` paths
- `php://memory`
- `php://temp`
- capability-gated `php://stdin`, `php://stdout`, and `php://stderr`

Local file streams are denied unless the caller provides
`FilesystemCapabilities::with_allowed_roots`. Relative paths are resolved
against the caller-provided current working directory and normalized before the
allowed-root check.

`php://memory` and `php://temp` use deterministic request-local buffers. They
are readable and writable with `w+`, `r+`, and other parsed read/write modes.
Stdio pseudo streams are disabled unless `with_stdio(true)` is set; even when
enabled, they are represented by deterministic buffers rather than direct host
stdin/stdout/stderr handles.

Remote URL schemes such as `http://`, `https://`, `ftp://`, and `ftps://` are
rejected with `E_PHP_RUNTIME_STREAM_REMOTE_DISABLED`.

## Remaining Scope

The streams layer intentionally does not add PHP-visible `fopen`,
`fread`, `fwrite`, directory handles, or stat/path functions. Those PHP-visible
producers and operations are tracked as `STDLIB-GAP-STREAM-PRODUCERS` until the
follow-up streams and filesystem work items implement them.
