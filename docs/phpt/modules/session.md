# session

- Strategy: deterministic CLI sessions plus process-local web persistence
- Classification: real-implementation-required for framework session support
- Selected manifest: `tests/phpt/manifests/modules/session.selected.jsonl`
- Selected gate: 92 PASS / 1 SKIP

## Implemented Scope

Request-local session state is available for CLI execution. The runtime now
registers the `session` extension, exposes the PHP session status constants,
exposes `$_SESSION` after successful session activation, and implements:

- `session_start`
- `session_status`
- `session_abort`
- `session_id`
- `session_create_id`
- `session_name`
- `session_regenerate_id`
- `session_reset`
- `session_cache_expire`
- `session_cache_limiter`
- `session_module_name`
- `session_save_path`
- `session_write_close`
- `session_commit`
- `session_destroy`
- `session_unset`
- `session_gc`
- `session_encode`
- `session_decode`
- `session_get_cookie_params`
- `session_set_cookie_params`

CLI execution remains request-local and deterministic. `$_SESSION` mutations
persist inside one VM request and are synchronized back to the request-local
session store around session builtin calls. The in-process web server owns a
bounded process-local `RuntimeSessionStore`, reuses incoming `PHPSESSID`
cookies, persists `$_SESSION` across consecutive requests in the same server
process, and emits a `Set-Cookie` header when `session_start()` creates a new
id. Cookie parameter readback is wired through the request-local INI registry
for `session.cookie_lifetime`, `session.cookie_path`,
`session.cookie_domain`, `session.cookie_secure`,
`session.cookie_partitioned`, `session.cookie_httponly`, and
`session.cookie_samesite`. Scalar and array `session_set_cookie_params()`
updates the request-local cookie lifetime/path/domain/secure/partitioned/
httponly/samesite INI values before a session is active, normalizes supported
boolean cookie INI values to PHP-compatible `"1"`/`"0"` strings, and emits the
PHP-compatible active-session and invalid-option warnings without mutating
existing cookie parameters after `session_start()`. `session_cache_expire()`
and `session_cache_limiter()` are backed by request-local INI values, honor
`--INI--`/`ini_set()` overrides, update the INI registry when changed before
activation, and match PHP's active-session warning/return-value behavior.
`session_name()` is backed by `session.name`, rejects numeric, empty,
null-byte, and forbidden-character names without mutation, and honors the
active-session mutation warning path. `session_id()` supports pre-start get/set,
preserves PHP's null-byte display compatibility, and rejects active-session
mutation attempts without changing the current ID. Generated CLI session IDs
honor `session.sid_length`, `ini_set()` emits the PHP 8.5 deprecations for
non-default `session.sid_length` and `session.sid_bits_per_character` values,
and `session_regenerate_id()` replaces active session IDs deterministically.
`session_create_id()` generates deterministic CLI IDs, validates prefixes with
PHP-compatible warnings and `ValueError`s, and cooperates with
`session.use_strict_mode` so strict starts replace preselected IDs.
`session_module_name()` validates handler names, rejects direct `"user"`
selection with PHP's `ValueError`, rejects unknown modules with PHP-compatible
warnings, and refuses active-session mutation. The
`files` save handler now rejects missing `session.save_path` directories during
startup with PHP-compatible open/read failure warnings, covers selected
open_basedir save-path warnings, and leaves the session inactive. The
request-local INI registry exposes the SESSION-1 runtime defaults used by
`session_ini_set.phpt`, normalizes boolean session INIs, keeps
`session.auto_start` non-runtime-changeable, and rejects supported session INI
mutations after `session_start()` with PHP's active-session warning.
Inactive `session_destroy()` emits PHP's uninitialized-session warning, and
`session_write_close()`/`session_commit()` return `false` after the session has
already been closed while preserving active close behavior. `session_abort()`
and `session_reset()` restore the last committed request-local session
snapshot, while `session_unset()` clears active live session data without
destroying the committed snapshot. `session_gc()` matches the selected
active/inactive return and warning surface without performing persistent-store
cleanup. `session_start()` now validates option arrays before activation,
reports PHP-compatible
`TypeError` and `ValueError` messages for invalid option values and non-string
option keys, emits unsupported-option warnings, ignores later
`session_start()` calls with PHP's active-session notice, discards user-created
pre-start `$_SESSION` globals during first activation, preserves live
`$_SESSION` after `session_destroy()` until the next start resets it, and
supports `read_and_close` so enabled starts close the request-local session
immediately. Startup INI overrides now apply before user code, so
`session.auto_start=1` activates the request-local session before top-level
execution while remaining non-runtime-mutable through `ini_set()` and reporting
PHP's automatic-start notice for repeated `session_start()` calls.
`session_encode()` and `session_decode()` support the selected `php`,
`php_binary`, and `php_serialize` serializer cases, including inactive-session
warnings, closed-session false returns, numeric-key skip warnings for the `php`
serializers, `php_serialize` numeric-key array payloads, resource payloads as
`i:0;`, `serialize_precision`-aware float payloads, top-level `R:n` reference
records and recursive self-references in `php`/`php_binary` session payloads,
decode error warning/return behavior, unknown serializer startup and
`ini_set()` rejection warnings, and PHP's undefined-global `$_SESSION` warning
after failed startup.

