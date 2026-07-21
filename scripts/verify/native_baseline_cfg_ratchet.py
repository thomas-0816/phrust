#!/usr/bin/env python3
"""Reject artificial baseline CFG growth using source and compiled metrics."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--report")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    source = (root / "crates/php_jit/src/cranelift_lowering/executable_region.rs").read_text(
        encoding="utf-8"
    )
    errors = []
    forbidden = {
        "instruction_blocks": "instruction-to-block baseline map",
        "terminator_blocks": "separate terminator block map",
    }
    for needle, description in forbidden.items():
        if needle in source:
            errors.append(f"found forbidden {description}: {needle}")
    if "transition_blocks" not in source or "instruction_has_native_transition" not in source:
        errors.append("resume blocks are not restricted to true native transitions")
    if "set_srcloc" not in source:
        errors.append("source positions are not represented with Cranelift source locations")

    if args.report:
        report = json.loads(Path(args.report).read_text(encoding="utf-8"))
        metrics = report.get("metrics", report)
        limits = {
            # Production admission retains 30% headroom below the absolute
            # Cranelift limits (768/16384/32768/4096). Reports must prove the
            # exact post-replan CLIF shape, not only the planner estimate.
            "max_fragment_clif_blocks": 537,
            "max_fragment_clif_values": 11_468,
            "max_fragment_clif_instructions": 22_937,
            "max_fragment_block_parameters": 2_867,
            "max_temporary_cache_entries": 256,
            "fragment_frame_slots": 1_024,
            "max_fragment_loads_per_source_instruction_milli": 24_000,
            "max_fragment_stores_per_source_instruction_milli": 24_000,
        }
        for key, limit in limits.items():
            value = metrics.get(key)
            if not isinstance(value, int):
                errors.append(f"compiled report is missing integer metric {key}")
            elif value > limit:
                errors.append(f"{key}={value} exceeds {limit}")
        replans = metrics.get("pre_regalloc_replans")
        if not isinstance(replans, int):
            errors.append("compiled report is missing integer metric pre_regalloc_replans")
        elif replans > 6:
            errors.append(f"pre_regalloc_replans={replans} exceeds 6")
        blocks = metrics.get("clif_blocks")
        real_blocks = metrics.get("plan_php_blocks")
        safepoints = metrics.get("plan_safepoints")
        fragments = metrics.get("plan_fragments")
        if all(isinstance(value, int) for value in (blocks, real_blocks, safepoints, fragments)):
            # Each semantic safepoint may lower several typed fallible helpers
            # (lifecycle, operation, cleanup). Every such helper needs one
            # success continuation, while failure bodies remain shared. This
            # bound rejects instruction-entry/terminator block maps without
            # pretending that helper continuations are free.
            structural_limit = real_blocks + 7 * safepoints + 8 * fragments + 16
            if blocks > structural_limit:
                errors.append(
                    f"clif_blocks={blocks} exceeds compact-CFG bound {structural_limit}"
                )
        else:
            errors.append("compiled report lacks compact-CFG relationship metrics")

    if errors:
        for error in errors:
            print(f"native baseline CFG ratchet: {error}", file=sys.stderr)
        return 1
    print("native baseline CFG ratchet: pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
