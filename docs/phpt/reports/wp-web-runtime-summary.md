# wp.web-runtime Summary

## Implemented Behavior

- HTTP response state is request-local and transported by `php_server`:
  `header`, `headers_list`, `headers_sent`, `http_response_code`,
  `setcookie`, and `setrawcookie`.
- HTTP request state seeds `$_GET`, `$_POST`, `$_COOKIE`, `$_REQUEST`,
  `$_SERVER`, and `$_FILES` for the in-process server.
- URL-encoded query/form parsing supports repeated values, nested bracket keys,
  and append keys such as `items[]`.
- `php://input` exposes deterministic request body bytes through the stream
  wrapper registry and `file_get_contents`.
- Multipart form parsing supports ordinary fields and file parts with
  deterministic docroot-local temporary upload files.
- Web sessions are process-local to the integrated server; incoming
  `PHPSESSID` cookies are reused across consecutive requests and new session
  ids emit a `Set-Cookie` header.
- Read-only local `phar://` entries are available through streams and
  include/require resolution.

## Module Counts

| Module | Before | After |
| --- | --- | --- |
| `wp.web-runtime` | New module | 1 PASS, 1 SKIP, 0 FAIL, 0 BORK |
| `filesystem.streams` | 11 PASS, 0 SKIP, 0 FAIL, 0 BORK | 11 PASS, 0 SKIP, 0 FAIL, 0 BORK |
| `session` | 1 PASS, 0 SKIP, 0 FAIL, 0 BORK; CLI-only scope | 1 PASS, 0 SKIP, 0 FAIL, 0 BORK; process-local web cookie persistence documented |
| `phar` | 1 PASS, 0 SKIP, 0 FAIL, 0 BORK; platform probe scope | 1 PASS, 0 SKIP, 0 FAIL, 0 BORK; read-only `phar://` stream/include scope documented |

The `wp.web-runtime` selected module intentionally keeps web transport behavior
in server tests and `server-smoke`; the CLI PHPT fixture covers only the
CLI-comparable function and superglobal surface.

## Validation

The closeout gates are recorded in
`docs/phpt/reports/wp-web-runtime-current.md`. The PHP oracle used for PHPT and
stdlib differential gates was:

`/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php`

The local `third_party/php-src` checkout is absent, so PHPT
source-integrity verification reports a skip. No generated `target/` reports or
vendored `php-src` files are part of the repository state.

## Remaining Gaps

- No FPM, FastCGI, CGI, Apache module, `mod_php`, phpdbg, or SAPI lifecycle is
  implemented.
- Cookie behavior covers WordPress-compatible basics, not the full PHP cookie
  matrix.
- Multipart uploads are an MVP: no streaming upload optimization, huge-upload
  tuning, SAPI hooks, or full upload INI matrix.
- Session state is process-local to one integrated server instance; cross-process
  or file-backed persistence, custom handlers, locking, serializers, and full
  INI policy remain future work.
- PHAR support is read-only for local uncompressed entries; signing,
  compression, stub execution, metadata parity, and mutation APIs remain gaps.
- Network streams, user stream wrappers, and external filesystem transports are
  outside this module scope.

## Merge Notes

Likely conflict areas for adjacent feature work:

- `crates/php_runtime/src/builtins/modules/core.rs`
- `crates/php_runtime/src/context.rs`
- `crates/php_runtime/src/resource.rs`
- `crates/php_server/src/server.rs`
- `crates/php_vm/src/include.rs`
- `docs/phpt/modules/session.md`
- `docs/phpt/reports/io-framework-extensions-summary.md`
- PHPT module manifests under `tests/phpt/manifests/modules/`
