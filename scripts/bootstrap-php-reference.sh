#!/usr/bin/env bash
set -euo pipefail

PHP_REF_REPO="${PHP_REF_REPO:-https://github.com/php/php-src.git}"
PHP_REF_TAG="${PHP_REF_TAG:-php-8.5.7}"
PHP_REF_SERIES="${PHP_REF_SERIES:-8.5}"
PHP_REF_VERSION="${PHP_REF_VERSION:-8.5.7}"
PHP_REF_DIR="${PHP_REF_DIR:-third_party/php-src}"
LOCKFILE="${PHP_REF_LOCKFILE:-references/php-src.lock.toml}"

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
  exit 1
}

if [[ ! -d "$PHP_REF_DIR" ]]; then
  mkdir -p "$(dirname "$PHP_REF_DIR")"
  git clone --depth 1 --branch "$PHP_REF_TAG" "$PHP_REF_REPO" "$PHP_REF_DIR"
else
  [[ -d "$PHP_REF_DIR/.git" ]] || fail "$PHP_REF_DIR exists but is not a Git repository"
  git -C "$PHP_REF_DIR" fetch --depth 1 origin "tag $PHP_REF_TAG"
  git -C "$PHP_REF_DIR" checkout --detach "$PHP_REF_TAG"
fi

commit="$(git -C "$PHP_REF_DIR" rev-parse HEAD)"
resolved_at_utc="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"

for path in "${critical_files[@]}"; do
  [[ -f "$PHP_REF_DIR/$path" ]] || fail "missing critical reference file: $path"
done

mkdir -p "$(dirname "$LOCKFILE")"
cat >"$LOCKFILE" <<EOF_LOCK
[php]
series = "$PHP_REF_SERIES"
version = "$PHP_REF_VERSION"
tag = "$PHP_REF_TAG"
repository = "$PHP_REF_REPO"
commit = "$commit"
resolved_at_utc = "$resolved_at_utc"

[paths]
local_checkout = "$PHP_REF_DIR"

[critical_files]
scanner = "Zend/zend_language_scanner.l"
parser = "Zend/zend_language_parser.y"
vm_def = "Zend/zend_vm_def.h"
ast = "Zend/zend_ast.h"
compile = "Zend/zend_compile.h"
types = "Zend/zend_types.h"
EOF_LOCK

printf 'PHP reference bootstrapped:\n'
printf '  repo: %s\n' "$PHP_REF_REPO"
printf '  tag: %s\n' "$PHP_REF_TAG"
printf '  commit: %s\n' "$commit"
printf '  lockfile: %s\n' "$LOCKFILE"
