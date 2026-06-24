#!/usr/bin/env python3
"""Local default-off polymorphic IC guard experiment for Phase 7."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

import jit_bench_matrix


ROOT = Path(__file__).resolve().parents[3]
DEFAULT_ENGINE = ROOT / "target/debug/php-vm"
DEFAULT_OUT = ROOT / "target/phase7/cranelift/polymorphic-ic/report.json"
DEFAULT_FIXTURE_DIR = ROOT / "target/phase7/cranelift/polymorphic-ic/fixtures"
DEFAULT_COUNTER_DIR = ROOT / "target/phase7/cranelift/polymorphic-ic/counters"
MAX_POLYMORPHIC_ENTRIES = 4


FIXTURES: dict[str, str] = {
    "property_polymorphic": """<?php
class Phase7PolyPropA { public int $value = 1; }
class Phase7PolyPropB { public int $value = 2; }
class Phase7PolyPropC { public int $value = 3; }
function phase7_poly_prop_read($object): int { return $object->value; }
echo phase7_poly_prop_read(new Phase7PolyPropA());
echo phase7_poly_prop_read(new Phase7PolyPropB());
echo phase7_poly_prop_read(new Phase7PolyPropC());
""",
    "property_megamorphic": """<?php
class Phase7MegaPropA { public int $value = 1; }
class Phase7MegaPropB { public int $value = 2; }
class Phase7MegaPropC { public int $value = 3; }
class Phase7MegaPropD { public int $value = 4; }
class Phase7MegaPropE { public int $value = 5; }
function phase7_mega_prop_read($object): int { return $object->value; }
echo phase7_mega_prop_read(new Phase7MegaPropA());
echo phase7_mega_prop_read(new Phase7MegaPropB());
echo phase7_mega_prop_read(new Phase7MegaPropC());
echo phase7_mega_prop_read(new Phase7MegaPropD());
echo phase7_mega_prop_read(new Phase7MegaPropE());
""",
    "method_polymorphic": """<?php
class Phase7PolyMethodA { public function value(): int { return 1; } }
class Phase7PolyMethodB { public function value(): int { return 2; } }
class Phase7PolyMethodC { public function value(): int { return 3; } }
function phase7_poly_method_call($object): int { return $object->value(); }
echo phase7_poly_method_call(new Phase7PolyMethodA());
echo phase7_poly_method_call(new Phase7PolyMethodB());
echo phase7_poly_method_call(new Phase7PolyMethodC());
""",
    "method_megamorphic": """<?php
