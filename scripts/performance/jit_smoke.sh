#!/usr/bin/env bash
set -euo pipefail

OUT_DIR="target/performance"
OUT_JSON="$OUT_DIR/jit-smoke.json"
FIXTURES_DIR="tests/fixtures/performance/jit"
VM="target/debug/php-vm"

mkdir -p "$OUT_DIR"

cargo test -p php_jit
cargo test -p php_jit --features jit-cranelift
cargo test -p php_vm --features jit-cranelift jit_
cargo test -p php_vm_cli
cargo build -p php_vm_cli --bin php-vm --features jit-cranelift

# This smoke proves the Cranelift tier specifically compiles and executes the
# int leaf. The copy-patch leaf tier (default-on) would otherwise claim the
# leaf before Cranelift tiering ever sees it, so isolate it for these runs.
export PHRUST_JIT_COPY_PATCH=0

"$VM" run --jit=off "$FIXTURES_DIR/int-leaf-hot-loop.php" >"$OUT_DIR/jit-off.out"
"$VM" run --exec-format=ir --jit=cranelift --counters-json "$OUT_DIR/jit-cranelift-counters.json" "$FIXTURES_DIR/int-leaf-hot-loop.php" >"$OUT_DIR/jit-cranelift.out"
diff -u "$OUT_DIR/jit-off.out" "$OUT_DIR/jit-cranelift.out"
"$VM" run --exec-format=ir --jit=cranelift --counters-json "$OUT_DIR/jit-rejected-counters.json" "$FIXTURES_DIR/rejected-fallback.php" >"$OUT_DIR/jit-rejected.out"
printf '90\n' >"$OUT_DIR/jit-expected.out"
printf '32\n' >"$OUT_DIR/jit-rejected-expected.out"
diff -u "$OUT_DIR/jit-expected.out" "$OUT_DIR/jit-cranelift.out"
diff -u "$OUT_DIR/jit-rejected-expected.out" "$OUT_DIR/jit-rejected.out"

python3 - <<'PY'
import json
from pathlib import Path

hot = json.loads(Path("target/performance/jit-cranelift-counters.json").read_text())
rejected = json.loads(Path("target/performance/jit-rejected-counters.json").read_text())
if hot.get("jit_compile_attempts", 0) <= 0:
    raise SystemExit("[fail] jit=cranelift recorded no compile attempts")
if hot.get("jit_compiled", 0) <= 0:
    raise SystemExit("[fail] jit=cranelift recorded no compiled functions")
if hot.get("jit_executed", 0) <= 0:
    raise SystemExit("[fail] jit=cranelift recorded no executions")
if hot.get("jit_bailouts", 0) != 0:
    raise SystemExit("[fail] int leaf jit path bailed out")
if rejected.get("jit_compile_attempts", 0) <= 0:
    raise SystemExit("[fail] rejected function recorded no compile attempt")
if rejected.get("jit_compiled", 0) != 0 or rejected.get("jit_executed", 0) != 0:
    raise SystemExit("[fail] rejected function should not compile or execute")
if rejected.get("jit_bailouts", 0) <= 0:
    raise SystemExit("[fail] rejected function recorded no bailout")
PY

cat >"$OUT_JSON" <<'JSON'
{
  "gate": "jit-smoke",
  "status": "passed",
  "default_feature_tests": "passed",
  "jit_cranelift_feature_tests": "passed",
  "eligibility_analysis": "passed",
  "abi_boundary_tests": "passed",
  "cranelift_lowering_smoke": "passed",
  "jit_execution_smoke": "passed",
  "jit_ab_output_identical": "passed",
  "jit_fallback_smoke": "passed",
  "native_machine_code_execution": false,
  "executable_memory_required": false
}
JSON

printf '%s\n' '[pass] performance jit smoke: default-off API, Cranelift lowering, guarded int-leaf execution, A/B output, and fallback passed'
