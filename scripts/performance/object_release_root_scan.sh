#!/usr/bin/env bash
set -euo pipefail

out_dir="target/performance/object-release-root-scan"
build_target_dir="${CARGO_TARGET_DIR:-target}"
vm="$build_target_dir/debug/php-vm"

mkdir -p "$out_dir"
cargo build -p php_vm_cli --bin php-vm --no-default-features --features runtime-telemetry

"$vm" run \
    --counters-json "$out_dir/unrooted-counters.json" \
    fixtures/runtime_semantics/destructors/early-unrooted-release.php \
    >"$out_dir/unrooted-output.txt"

printf 'before\ndestruct\nafter\n' | cmp -s - "$out_dir/unrooted-output.txt" || {
    printf '%s\n' '[fail] early unrooted object release changed PHP-visible order' >&2
    exit 1
}

jq -e '
    .schema_version == 11 and
    .runtime_helper_calls_by_id.value_release > 0 and
    .runtime_helper_object_release_fast_paths > 0 and
    .runtime_helper_object_release_root_scans == 0
' "$out_dir/unrooted-counters.json" >/dev/null || {
    printf '%s\n' '[fail] unrooted object release did not use the scan-free path' >&2
    exit 1
}

"$vm" run \
    --counters-json "$out_dir/rooted-counters.json" \
    fixtures/runtime_semantics/destructors/rooted-release-scan.php \
    >"$out_dir/rooted-output.txt"

printf 'rooted\nafter\n' | cmp -s - "$out_dir/rooted-output.txt" || {
    printf '%s\n' '[fail] rooted object release changed PHP-visible output' >&2
    exit 1
}

jq -e '
    .schema_version == 11 and
    .runtime_helper_calls_by_id.value_release > 0 and
    .runtime_helper_object_release_root_scans > 0
' "$out_dir/rooted-counters.json" >/dev/null || {
    printf '%s\n' '[fail] rooted object release did not exercise the request-root scan' >&2
    exit 1
}

printf '%s\n' '[pass] object release preserves output and exercises the clone-free root scan'
