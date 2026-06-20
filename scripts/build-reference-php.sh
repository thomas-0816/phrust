#!/usr/bin/env bash
set -euo pipefail

PHP_REF_DIR="${PHP_REF_DIR:-third_party/php-src}"
LOG_DIR="${PHP_REF_BUILD_LOG_DIR:-.cache/php-ref-build}"

detect_jobs() {
  if [[ -n "${PHP_REF_BUILD_JOBS:-}" ]]; then
    printf '%s\n' "$PHP_REF_BUILD_JOBS"
  elif command -v nproc >/dev/null 2>&1; then
    nproc
  elif command -v sysctl >/dev/null 2>&1; then
    sysctl -n hw.ncpu
  else
    printf '2\n'
  fi
}

fail() {
  printf 'error: %s\n' "$*" >&2
  printf 'hint: run `nix develop -c just bootstrap-ref`\n' >&2
  exit 1
}

[[ -d "$PHP_REF_DIR" ]] || fail "missing PHP reference checkout: $PHP_REF_DIR"
[[ -f "$PHP_REF_DIR/buildconf" ]] || fail "missing buildconf in PHP reference checkout"

jobs="$(detect_jobs)"
mkdir -p "$LOG_DIR"

pushd "$PHP_REF_DIR" >/dev/null

if [[ ! -x ./configure || "${FORCE_BUILDCONF:-0}" == "1" ]]; then
  ./buildconf --force 2>&1 | tee "../../$LOG_DIR/buildconf.log"
fi

./configure \
  --disable-all \
  --enable-cli \
  --enable-tokenizer \
  --enable-debug \
  2>&1 | tee "../../$LOG_DIR/configure.log"

make -j"$jobs" 2>&1 | tee "../../$LOG_DIR/make.log"

test -x sapi/cli/php
sapi/cli/php -v
sapi/cli/php -m
sapi/cli/php -r 'echo PHP_VERSION, "\n";'
sapi/cli/php -r 'var_export(function_exists("token_get_all")); echo "\n";'

popd >/dev/null

printf 'reference PHP CLI build complete: %s/sapi/cli/php\n' "$PHP_REF_DIR"