## Selected Upstream Coverage

- `ext/session/tests/session_abort_basic.phpt`
- `ext/session/tests/bug73100.phpt`
- `ext/session/tests/session_cache_expire_basic.phpt`
- `ext/session/tests/session_cache_expire_variation1.phpt`
- `ext/session/tests/session_cache_expire_variation2.phpt`
- `ext/session/tests/session_cache_expire_variation3.phpt`
- `ext/session/tests/session_cache_limiter_basic.phpt`
- `ext/session/tests/session_cache_limiter_variation1.phpt`
- `ext/session/tests/session_cache_limiter_variation2.phpt`
- `ext/session/tests/session_cache_limiter_variation3.phpt`
- `ext/session/tests/session_commit_basic.phpt`
- `ext/session/tests/session_commit_variation1.phpt`
- `ext/session/tests/session_commit_variation2.phpt`
- `ext/session/tests/session_commit_variation3.phpt`
- `ext/session/tests/session_commit_variation4.phpt`
- `ext/session/tests/session_commit_variation5.phpt`
- `ext/session/tests/session_create_id_basic.phpt`
- `ext/session/tests/session_create_id_invalid_prefix.phpt`
- `ext/session/tests/session_destroy_variation1.phpt`
- `ext/session/tests/session_destroy_variation2.phpt`
- `ext/session/tests/session_destroy_variation3.phpt`
- `ext/session/tests/session_decode_basic.phpt`
- `ext/session/tests/session_decode_basic_serialize.phpt`
- `ext/session/tests/session_decode_error2.phpt`
- `ext/session/tests/session_decode_error3.phpt`
- `ext/session/tests/session_decode_variation1.phpt`
- `ext/session/tests/session_decode_variation2.phpt`
- `ext/session/tests/session_decode_variation3.phpt`
- `ext/session/tests/session_decode_variation4.phpt`
- `ext/session/tests/session_encode_basic.phpt`
- `ext/session/tests/session_encode_error2.phpt`
- `ext/session/tests/session_encode_serialize.phpt`
- `ext/session/tests/session_encode_variation1.phpt`
- `ext/session/tests/session_encode_variation2.phpt`
- `ext/session/tests/session_encode_variation3.phpt`
- `ext/session/tests/session_encode_variation4.phpt`
- `ext/session/tests/session_encode_variation5.phpt`
- `ext/session/tests/session_encode_variation6.phpt`
- `ext/session/tests/session_encode_variation7.phpt`
- `ext/session/tests/session_encode_variation8.phpt`
- `ext/session/tests/session_gc_basic.phpt`
- `ext/session/tests/session_gc_probability_ini.phpt`
- `ext/session/tests/session_get_cookie_params_basic.phpt`
- `ext/session/tests/session_get_cookie_params_variation1.phpt`
- `ext/session/tests/gh16590.phpt`
- `ext/session/tests/session_id_basic.phpt`
- `ext/session/tests/session_id_basic2.phpt`
- `ext/session/tests/session_id_error2.phpt`
- `ext/session/tests/session_id_error3.phpt` (SKIP: non-UTF8 PHPT source
  tracked as a runner malformed-or-non-UTF8 gap)
