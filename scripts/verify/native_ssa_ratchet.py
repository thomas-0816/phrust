#!/usr/bin/env python3
"""Structural ratchet for executable SSA, scalar lowering, and root indexing."""

from __future__ import annotations

import sys
import subprocess
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


def read(relative: str) -> str:
    path = ROOT / relative
    if not path.is_file():
        raise FileNotFoundError(relative)
    return path.read_text(encoding="utf-8")


def main() -> int:
    failures: list[str] = []
    required = (
        "crates/php_jit/src/region_ir/ssa/mod.rs",
        "crates/php_jit/src/region_ir/value_flow.rs",
        "crates/php_jit/src/region_ir/ownership.rs",
        "crates/php_jit/src/region_ir/opt/executable.rs",
        "crates/php_jit/src/cranelift_lowering/value_lowering.rs",
        "crates/php_vm/src/vm/jit_abi/root_index.rs",
    )
    for relative in required:
        if not (ROOT / relative).is_file():
            failures.append(f"missing required executable SSA component: {relative}")

    try:
        lowering = read("crates/php_jit/src/cranelift_lowering.rs")
        executable = read("crates/php_jit/src/cranelift_lowering/executable_region.rs")
        tests = read("crates/php_jit/src/cranelift_lowering/tests.rs")
        root_index = read("crates/php_vm/src/vm/jit_abi/root_index.rs")
        runtime = read("crates/php_vm/src/vm/jit_abi.rs")
        runtime_ops = read("crates/php_vm/src/vm/jit_abi/runtime_ops.rs")
        optimizer = read("crates/php_jit/src/region_ir/opt/executable.rs")
        ownership = read("crates/php_jit/src/region_ir/ownership.rs")
        value_flow = read("crates/php_jit/src/region_ir/value_flow.rs")
    except FileNotFoundError as error:
        failures.append(f"cannot inspect missing file: {error}")
    else:
        checks = (
            (
                "crate::region_ir::analyze_executable_value_flow(candidate, &unit.constants)",
                executable,
                "optimizing lowering does not consume executable value-flow facts",
            ),
            (
                "optimize_executable_region(candidate)",
                executable,
                "optimizing policy does not transform authoritative RegionGraph code",
            ),
            (
                "local_storage(*local).is_promoted()",
                lowering,
                "plain-local helper bypass is absent",
            ),
            (
                "value_copy_requires_retain(fact)",
                lowering,
                "SSA copies do not consult ownership/lifecycle facts",
            ),
            (
                "value_flow.register_fact(register).ownership",
                lowering,
                "borrowed call operands can regress to consumed-argument releases",
            ),
            (
                "lower_direct_compare(",
                lowering,
                "exact scalar comparisons are not lowered directly",
            ),
            (
                "scalar_truthy(",
                lowering,
                "exact scalar truthiness is not lowered directly",
            ),
            (
                "optimizing_scalar_ssa_executes_without_local_truthy_or_lifecycle_helpers",
                tests,
                "no executable helper-elimination fixture protects the common path",
            ),
            (
                "collect_root_membership",
                root_index,
                "root membership is not indexed by reachable object identity",
            ),
            (
                "self.root_index.contains(object_id)",
                runtime,
                "object release does not use O(1) root membership",
            ),
            (
                "constants_folded",
                optimizer,
                "optimizer transformations remain report-only",
            ),
            (
                "dead_instructions",
                optimizer,
                "executable DCE is absent",
            ),
            (
                "loop_invariants_hoisted",
                optimizer,
                "executable LICM/GCM placement is absent",
            ),
            (
                "optimizing_owned_handle_moves_into_plain_local_without_refcount_pair",
                tests,
                "no executable last-use handle move fixture protects lifecycle elimination",
            ),
            (
                "optimizing_array_append_keeps_promoted_array_out_of_local_helpers",
                tests,
                "promoted array mutation can regress to local fetch/store helpers",
            ),
            (
                "flow.verify_ownership(candidate)",
                executable,
                "native code generation does not run the ownership verifier",
            ),
            (
                "ownership_verifier_rejects_use_after_forced_move",
                value_flow,
                "ownership verifier has no executable use-after-move rejection fixture",
            ),
            (
                "every_stable_helper_declares_an_ownership_contract",
                ownership,
                "new helpers can omit ownership effects",
            ),
        )
        for needle, text, message in checks:
            if needle not in text:
                failures.append(message)

        if "fn reaches_object(" in runtime:
            failures.append("whole-request per-object root search remains on release path")
        if "phrust_native_value_lifecycle" in lowering:
            failures.append("deleted generic retain/release helper returned")
        if re.search(
            r"lower_native_value_operation\(\s*module,\s*builder,\s*"
            r"(?:native_operations\.value_release|native_value_release_helper|lifecycle)",
            lowering,
        ):
            failures.append(
                "typed final release regressed through the opcode/out-pointer value adapter"
            )
        if "jit_native_value_lifecycle_abi" in runtime_ops:
            failures.append("deleted combined lifecycle runtime ABI returned")

    if not failures:
        commands = (
            ["cargo", "test", "-q", "-p", "php_jit", "--lib", "optimizing_"],
            ["cargo", "test", "-q", "-p", "php_vm", "--lib", "vm::jit_abi::root_index"],
        )
        for command in commands:
            completed = subprocess.run(command, cwd=ROOT, check=False)
            if completed.returncode != 0:
                failures.append(f"executable ratchet fixture failed: {' '.join(command)}")

    if failures:
        for failure in failures:
            print(f"[fail] {failure}", file=sys.stderr)
        return 1
    print("[pass] executable SSA/lifetime structural ratchet")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
