#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

work_dir="${EXTENSION_SURFACE_VERIFY_WORK_DIR:-target/stdlib/generated-extension-surfaces-verify}"
rm -rf "$work_dir"
mkdir -p "$work_dir"

scripts/stdlib/generate_extension_surfaces.py \
  --schema-dir fixtures/stdlib/extensions \
  --arginfo crates/php_std/src/generated/arginfo.rs \
  --out-root "$work_dir"
find "$work_dir/crates" -name '*.rs' -print0 | xargs -0 rustfmt --edition 2024

paths=(
  crates/php_std/src/generated/extensions
  crates/php_runtime/src/builtins/generated
  crates/php_extensions/src/generated.rs
)
for path in "${paths[@]}"; do
  if ! diff -ruN "$path" "$work_dir/$path"; then
    printf '%s\n' "generated extension surface drift detected: $path" >&2
    printf '%s\n' \
      "Regenerate with: nix develop -c just generate-extension-surfaces" >&2
    exit 1
  fi
done

printf '%s\n' '[ok] generated extension surfaces match canonical descriptors'
