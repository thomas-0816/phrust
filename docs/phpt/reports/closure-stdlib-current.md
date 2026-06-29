# Closure Standard Library Current Report

Reference oracle:
`/Volumes/CrucialMusic/src/phrust/third_party/php-src` with
`/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php`.

## Prompt Coverage

- 2.1: Added the `closure.stdlib` dashboard, module descriptor, selected
  manifest, and generated closure fixtures. The selected gate spans 49 PHPTs
  across the required config/env, output buffering, serialization, stream/stat,
  glob, JSON, PCRE, Date, SPL, Reflection, HTML, query, and formatting areas.
- 2.2: The selected PHPTs exercise the request-aware VM implementations for
  `error_reporting`, INI/config helpers, environment helpers, SAPI name,
  uname, memory usage, and `set_time_limit`.
- 2.3: The selected PHPTs exercise nested output-buffer stack state, capture,
  clean, flush, read length, and level behavior.
- 2.4: The selected PHPTs exercise implemented `serialize`, `unserialize`,
  `var_export`, `var_dump`, and `print_r` behavior. Reference records,
  `allowed_classes`, and magic serialization hooks remain explicit known gaps.
- 2.5: The selected PHPTs exercise local streams/filesystem, stat, glob,
  printf/sprintf, query-string helpers, URL helpers, and HTML escaping.
- 2.6: The selected PHPTs exercise JSON common flags, throw-on-error, PCRE
  captures/replacement/last-error state, Date/Time formatting/timezones, SPL
  iterator MVPs, and Reflection metadata.
- 2.7: The closure stdlib selected gate is the closeout gate for this cross-cut.
  Broader module gates remain owned by their existing manifests and known-gap
  docs.

## Existing Focused Gate Snapshot

The final focused module checks used the same oracle path and produced:

| Module | Target outcome |
| --- | --- |
| `closure.stdlib` | PASS, 49 PASS |
| `standard.output` | PASS, 11 PASS |
| `standard.strings` | PASS, 16 PASS |
| `standard.variables` | PASS, 27 PASS |
| `standard.serialization` | PASS, 5 PASS |
| `filesystem.streams` | PASS, 11 PASS |
| `json` | PASS, 10 PASS |
| `pcre` | PASS, 5 PASS |
| `date` | PASS, 7 PASS |
| `reflection` | PASS, 22 PASS |
| `spl` | Existing aggregate gate remains non-green: 18 PASS, 1 SKIP, 189 non-green broad upstream SPL cases outside the current MVP. |

## Final Validation

- `nix develop -c cargo test -p php_runtime`: PASS, 256 tests.
- `nix develop -c cargo test -p php_vm`: PASS, 472 tests.
- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src nix develop -c just phpt-dev-module MODULE=closure.stdlib`: PASS, 49 PHPTs, 0 non-green outcomes.
- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src nix develop -c just verify-stdlib`: PASS.
- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src nix develop -c just verify-phpt`: PASS.
- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src nix develop -c just verify-runtime`: PASS.

## Remaining Explicit Gaps

- Full upstream SPL aggregate parity, including advanced container, iterator,
  file, serialization, mutation, and exception edge cases.
- `unserialize` `allowed_classes`, incomplete classes, `__serialize`,
  `__unserialize`, legacy Serializable hooks, and PHP `R`/`r` reference records.
- Output-buffer callbacks, chunk sizes, handler flags, and handler-list APIs.
- Full network stream, PHAR, Zend extension ABI, and non-CLI SAPI behavior.
