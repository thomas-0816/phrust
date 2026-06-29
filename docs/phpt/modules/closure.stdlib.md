# closure.stdlib

- Priority: 17
- Selected manifest: `tests/phpt/manifests/modules/closure.stdlib.selected.jsonl`
- Current selected scope: 49 PHPTs across config/env, output buffering,
  serialization/debug formatting, filesystem/stat/glob/streams, string/query/HTML
  helpers, JSON/PCRE/Date, SPL, and Reflection.

## Scope

- Request-aware helpers: `error_reporting`, `ini_get`, `ini_set`,
  `ini_get_all`, `get_cfg_var`, `getenv`, `putenv`, `php_sapi_name`,
  `php_uname`, `memory_get_usage`, `memory_get_peak_usage`, and
  `set_time_limit`
- Output buffering stack operations: `ob_start`, `ob_get_contents`,
  `ob_get_clean`, `ob_get_length`, `ob_get_level`, `ob_end_clean`,
  `ob_end_flush`, and `flush`
- Implemented serialization and debug-output helpers: `serialize`,
  `unserialize`, `var_export`, `var_dump`, and `print_r`
- Local filesystem, stream, stat, glob, query, HTML, JSON, PCRE, Date, selected
  SPL iterator, and Reflection metadata coverage

## Non-Scope

- FPM, CGI, Apache module behavior, phpdbg, and Zend extension ABI
- Network streams, stream filters, PHAR, and remote TLS wrappers
- `unserialize` `allowed_classes` enforcement, incomplete classes, and
  `__serialize`/`__unserialize` dispatch
- Full SPL aggregate upstream corpus and complete container/file/iterator edge
  parity

## Relevant PHPT Paths

- `tests/phpt/generated/closure.stdlib/*.phpt`
- `tests/phpt/generated/wp.core-builtins/*.phpt`
- `tests/phpt/generated/standard.output/*.phpt`
- `tests/phpt/generated/standard.serialization/*.phpt`
- `tests/phpt/generated/standard.variables/*.phpt`
- `tests/phpt/generated/filesystem.streams/*.phpt`
- `tests/phpt/generated/json/*.phpt`
- `tests/phpt/generated/pcre/*.phpt`
- `tests/phpt/generated/date/*.phpt`
- `tests/phpt/generated/spl.*/*.phpt`
- `tests/phpt/generated/reflection.functions/*.phpt`
- `tests/phpt/generated/reflection.parameters/*.phpt`
- `tests/phpt/generated/reflection.classes/*.phpt`

## Relevant Source Areas

- `crates/php_runtime/src/builtins/modules/core.rs`
- `crates/php_runtime/src/builtins/modules/{arrays,strings,filesystem,streams,json,pcre,date,spl,reflection}.rs`
- `crates/php_runtime/src/{serialization,resource,ini,output}.rs`
- `crates/php_vm/src/vm/mod.rs`
- `crates/php_std/src/arginfo.rs`
- `crates/php_std/src/generated/**`

## Target Gates

- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=closure.stdlib`
- `nix develop -c just verify-stdlib`
- `nix develop -c just verify-phpt`
- `nix develop -c just verify-runtime`

## Known Gaps

- `STDLIB-GAP-UNSERIALIZE-ALLOWED-CLASSES`
- `STDLIB-GAP-SERIALIZE-MAGIC-RESOURCE`
- `STDLIB-GAP-SERIALIZE-REFERENCES`
- `STDLIB-GAP-OUTPUT-BUFFER-CALLBACKS`
- `STDLIB-GAP-GLOB-ADVANCED`
- `STDLIB-GAP-STAT-BYTE-PERFECT`
- `STDLIB-GAP-JSON-FLAGS-BYTE-PERFECT`
- `STDLIB-GAP-PCRE-ADVANCED-FLAGS`
- `STDLIB-GAP-DATE-TIMELIB-PARITY`
- `STDLIB-GAP-SPL-ITERATOR-FULL-API`
- `STDLIB-GAP-SPL-CONTAINER-FULL-API`
- `STDLIB-GAP-REFLECTION-ARGINfo-PARITY`
