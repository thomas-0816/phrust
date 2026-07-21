#!/usr/bin/env bash
set -euo pipefail

OUT_DIR="target/performance"
BUILD_TARGET_DIR="${PHRUST_NATIVE_SMOKE_TARGET_DIR:-${CARGO_TARGET_DIR:-target}}"
VM="$BUILD_TARGET_DIR/debug/php-vm"

mkdir -p "$OUT_DIR"

cargo test -p php_jit
cargo test -p php_vm native_entry
cargo build -p php_vm_cli --bin php-vm --no-default-features --features runtime-telemetry

# Exercise a fixture that was once rejected by the partial native compiler and
# now covers calls, arrays, loops, and a builtin through production Cranelift.
counters="$OUT_DIR/native-smoke-counters.json"
"$VM" run --counters-json "$counters" \
    tests/fixtures/performance/jit/rejected-fallback.php \
    >"$OUT_DIR/native-smoke.out"
printf '32\n' | cmp -s - "$OUT_DIR/native-smoke.out" || {
    printf '%s\n' '[fail] native smoke output differs from the PHP fixture contract' >&2
    exit 1
}
jq -e '
  .schema_version == 13 and
  .native_execution_entries > 0 and
  ((.native_compile_successes + .native_cache_hits) > 0)
' "$counters" >/dev/null

# Cross-unit calls share one request value arena. Warm calls retain existing
# immutable/object handles and transfer the owned return handle instead of
# cloning both sides through decode/encode. Once the included signature is
# published, by-value lvalues must also avoid speculative ReferenceCells. The
# loop turns either regression into a deterministic high-water failure.
transfer_counters="$OUT_DIR/native-cross-unit-transfer-counters.json"
transfer_output="$OUT_DIR/native-cross-unit-transfer.out"
"$VM" run --counters-json "$transfer_counters" \
    tests/fixtures/performance/native_tier/cross_unit_handle_transfer.php \
    >"$transfer_output"
cmp -s \
    tests/fixtures/performance/native_tier/cross_unit_handle_transfer.php.out \
    "$transfer_output" || {
    printf '%s\n' '[fail] native cross-unit transfer output differs from PHP' >&2
    exit 1
}
jq -e '
  .native_cross_unit_direct_executed > 0 and
  .native_value_table_allocations < 1200 and
  .native_value_table_high_water < 1200 and
  .native_value_table_reuses < 100
' "$transfer_counters" >/dev/null || {
    printf '%s\n' '[fail] native cross-unit handle transfer exceeded its arena budget' >&2
    exit 1
}

# Positional by-value builtins call the stable-ID helper directly. The former
# generic call-frame path wrote 512000 bytes for this fixture; the direct ABI
# publishes only the 32000 bytes of actual i64 arguments.
compact_builtin_counters="$OUT_DIR/native-compact-builtin-counters.json"
compact_builtin_output="$OUT_DIR/native-compact-builtin.out"
"$VM" run --counters-json "$compact_builtin_counters" \
    tests/fixtures/performance/native_tier/compact_builtin_arguments.php \
    >"$compact_builtin_output"
cmp -s \
    tests/fixtures/performance/native_tier/compact_builtin_arguments.php.out \
    "$compact_builtin_output" || {
    printf '%s\n' '[fail] compact builtin argument output differs from PHP' >&2
    exit 1
}
jq -e '
  .native_builtin_direct_executed == 3000 and
  .native_call_direct == 3000 and
  .native_callsite_total == 3000 and
  .runtime_helper_calls_by_id.string_predicate == 3000 and
  .native_call_frame_bytes <= 40000
' "$compact_builtin_counters" >/dev/null || {
    printf '%s\n' '[fail] direct builtin calls exceeded the compact frame budget' >&2
    exit 1
}

python3 - <<'PY'
import json
from pathlib import Path

report = {
    "gate": "native-smoke",
    "status": "passed",
    "compiler": "cranelift",
    "compiler_optional": False,
    "baseline_native_entry": "passed",
    "optimizing_native_entry": "passed",
    "native_execution_entries": "positive",
    "native_compile_or_cache_entries": "positive",
    "interpreter_fallback": "structurally unavailable",
}
Path("target/performance/native-smoke.json").write_text(
    json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
)
PY

printf '%s\n' '[pass] mandatory Cranelift native-entry and no-fallback smoke passed'
