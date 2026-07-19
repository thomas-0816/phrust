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
  .schema_version == 12 and
  .native_execution_entries > 0 and
  ((.native_compile_successes + .native_cache_hits) > 0)
' "$counters" >/dev/null

# Cross-unit calls share one request value arena. Warm calls retain existing
# immutable/object handles and transfer the owned return handle instead of
# cloning both sides through decode/encode. The loop turns one extra transfer
# slot per invocation into a deterministic high-water regression.
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
  .native_value_table_allocations < 4200 and
  .native_value_table_high_water < 4200 and
  .native_value_table_reuses < 100
' "$transfer_counters" >/dev/null || {
    printf '%s\n' '[fail] native cross-unit handle transfer exceeded its arena budget' >&2
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
