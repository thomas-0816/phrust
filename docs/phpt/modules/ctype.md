# ctype PHPT module status

## Scope

- Complete selected PHP 8.5.7 `ext/ctype` PHPT target set.
- ASCII C-locale byte classification for `ctype_alnum`, `ctype_alpha`,
  `ctype_cntrl`, `ctype_digit`, `ctype_graph`, `ctype_lower`,
  `ctype_print`, `ctype_punct`, `ctype_space`, `ctype_upper`, and
  `ctype_xdigit`.
- PHP 8.5 legacy non-string behavior for integer fallbacks, false-returning
  non-strings, and deprecation diagnostics.

## Non-scope

- Host locale behavior when the local oracle/runtime environment cannot provide
  the requested locale.

## Selected tests

- `tests/phpt/generated/ctype/basic.phpt`
- `tests/phpt/generated/ctype/fallbacks.phpt`
- Full selected upstream `ext/ctype/tests/*.phpt` set listed in
  `tests/phpt/manifests/modules/ctype.selected.jsonl`.

## Verification

- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 PHPT_DISABLE_REFERENCE_REUSE=1 PHPT_TIMEOUT_SECONDS=20 PHPT_WORK_DIR=/private/tmp/phrust-phpt-ctype-selected-full nix develop -c just phpt-dev-module MODULE=ctype`
  - Reference: SKIP 51 because the local oracle build does not load `ctype`.
  - Target: PASS 50, SKIP 1 for the host-locale-dependent row, non-green 0.
  - php-src manifest integrity: verified 24475 entries, skipped 0
    host-generated entries.
- `nix develop -c cargo test -q -p php_runtime builtins::modules::ctype::tests`
  - PASS: 4 tests.
