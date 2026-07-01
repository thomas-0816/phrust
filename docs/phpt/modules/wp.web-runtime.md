# wp.web-runtime

- Strategy: in-process WordPress-like web/runtime request state
- Classification: generated focused harness plus server transport tests
- Selected manifest: `tests/phpt/manifests/modules/wp.web-runtime.selected.jsonl`

## Implemented Scope

- Response functions: `header`, `header_remove`, `headers_list`,
  `headers_sent`, `http_response_code`, `setcookie`, and `setrawcookie`.
- HTTP request superglobals: `$_GET`, `$_POST`, `$_COOKIE`, `$_REQUEST`,
  `$_SERVER`, and `$_FILES`.
- URL-encoded request parsing supports nested bracket keys such as
  `user[name]` and append keys such as `items[]`.
- Multipart form parsing writes upload bodies to a deterministic docroot-local
  temp directory and exposes upload metadata through `$_FILES`.
- `php://input` exposes deterministic request body bytes through the existing
  stream wrapper registry.
- Process-local web sessions reuse incoming `PHPSESSID` cookies, persist
  `$_SESSION` across consecutive requests in one server process, and emit
  `Set-Cookie` when `session_start()` creates a new id.
- Read-only local `phar://` includes use the existing PHAR parser and include
  loader root policy.

## Fixture

- `tests/phpt/generated/wp.web-runtime/platform-surface.phpt` checks the
  CLI-comparable function and superglobal surface.
- `tests/phpt/generated/wp.web-runtime/transport-web-only.phpt` is intentionally
  skipped because web transport behavior is not populated by the PHP CLI oracle.

## Server Coverage

- `crates/php_server` tests cover URL-encoded bodies, cookies, multipart upload
  parsing, and HTTP session cookie reuse.
- `scripts/server/smoke.sh` covers GET, nested POST, cookies, `php://input`,
  multipart uploads, response status/headers/cookies, and session reuse through
  real HTTP requests.

## Remaining Gaps

- Cross-process/file-backed session storage, custom session handlers, and full
  browser cookie option parity remain future work.
- Multipart parsing is an MVP for in-process development server requests, not a
  complete PHP SAPI upload implementation.
- PHAR support remains read-only and limited to local uncompressed archive
  entries.

## Target Gates

- `nix develop -c cargo test -p php_runtime`
- `nix develop -c cargo test -p php_vm`
- `nix develop -c cargo test -p php_server`
- `nix develop -c just server-smoke`
- `nix develop -c just phpt-dev-module MODULE=wp.web-runtime`
