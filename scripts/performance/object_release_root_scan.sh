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
    .schema_version == 12 and
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
    .schema_version == 12 and
    .runtime_helper_calls_by_id.value_release > 0 and
    .runtime_helper_object_release_fast_paths > 0 and
    .runtime_helper_object_release_root_scans == 0 and
    .runtime_helper_object_release_root_scans_by_reason.request_membership_traversal > 0
' "$out_dir/rooted-counters.json" >/dev/null || {
    printf '%s\n' '[fail] rooted object release did not use indexed request-root membership' >&2
    exit 1
}

if rg -q 'call_arguments: Vec<Vec<Value>>|push_call_arguments|pop_call_arguments' \
    crates/php_vm/src/vm/jit_abi.rs \
    crates/php_vm/src/vm/jit_abi/call_support.rs; then
    printf '%s\n' '[fail] warm userland calls still materialize a duplicate argument root stack' >&2
    exit 1
fi

rg -q 'fn live_native_values_contain_object' crates/php_vm/src/vm/jit_abi.rs || {
    printf '%s\n' '[fail] destructor release no longer scans already-live native handles on demand' >&2
    exit 1
}

rg -q 'Option<NativeInstructionPtr>' crates/php_vm/src/vm/jit_abi.rs || {
    printf '%s\n' '[fail] continuation lookup again clones owned IR instructions on the helper path' >&2
    exit 1
}

rg -q 'map\(std::sync::Arc::as_ptr\)' crates/php_vm/src/vm/jit_abi.rs || {
    printf '%s\n' '[fail] callsite lookup again performs an Arc clone/drop per warm dispatch' >&2
    exit 1
}

rg -q 'Option<(super::)?NativeFunctionMetadataPtr>' crates/php_vm/src/vm/jit_abi/request_state.rs || {
    printf '%s\n' '[fail] native frames again clone immutable function metadata per warm call' >&2
    exit 1
}

if rg -q 'let target_(name|params) = Arc::clone' \
    crates/php_vm/src/vm/jit_abi/call_support.rs; then
    printf '%s\n' '[fail] call binding again bumps immutable metadata refcounts per warm call' >&2
    exit 1
fi

rg -q 'called_classes: Vec<Arc<str>>' crates/php_vm/src/vm/jit_abi.rs || {
    printf '%s\n' '[fail] native method dispatch again allocates called-class strings per warm frame' >&2
    exit 1
}

if rg -q 'enter_native_call|NATIVE_CALL_DEPTH.with' \
    crates/php_vm/src/vm/jit_abi.rs \
    crates/php_vm/src/vm/jit_abi/call_support.rs; then
    printf '%s\n' '[fail] warm userland calls again perform duplicate TLS depth accounting' >&2
    exit 1
fi

printf '%s\n' '[pass] object release preserves output without per-call root or metadata clones'
