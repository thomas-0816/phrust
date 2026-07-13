#!/usr/bin/env bash
set -euo pipefail

OUT_DIR="target/performance"
BUILD_TARGET_DIR="${PHRUST_JIT_SMOKE_TARGET_DIR:-${CARGO_TARGET_DIR:-target}}"
VM="$BUILD_TARGET_DIR/debug/php-vm"

mkdir -p "$OUT_DIR"

cargo test -p php_jit
cargo test -p php_vm native_entry
cargo build -p php_vm_cli --bin php-vm --no-default-features --features runtime-telemetry
scripts/verify/mandatory_cranelift.py

# Product execution has no managed fallback. This broad PHP fixture is not yet
# covered by the cutover compiler, so setup must fail with the precise native
# lowering diagnostic instead of producing interpreter output.
set +e
"$VM" run tests/fixtures/performance/jit/rejected-fallback.php \
    >"$OUT_DIR/native-only-rejected.out" \
    2>"$OUT_DIR/native-only-rejected.err"
exit_code=$?
set -e
if [[ "$exit_code" -eq 0 ]]; then
    printf '%s\n' '[fail] unsupported product fixture unexpectedly executed' >&2
    exit 1
fi
rg 'E_NATIVE_UNSUPPORTED_LOWERING.*instruction_kind=.*span=' \
    "$OUT_DIR/native-only-rejected.err" >/dev/null

python3 - <<'PY'
import json
from pathlib import Path

report = {
    "gate": "jit-smoke",
    "status": "passed",
    "compiler": "cranelift",
    "compiler_optional": False,
    "baseline_native_entry": "passed",
    "optimizing_native_entry": "passed",
    "unsupported_lowering_fallback": False,
}
Path("target/performance/jit-smoke.json").write_text(
    json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
)
PY

printf '%s\n' '[pass] mandatory Cranelift native-entry and no-fallback smoke passed'