- `ext/session/tests/session_ini_set.phpt`
- `ext/session/tests/session_module_name_basic.phpt`
- `ext/session/tests/session_module_name_variation1.phpt`
- `ext/session/tests/session_name_basic.phpt`
- `ext/session/tests/session_name_variation1.phpt`
- `ext/session/tests/session_name_variation2.phpt`
- `ext/session/tests/session_name_variation_null_byte.phpt`
- `ext/session/tests/session_regenerate_id_basic.phpt`
- `ext/session/tests/session_reset_basic.phpt`
- `ext/session/tests/session_save_path_basic.phpt`
- `ext/session/tests/session_save_path_variation2.phpt`
- `ext/session/tests/session_save_path_variation3.phpt`
- `ext/session/tests/session_save_path_variation4.phpt`
- `ext/session/tests/session_save_path_variation5.phpt`
- `ext/session/tests/session_set_cookie_params_basic.phpt`
- `ext/session/tests/session_set_cookie_params_variation1.phpt`
- `ext/session/tests/session_set_cookie_params_variation2.phpt`
- `ext/session/tests/session_set_cookie_params_variation3.phpt`
- `ext/session/tests/session_set_cookie_params_variation4.phpt`
- `ext/session/tests/session_set_cookie_params_variation5.phpt`
- `ext/session/tests/session_set_cookie_params_variation6.phpt`
- `ext/session/tests/session_set_cookie_params_variation7.phpt`
- `ext/session/tests/session_set_cookie_params_variation8.phpt`
- `ext/session/tests/session_start_error.phpt`
- `ext/session/tests/session_start_read_and_close.phpt`
- `ext/session/tests/session_start_variation1.phpt`
- `ext/session/tests/session_start_variation2.phpt`
- `ext/session/tests/session_start_variation3.phpt`
- `ext/session/tests/session_start_variation4.phpt`
- `ext/session/tests/session_start_variation5.phpt`
- `ext/session/tests/session_start_variation6.phpt`
- `ext/session/tests/session_start_variation7.phpt`
- `ext/session/tests/session_start_variation8.phpt`
- `ext/session/tests/session_start_variation9.phpt`
- `ext/session/tests/session_status.phpt`
- `ext/session/tests/session_unset_basic.phpt`
- `ext/session/tests/session_unset_variation1.phpt`
- `ext/session/tests/session_write_close_basic.phpt`
- `ext/session/tests/session_write_close_variation1.phpt`
- `ext/session/tests/session_write_close_variation2.phpt`
- `ext/session/tests/session_write_close_variation3.phpt`
- `ext/session/tests/session_write_close_variation4.phpt`
- `ext/session/tests/bug80774.phpt`
- `tests/phpt/generated/session/platform-checks.phpt`

## Remaining Gaps

- Stable ID: `PHPT-SESSION-CLI-MVP-GAPS`
- Reference behavior: PHP with `session` enabled includes web SAPI lifecycle,
  cookie headers, file-backed storage, serializers, complete INI
  configuration, serialized references and complete object serialization,
  custom save handlers, file cleanup/garbage collection, locking, and
  `SessionHandler` classes/interfaces.
