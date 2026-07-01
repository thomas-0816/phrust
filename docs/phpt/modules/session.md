# session

- Strategy: deterministic CLI sessions plus process-local web persistence
- Classification: real-implementation-required for framework session support
- Selected manifest: `tests/phpt/manifests/modules/session.selected.jsonl`
- Selected gate: 7 PASS

## Implemented Scope

Request-local session state is available for CLI execution. The runtime now
registers the `session` extension, exposes the PHP session status constants,
seeds `$_SESSION`, and implements:

- `session_start`
- `session_status`
- `session_id`
- `session_name`
- `session_cache_expire`
- `session_cache_limiter`
- `session_module_name`
- `session_save_path`
- `session_write_close`
- `session_commit`
- `session_destroy`

CLI execution remains request-local and deterministic. `$_SESSION` mutations
persist inside one VM request and are synchronized back to the request-local
session store around session builtin calls. The in-process web server owns a
bounded process-local `RuntimeSessionStore`, reuses incoming `PHPSESSID`
cookies, persists `$_SESSION` across consecutive requests in the same server
process, and emits a `Set-Cookie` header when `session_start()` creates a new
id.

## Remaining Gaps

- Stable ID: `PHPT-SESSION-CLI-MVP-GAPS`
- Reference behavior: PHP with `session` enabled includes web SAPI lifecycle,
  cookie headers, file-backed storage, serializers, INI configuration, custom
  save handlers, locking, and `SessionHandler` classes/interfaces.
- Current phrust behavior: request-local CLI session basics pass through
  generated coverage; the web server covers `PHPSESSID` cookie reuse, creation,
  destroy, and process-local persistence across requests. Selected
  cache/module/save-path metadata and write-close behavior remain intentionally
  limited. Cross-process/file-backed persistence, custom handlers, upload
  lifecycle, INI policy, and the full session handler matrix remain
  unsupported.
- Fixture: `tests/phpt/generated/session/platform-checks.phpt`
- Next owner layer: future request/runtime state work for filesystem-backed
  persistence, INI policy, and handler objects.

## Non-Scope

- Full web SAPI lifecycle outside the in-process server
- uploads/request lifecycle
- file-backed or cross-process persistence and locking
- custom session handlers
- `SessionHandler` and related handler classes/interfaces

## Source References

- `ext/session/session.stub.php`
- `ext/session/php_session.h`
- `ext/session/tests/`

## Target Gates

- `nix develop -c cargo test -p php_runtime session -- --nocapture`
- `nix develop -c cargo test -p php_std introspection -- --nocapture`
- `nix develop -c cargo test -p php_vm session -- --nocapture`
- `nix develop -c just phpt-dev-module MODULE=session`
- `nix develop -c just verify-stdlib` if runtime code changes
- `nix develop -c just verify-phpt`
