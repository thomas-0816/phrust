# Server Wave 2 Functionality Plan

Wave 2 builds on the integrated web-server MVP with unmodified PHP application
functionality first, then server speed and hardening. The server architecture
remains direct and integrated: Hyper/Tokio accepts HTTP requests, `php_server`
routes them, `php_executor` compiles and executes PHP in-process through phrust
crates, and the response is emitted directly by the server.

The server must not use FPM, FastCGI, CGI, Apache module behavior, `mod_php`,
external PHP subprocesses, external PHP worker sockets, or a replacement web
framework stack in the hot path.

## Scope

This wave is organized as a serial set of prompts. Each prompt lands with its
own focused fixtures, tests, and validation gates before the next prompt starts.

Planned functionality:

- A compatibility fixture app and `server-compat-smoke` harness for incremental
  app-surface checks.
- PHP-compatible URL-encoded input array construction for `$_GET`, `$_POST`,
  and `$_REQUEST`.
- Bounded multipart parsing and populated `$_FILES`.
- Upload builtins: `is_uploaded_file()` and `move_uploaded_file()`.
- Cookie emission through `setcookie()` and `setrawcookie()`.
- Persistent web sessions backed by integrated server storage.
- Output-buffering builtins wired to the existing VM output-buffer stack.
- PHP execution deadlines and `set_time_limit()` integration.
- Include/realpath and compiled-include caching for hot applications.
- Bounded script cache behavior, preload, anti-stampede protection, and safe
  cache invalidation.
- Static file streaming, conditional requests, ranges, and precompressed asset
  selection.
- Production-oriented config, access logs, metrics hardening, TLS, and optional
  HTTP/2 transport.

## Out Of Scope

Wave 2 does not introduce FPM, FastCGI, CGI, Apache modules, `mod_php`,
external PHP process execution, Zend ABI emulation, a complete SAPI
compatibility layer, HTTP/3, Opcache parity, a full standard library, or a
production process manager.

Known gaps should stay explicit until implemented and verified. For example,
`server-compat-smoke` starts as a compatibility framework in Prompt 00. Future
sections are intentionally skipped until the prompt that owns that behavior
makes the section strict.

## Compatibility Harness

The compatibility app lives under `fixtures/server/apps/compat/`. The harness
can run named sections:

- `static`
- `input`
- `upload`
- `cookie`
- `session`
- `output-buffer`
- `all`

Prompt 00 makes `static` strict. Prompt 01 makes `input` strict for nested
URL-encoded query and form data. Prompt 02 makes `upload` strict for bounded
multipart fields and scalar `$_FILES` metadata. Later prompts make their
corresponding sections strict as support lands.

## Persistent Web Sessions

The integrated server owns web session persistence. By default sessions are
enabled with cookie name `PHPSESSID`, cookie path `/`, and save path
`$TMPDIR/phrust-sessions`. Operators can override these with
`--session-save-path`, `--session-cookie-name`, and `--session-cookie-path`, or
disable the feature with `--disable-sessions`.

Session files are stored as `sess_<id>` under the configured save path. Session
ids are validated as bounded ASCII path segments before any file access, so ids
cannot contain directory separators or traversal components. Payloads are a
phrust-owned PHP-serialize-compatible encoding of the whole `$_SESSION` array,
not PHP's historical `name|serialized-value` session module format. Writes use
a temporary file followed by rename so a completed write replaces the previous
payload atomically.

The server holds a process-local session mutex while loading, executing, and
finalizing a request. This prevents in-process concurrent request corruption.
It is not a cross-process lock, so multiple server processes sharing the same
session save path are outside the current guarantee.

## PHP Execution Deadlines

The integrated server configures a cooperative PHP execution deadline with
`--max-execution-ms`, defaulting to `30000`. The deadline is separate from
`--request-timeout-ms`, which only bounds request body reads. When PHP execution
exceeds its request-local deadline, the VM returns the stable diagnostic
`E_PHP_VM_EXECUTION_TIMEOUT` and the server maps it to `504 Gateway Timeout`
with body `php execution timeout`.

`set_time_limit($seconds)` resets the request-local deadline when a mutable
execution deadline is configured. Passing `0` disables the deadline for that
request, matching the supported web-mode behavior for this wave. The optional
`--disable-execution-deadline` flag disables server-created deadlines for
development and deterministic tests; metrics expose both timeout totals and
disabled-deadline request counts.

Deadline enforcement is cooperative in the VM dispatch loops. It does not use
Tokio task cancellation as the primary safety mechanism, so native blocking
builtins are checked when control returns to VM dispatch.

## Include Cache

The server owns one process-local include cache and passes it into each request
VM through `php_executor`. The cache has two independent shard sets: one for
include path resolution and one for compiled include units. Resolution entries
are keyed by the including directory, requested path, include path entries, cwd,
and allowed-root fingerprint. Compiled include entries are keyed by canonical
path plus file metadata and compiler fingerprint. File metadata changes remove
stale entries before reuse.

`include_once` and `require_once` tracking stays request-local in VM state; the
shared cache only reuses resolved paths and compiled units. The server exposes
include resolution hits/misses, include compile hits/misses, stale
invalidations, and include compile errors under `/__phrust/metrics`.

Web requests allow includes under the public docroot and its parent app root so
compatibility fixtures can keep non-public helpers outside `public/`. Compiled
include artifacts remain in memory only and are never serialized to disk.

## Script Cache Controls

The server owns a bounded process-local compiled script cache for request entry
scripts. It is configured with `--script-cache-shards` and
`--script-cache-max-entries`; entries are distributed across shards and each
shard evicts approximately least-recently-used entries when it exceeds its
share of the configured limit. The cache key includes the canonical path,
source fingerprint, source hash, source path, optimization level, and compiler
fingerprint.

By default the cache checks file metadata on every lookup so local development
sees edits immediately. Operators can set
`--script-cache-check-interval-ms <n>` to skip repeated stat checks for hot
entries during that interval. Concurrent requests for the same missing script
share a per-path compile guard so only one request compiles the miss while the
others wait for the populated entry.

`--script-cache-preload <file>` reads a newline-delimited list of absolute
paths or docroot-relative paths at startup and compiles those scripts through
the same cache path as requests. Blank lines and `#` comments are ignored.
Preload failures warn and continue by default; `--strict-preload` turns preload
read or compile failures into startup failures.

Local cache invalidation is disabled by default. When explicitly enabled with
`--enable-cache-clear-endpoint`, `POST /__phrust/cache/clear` clears process
local entry-script and include caches, and the handler still rejects
non-loopback peers. There is no remote or cross-process invalidation protocol.

Metrics expose script cache hits, misses, entries, entries by shard, stale
invalidations, compile errors, evictions, in-progress compiles, and preload
success/failure totals under `/__phrust/metrics`.

## Expected Acceptance Commands

Prompt 00 baseline:

```bash
nix develop -c cargo fmt --all --check
nix develop -c cargo clippy -p php_server -p php_executor -p php_runtime --all-targets -- -D warnings
nix develop -c cargo test -p php_server
nix develop -c bash scripts/server/compat_smoke.sh static
nix develop -c just server-smoke
nix develop -c rg "FastCGI|php-fpm|mod_php|CGI|std::process::Command|Command::new" crates/php_server crates/php_executor docs README.md
```

The full wave ends with the broader final integration gates documented in the
serial prompt pack.