- Current phrust behavior: request-local CLI session basics pass through
  generated coverage; the web server covers `PHPSESSID` cookie reuse, creation,
  destroy, and process-local persistence across requests. Selected
  cache/module/save-path metadata, module-name validation, reserved user-handler
  rejection, files handler missing-save-path startup/read failure warnings,
  selected save-path open_basedir warnings,
  startup `session.auto_start`, status/destroy/write-close/
  commit/start lifecycle return values, warnings, `$_SESSION`
  reset/preservation behavior, start option validation/
  read-and-close handling, cache INI mutation, cookie parameter readback,
  scalar/array cookie parameter mutation, selected serializer encode/decode
  behavior including basic upstream encode/decode, decode error handling,
  invalid serializer `ini_set()` rejection, `serialize_precision` float
  payloads, and shared top-level plus recursive session `R:n` references,
  abort/reset committed-snapshot behavior, session_unset live-data clearing,
  and selected session_gc return/warning behavior are covered.
  Cross-process/file-backed persistence, custom handlers, upload lifecycle,
  cookie header emission from INI policy, lazy-write persistence behavior,
  persistent-store cleanup/garbage collection, secure server-mode generated ID
  entropy, broader serialized reference and object payload parity, and
  the full session handler matrix remain unsupported.
- Fixture: `tests/phpt/generated/session/platform-checks.phpt`
- Next owner layer: future request/runtime state work for filesystem-backed
  persistence, INI policy, and handler objects.

## Non-Scope

- Full web SAPI lifecycle outside the in-process server
- uploads/request lifecycle
- file-backed or cross-process persistence and locking
- persistent-store cleanup/garbage collection
- broader serialized references and complete object serialization in
  session payloads
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
- `env PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 PHPT_TIMEOUT_SECONDS=10 nix develop -c just phpt-module-target MODULE=session FILE=ext/session/tests/session_get_cookie_params_variation1.phpt`
- `env PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 PHPT_TIMEOUT_SECONDS=10 nix develop -c just phpt-module-target MODULE=session FILE=ext/session/tests/session_cache_expire_variation3.phpt`
- `env PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 PHPT_TIMEOUT_SECONDS=10 nix develop -c just phpt-module-target MODULE=session FILE=ext/session/tests/session_cache_limiter_variation3.phpt`
- `env PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 PHPT_TIMEOUT_SECONDS=10 nix develop -c just phpt-module-target MODULE=session FILE=ext/session/tests/session_set_cookie_params_basic.phpt`
- `env PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 PHPT_TIMEOUT_SECONDS=10 nix develop -c just phpt-module-target MODULE=session FILE=ext/session/tests/session_set_cookie_params_variation8.phpt`
- `env PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src PHPT_REUSE_LAST=0 PHPT_TIMEOUT_SECONDS=10 nix develop -c just phpt-dev-fast MODULE=session FILE=ext/session/tests/session_abort_basic.phpt`
- `env PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src PHPT_REUSE_LAST=0 PHPT_TIMEOUT_SECONDS=10 nix develop -c just phpt-dev-fast MODULE=session FILE=ext/session/tests/session_reset_basic.phpt`
- `env PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src PHPT_REUSE_LAST=0 PHPT_TIMEOUT_SECONDS=10 nix develop -c just phpt-dev-fast MODULE=session FILE=ext/session/tests/session_unset_basic.phpt`
- `env PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src PHPT_REUSE_LAST=0 PHPT_TIMEOUT_SECONDS=10 nix develop -c just phpt-dev-fast MODULE=session FILE=ext/session/tests/session_unset_variation1.phpt`
- `env PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src PHPT_REUSE_LAST=0 PHPT_TIMEOUT_SECONDS=10 nix develop -c just phpt-dev-fast MODULE=session FILE=ext/session/tests/session_gc_basic.phpt`
- `env PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src PHPT_REUSE_LAST=0 PHPT_TIMEOUT_SECONDS=10 nix develop -c just phpt-dev-fast MODULE=session FILE=ext/session/tests/session_gc_probability_ini.phpt`
- `env PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 PHPT_DISABLE_REFERENCE_REUSE=1 PHPT_TIMEOUT_SECONDS=10 nix develop -c just phpt-dev-module MODULE=session`
- `nix develop -c just phpt-dev-module MODULE=session`
- `nix develop -c just verify-stdlib` if runtime code changes
- `nix develop -c just verify-phpt`
