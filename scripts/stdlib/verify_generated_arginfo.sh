#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

committed="crates/php_std/src/generated/arginfo.rs"
overrides="fixtures/stdlib/arginfo_overrides.txt"
work_dir="${ARGININFO_VERIFY_WORK_DIR:-target/stdlib/generated-arginfo-verify}"
generated="$work_dir/arginfo.rs"

resolve_php_src() {
  if [[ -n "${PHP_SRC_DIR:-}" ]]; then
    printf '%s\n' "$PHP_SRC_DIR"
    return
  fi
  if [[ -d "third_party/php-src-8.5.7" ]]; then
    printf '%s\n' "third_party/php-src-8.5.7"
    return
  fi
  printf '%s\n' "third_party/php-src"
}

php_src="$(resolve_php_src)"

if [[ ! -d "$php_src" ]]; then
  cat >&2 <<EOF
generated arginfo verification requires the pinned php-src checkout.

Looked for: $php_src
Set PHP_SRC_DIR=/path/to/php-src or bootstrap the local reference checkout before running:

  nix develop -c just verify-generated-arginfo
EOF
  exit 1
fi

if [[ -z "$(find "$php_src" -name '*.stub.php' -print -quit)" ]]; then
  cat >&2 <<EOF
generated arginfo verification requires a php-src tree with *.stub.php files.

Path did not contain stub files: $php_src
Set PHP_SRC_DIR to the pinned PHP 8.5.7 php-src checkout.
EOF
  exit 1
fi

rm -rf "$work_dir"
mkdir -p "$work_dir"

scripts/stdlib/generate_arginfo.py \
  --php-src "$php_src" \
  --overrides "$overrides" \
  --out "$generated"
rustfmt --edition 2024 "$generated"

if ! diff -u "$committed" "$generated"; then
  cat >&2 <<EOF
generated arginfo drift detected.

Committed snapshot: $committed
Regenerated file:   $generated

Regenerate from the pinned php-src checkout with:

  nix develop -c just generate-arginfo php_src="$php_src"
  nix develop -c just verify-generated-arginfo
EOF
  exit 1
fi

printf '%s\n' "[ok] generated arginfo matches committed snapshot"
