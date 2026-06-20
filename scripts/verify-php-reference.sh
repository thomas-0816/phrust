#!/usr/bin/env bash
set -euo pipefail

LOCKFILE="${PHP_REF_LOCKFILE:-references/php-src.lock.toml}"
PHP_REF_DIR="${PHP_REF_DIR:-third_party/php-src}"

critical_files=(
  "Zend/zend_language_scanner.l"
  "Zend/zend_language_parser.y"
  "Zend/zend_vm_def.h"
  "Zend/zend_ast.h"
  "Zend/zend_compile.h"
  "Zend/zend_types.h"
)

fail() {
  printf 'error: %s\n' "$*" >&2
  printf 'hint: run `nix develop -c just bootstrap-ref`\n' >&2
  exit 1
}

[[ -f "$LOCKFILE" ]] || fail "missing PHP reference lockfile: $LOCKFILE"
[[ -d "$PHP_REF_DIR/.git" ]] || fail "missing PHP reference checkout: $PHP_REF_DIR"

expected_commit="$(
  awk -F ' *= *' '/^commit = / {
    gsub(/"/, "", $2);
    print $2;
    exit
  }' "$LOCKFILE"
)"
[[ -n "$expected_commit" ]] || fail "lockfile does not contain a commit"

actual_commit="$(git -C "$PHP_REF_DIR" rev-parse HEAD)"
if [[ "$actual_commit" != "$expected_commit" ]]; then
  fail "reference commit mismatch: lockfile has $expected_commit, checkout has $actual_commit"
fi

for path in "${critical_files[@]}"; do
  [[ -f "$PHP_REF_DIR/$path" ]] || fail "missing critical reference file: $path"
done

if [[ -x "$PHP_REF_DIR/sapi/cli/php" ]]; then
  php_version="$("$PHP_REF_DIR/sapi/cli/php" -r 'echo PHP_VERSION;')"
  if [[ "$php_version" != 8.5.7* ]]; then
    fail "reference PHP CLI version is $php_version, expected 8.5.7"
  fi
  tokenizer_available="$("$PHP_REF_DIR/sapi/cli/php" -r 'var_export(function_exists("token_get_all"));')"
  if [[ "$tokenizer_available" != "true" ]]; then
    fail "reference PHP CLI does not provide token_get_all"
  fi
  printf '  cli: %s/sapi/cli/php (%s, token_get_all available)\n' "$PHP_REF_DIR" "$php_version"
else
  printf 'info: reference PHP CLI not built; run `nix develop -c just build-ref-php` if needed.\n'
fi

printf 'PHP reference verification passed:\n'
printf '  checkout: %s\n' "$PHP_REF_DIR"
printf '  commit: %s\n' "$actual_commit"
