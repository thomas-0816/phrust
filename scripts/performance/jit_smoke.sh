#!/usr/bin/env bash
set -euo pipefail

OUT_DIR="target/performance"
OUT_JSON="$OUT_DIR/jit-smoke.json"
FIXTURES_DIR="tests/fixtures/performance/jit"
BUILD_TARGET_DIR="${PHRUST_JIT_SMOKE_TARGET_DIR:-${CARGO_TARGET_DIR:-target}}"
VM="$BUILD_TARGET_DIR/debug/php-vm"

mkdir -p "$OUT_DIR"

cargo test -p php_jit
cargo test -p php_jit --features jit-cranelift
cargo test -p php_vm --no-default-features --features jit-cranelift jit_
cargo test -p php_vm --no-default-features --features jit-cranelift cranelift_
cargo test -p php_vm_cli
cargo build -p php_vm_cli --bin php-vm --no-default-features --features jit-cranelift,runtime-telemetry

# This smoke proves the Cranelift tier specifically compiles and executes the
# integer fixture.

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
import os
import platform
from pathlib import Path

hot = json.loads(Path("target/performance/jit-cranelift-counters.json").read_text())
rejected = json.loads(Path("target/performance/jit-rejected-counters.json").read_text())
machine = platform.machine().lower()
if os.environ.get("PHRUST_REQUIRE_AMD64") == "1" and machine not in {"x86_64", "amd64"}:
    raise SystemExit(f"[fail] jit-smoke-amd64 requires x86_64/amd64, got {machine}")
if hot.get("jit_mode") != "cranelift":
    raise SystemExit(f"[fail] expected jit_mode=cranelift, got {hot.get('jit_mode')!r}")
if hot.get("jit_compile_attempts", 0) <= 0:
    raise SystemExit("[fail] jit=cranelift recorded no compile attempts")
if hot.get("jit_compiled", 0) <= 0:
    raise SystemExit("[fail] jit=cranelift recorded no compiled functions")
if hot.get("jit_executed", 0) <= 0:
    raise SystemExit("[fail] jit=cranelift recorded no executions")
if hot.get("jit_bailouts", 0) != 0:
    raise SystemExit("[fail] int leaf jit path bailed out")
if hot.get("native_platform_unavailable", 0) != 0:
    raise SystemExit("[fail] Cranelift reported native platform unavailable")
if hot.get("jit_process_cache_misses", 0) <= 0:
    raise SystemExit("[fail] Cranelift smoke did not publish through the process code manager")
if hot.get("jit_code_bytes_live", 0) <= 0 or hot.get("jit_code_generations", 0) <= 0:
    raise SystemExit("[fail] Cranelift code-manager ownership gauges are empty")
if rejected.get("jit_compile_attempts", 0) <= 0:
    raise SystemExit("[fail] rejected function recorded no compile attempt")
if rejected.get("jit_compiled", 0) != 0 or rejected.get("jit_executed", 0) != 0:
    raise SystemExit("[fail] rejected function should not compile or execute")
if rejected.get("jit_bailouts", 0) <= 0:
    raise SystemExit("[fail] rejected function recorded no bailout")
descriptors = hot.get("jit_compile_descriptors") or []
if not descriptors:
    raise SystemExit("[fail] native compile did not report an ISA/ABI descriptor")
descriptor = descriptors[0]
for field in ("target_isa", "abi_hash", "config_hash"):
    if descriptor.get(field) in (None, ""):
        raise SystemExit(f"[fail] native compile descriptor missing {field}")
report = {
    "gate": "jit-smoke-amd64" if os.environ.get("PHRUST_REQUIRE_AMD64") == "1" else "jit-smoke",
    "status": "passed",
    "platform_machine": machine,
    "default_feature_tests": "passed",
    "jit_cranelift_feature_tests": "passed",
    "eligibility_analysis": "passed",
    "abi_boundary_tests": "passed",
    "cranelift_lowering_smoke": "passed",
    "jit_execution_smoke": "passed",
    "jit_ab_output_identical": "passed",
    "jit_fallback_smoke": "passed",
    "jit_mode": hot["jit_mode"],
    "jit_compiled": hot["jit_compiled"],
    "jit_executed": hot["jit_executed"],
    "native_platform_unavailable": hot["native_platform_unavailable"],
    "process_code_manager": {
        field: hot.get(field, 0)
        for field in (
            "jit_process_cache_hits",
            "jit_process_cache_misses",
            "jit_compile_waits",
            "jit_duplicate_compiles_avoided",
            "jit_code_bytes_live",
            "jit_code_bytes_retired",
            "jit_code_generations",
            "jit_evictions",
        )
    },
    "native_compile_descriptor": descriptor,
    "native_machine_code_execution": True,
    "executable_memory_required": True,
}
Path("target/performance/jit-smoke.json").write_text(
    json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
)
PY

printf '%s\n' '[pass] performance jit smoke: default-off API, Cranelift lowering, guarded int-leaf execution, A/B output, and fallback passed'