class Phase7MegaMethodA { public function value(): int { return 1; } }
class Phase7MegaMethodB { public function value(): int { return 2; } }
class Phase7MegaMethodC { public function value(): int { return 3; } }
class Phase7MegaMethodD { public function value(): int { return 4; } }
class Phase7MegaMethodE { public function value(): int { return 5; } }
function phase7_mega_method_call($object): int { return $object->value(); }
echo phase7_mega_method_call(new Phase7MegaMethodA());
echo phase7_mega_method_call(new Phase7MegaMethodB());
echo phase7_mega_method_call(new Phase7MegaMethodC());
echo phase7_mega_method_call(new Phase7MegaMethodD());
echo phase7_mega_method_call(new Phase7MegaMethodE());
""",
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--engine", type=Path, default=Path(os.getenv("PHRUST_PHP_VM", DEFAULT_ENGINE)))
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT)
    parser.add_argument("--fixture-dir", type=Path, default=DEFAULT_FIXTURE_DIR)
    parser.add_argument("--counter-dir", type=Path, default=DEFAULT_COUNTER_DIR)
    parser.add_argument("--timeout", type=float, default=5.0)
    return parser.parse_args()


def rel(path: Path) -> str:
    try:
        return path.resolve().relative_to(ROOT).as_posix()
    except ValueError:
        return path.as_posix()


def write_fixtures(fixture_dir: Path) -> dict[str, Path]:
    fixture_dir.mkdir(parents=True, exist_ok=True)
    paths: dict[str, Path] = {}
    for name, source in FIXTURES.items():
        path = fixture_dir / f"{name}.php"
        path.write_text(source, encoding="utf-8")
        paths[name] = path
    return paths


def normalized_env() -> dict[str, str]:
    env = jit_bench_matrix.normalized_env(ROOT / "target/phase7/cranelift/polymorphic-ic/tmp")
    env["PHRUST_RANDOM_SEED"] = "phase7-cranelift-polymorphic-ic"
    env["RUST_TEST_SEED"] = "phase7-cranelift-polymorphic-ic"
    return env


def run_vm(
    *,
    engine: Path,
    fixture: Path,
    mode: str,
    counters_json: Path | None,
    timeout: float,
) -> tuple[subprocess.CompletedProcess[str], float]:
    command = [
        str(engine),
        "run",
        "--inline-caches=on",
        f"--jit={mode}",
    ]
    if mode == "cranelift":
        command.append("--jit-eager")
    if counters_json is not None:
        command.extend(["--counters-json", rel(counters_json)])
    command.append(rel(fixture))
    start = time.perf_counter()
    completed = subprocess.run(
        command,
        cwd=ROOT,
        env=normalized_env(),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        check=False,
    )
    return completed, time.perf_counter() - start


def load_counters(path: Path) -> dict[str, Any]:
    try:
        decoded = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise SystemExit(f"invalid counters JSON {path}: {exc}") from exc
    if not isinstance(decoded, dict):
        raise SystemExit(f"counters JSON is not an object: {path}")
    return decoded


def profiles_for(kind: str, counters: dict[str, Any]) -> list[dict[str, Any]]:
    key = "property_fetch_profiles" if kind == "property" else "method_call_profiles"
    raw = counters.get(key)
    if not isinstance(raw, list):
        return []
    return [item for item in raw if isinstance(item, dict)]


def classify_profile(profile: dict[str, Any], kind: str) -> dict[str, Any]:
    class_ids = [int(value) for value in profile.get("class_ids", []) if isinstance(value, int)]
    receiver_classes = [
        str(value) for value in profile.get("receiver_classes", []) if isinstance(value, str)
    ]
    entries = []
    for index, class_id in enumerate(class_ids[:MAX_POLYMORPHIC_ENTRIES]):
        entry: dict[str, Any] = {
            "class_id": class_id,
            "receiver_class": receiver_classes[index] if index < len(receiver_classes) else "",
        }
        if kind == "property":
            slots = profile.get("property_slot_indexes", [])
            layouts = profile.get("class_layout_versions", [])
            if index < len(slots) and isinstance(slots[index], int):
                entry["property_slot_index"] = slots[index]
            if index < len(layouts) and isinstance(layouts[index], int):
                entry["class_layout_version"] = layouts[index]
        else:
            method_ids = profile.get("method_ids", [])
            slots = profile.get("method_slot_indexes", [])
            layouts = profile.get("override_layout_versions", [])
            if index < len(method_ids) and isinstance(method_ids[index], int):
                entry["method_id"] = method_ids[index]
            if index < len(slots) and isinstance(slots[index], int):
                entry["method_slot_index"] = slots[index]
            if index < len(layouts) and isinstance(layouts[index], int):
                entry["override_layout_version"] = layouts[index]
        entries.append(entry)

    receiver_count = len(receiver_classes)
    if receiver_count <= 1:
        prototype_state = "monomorphic"
        fallback = "none"
    elif receiver_count <= MAX_POLYMORPHIC_ENTRIES:
        prototype_state = "polymorphic"
        fallback = "generic_on_guard_miss"
    else:
        prototype_state = "megamorphic"
        fallback = "megamorphic_fallback"

    return {
        "kind": kind,
        "callsite": profile.get("callsite"),
        "name": profile.get("property") if kind == "property" else profile.get("method"),
        "state": profile.get("state"),
        "prototype_state": prototype_state,
        "observed_receiver_count": receiver_count,
        "max_polymorphic_entries": MAX_POLYMORPHIC_ENTRIES,
        "guard_entry_count": len(entries),
        "guard_entries": entries,
        "fallback": fallback,
        "default_enabled": False,
        "guard_policy": "receiver-class guard plus slot/layout metadata; generic VM fallback on miss",
        "non_eligible_reasons": profile.get("non_eligible_reasons", []),
    }


def scenario_kind(name: str) -> str:
    return "property" if name.startswith("property_") else "method"


def main() -> int:
    args = parse_args()
    if not args.engine.is_file() or not os.access(args.engine, os.X_OK):
        raise SystemExit(f"Rust VM is not executable: {args.engine}")

    fixtures = write_fixtures(args.fixture_dir)
    args.counter_dir.mkdir(parents=True, exist_ok=True)
    rows: list[dict[str, Any]] = []
    guard_extension: list[dict[str, Any]] = []
    failures: list[str] = []

    for scenario, fixture in fixtures.items():
        counter_path = args.counter_dir / f"{scenario}.json"
        off, off_seconds = run_vm(
            engine=args.engine,
            fixture=fixture,
            mode="off",
            counters_json=None,
            timeout=args.timeout,
        )
        cranelift, cranelift_seconds = run_vm(
            engine=args.engine,
            fixture=fixture,
            mode="cranelift",
            counters_json=counter_path,
            timeout=args.timeout,
        )
        counters = load_counters(counter_path)
        kind = scenario_kind(scenario)
        profiles = profiles_for(kind, counters)
        prototype_profiles = [classify_profile(profile, kind) for profile in profiles]
        selected = prototype_profiles[0] if prototype_profiles else None

        stdout_match = jit_bench_matrix.normalize_text(off.stdout) == jit_bench_matrix.normalize_text(
            cranelift.stdout
        )
        stderr_match = jit_bench_matrix.normalize_text(off.stderr) == jit_bench_matrix.normalize_text(
            cranelift.stderr
        )
        if off.returncode != cranelift.returncode:
            failures.append(f"{scenario}: exit mismatch off={off.returncode} cranelift={cranelift.returncode}")
        if not stdout_match:
            failures.append(f"{scenario}: stdout mismatch")
        if not stderr_match:
            failures.append(f"{scenario}: stderr mismatch")
        expected_state = "megamorphic" if scenario.endswith("megamorphic") else "polymorphic"
        if selected is None:
            failures.append(f"{scenario}: missing {kind} profile")
        elif selected["prototype_state"] != expected_state:
            failures.append(
                f"{scenario}: expected prototype state {expected_state}, got {selected['prototype_state']}"
            )

        row = {
            "scenario": scenario,
            "kind": kind,
            "fixture": rel(fixture),
            "counters_json": rel(counter_path),
            "off_exit": off.returncode,
            "cranelift_exit": cranelift.returncode,
            "output_match": off.returncode == cranelift.returncode and stdout_match and stderr_match,
            "stdout_match": stdout_match,
            "stderr_match": stderr_match,
            "off_seconds": off_seconds,
            "cranelift_seconds": cranelift_seconds,
            "selected_profile": selected,
            "prototype_state": selected["prototype_state"] if selected is not None else None,
            "observed_receiver_count": selected["observed_receiver_count"] if selected is not None else None,
            "guard_entry_count": selected["guard_entry_count"] if selected is not None else None,
            "fallback": selected["fallback"] if selected is not None else None,
            "default_enabled": selected["default_enabled"] if selected is not None else False,
            "profiles": prototype_profiles,
        }
        rows.append(row)
        if selected is not None:
            guard_extension.append(
                {
                    "scenario": scenario,
                    "kind": kind,
                    "state": selected["prototype_state"],
                    "guard_entry_count": selected["guard_entry_count"],
                    "max_polymorphic_entries": MAX_POLYMORPHIC_ENTRIES,
                    "fallback": selected["fallback"],
                }
            )

    if not any(item.get("fallback") == "megamorphic_fallback" for item in guard_extension):
        failures.append("expected at least one megamorphic fallback in guard report extension")

    result = {
        "schema_version": 1,
        "status": "pass" if not failures else "fail",
        "runtime_policy": "experimental-off local diagnostic; no default runtime or CI gate enables polymorphic JIT IC dispatch",
        "max_polymorphic_entries": MAX_POLYMORPHIC_ENTRIES,
        "fixtures": {name: rel(path) for name, path in fixtures.items()},
        "rows": rows,
        "guard_report_extension": guard_extension,
        "failures": failures,
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")

    if failures:
        print(f"[fail] Cranelift polymorphic IC experiment found {len(failures)} failure(s); wrote {rel(args.out)}", file=sys.stderr)
        return 1
    print(f"[pass] Cranelift polymorphic IC experiment wrote {rel(args.out)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
