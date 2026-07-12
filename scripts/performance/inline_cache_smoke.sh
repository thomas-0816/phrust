#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

ENGINE="${PHRUST_PHP_VM:-${CARGO_TARGET_DIR:-target}/debug/php-vm}"
OUT_DIR="target/performance/inline-cache-smoke"
FIXTURE_DIRS=(
    "tests/fixtures/performance/perf_smoke"
    "tests/fixtures/performance/inline_cache"
)

if [ ! -x "$ENGINE" ]; then
    printf '[fail] Rust VM is not executable: %s\n' "$ENGINE" >&2
    exit 1
fi

python3 - <<'PY'
from pathlib import Path

inline_cache = Path("crates/php_vm/src/inline_cache.rs").read_text(encoding="utf-8")
# The route helpers move between vm submodules as the dispatch code is
# refactored; scan every file that can legitimately host the contracts.
vm = "".join(
    Path(name).read_text(encoding="utf-8")
    for name in (
        "crates/php_vm/src/vm/mod.rs",
        "crates/php_vm/src/vm/calls.rs",
        "crates/php_vm/src/vm/method_dispatch.rs",
        "crates/php_vm/src/vm/inline_cache_access.rs",
    )
    if Path(name).exists()
)

forbidden = "Arc::new(declaring_class.clone())"
if forbidden in vm:
    raise SystemExit(f"[fail] warmed method route deep-clones its class: {forbidden}")

required = {
    "stable method route identity": "pub identity: MethodCallRouteIdentity",
    "stable function owner identity": "unit_identity: u64",
    "canonical declaring class handle": "owner.lookup_class_handle(&declaring_class.name)",
    "stable compiled-unit cache key": "compiled.cache_identity()",
}
combined = inline_cache + vm
for contract, needle in required.items():
    if needle not in combined:
        raise SystemExit(f"[fail] inline-cache route contract missing: {contract}")
PY

mkdir -p "$OUT_DIR"
rm -f "$OUT_DIR"/*

fixture_count=0
for fixtures_dir in "${FIXTURE_DIRS[@]}"; do
    for fixture in "$fixtures_dir"/*.php; do
        name="$(basename "$fixtures_dir")-$(basename "$fixture" .php)"
        expected="$fixture.out"
        if [ ! -f "$expected" ]; then
            continue
        fi

        "$ENGINE" run \
            --inline-caches=off \
            --counters-json "$OUT_DIR/$name.off.counters.json" \
            "$fixture" \
            > "$OUT_DIR/$name.off.stdout" \
            2> "$OUT_DIR/$name.off.stderr"

        "$ENGINE" run \
            --inline-caches=on \
            --counters-json "$OUT_DIR/$name.on.counters.json" \
            "$fixture" \
            > "$OUT_DIR/$name.on.stdout" \
            2> "$OUT_DIR/$name.on.stderr"

        cmp -s "$OUT_DIR/$name.off.stdout" "$OUT_DIR/$name.on.stdout" || {
            printf '[fail] inline-cache stdout diverged for %s\n' "$fixture" >&2
            exit 1
        }
        cmp -s "$OUT_DIR/$name.off.stderr" "$OUT_DIR/$name.on.stderr" || {
            printf '[fail] inline-cache stderr diverged for %s\n' "$fixture" >&2
            exit 1
        }
        cmp -s "$expected" "$OUT_DIR/$name.on.stdout" || {
            printf '[fail] inline-cache output does not match fixture expectation for %s\n' "$fixture" >&2
            exit 1
        }
        fixture_count=$((fixture_count + 1))
    done
done

python3 - <<'PY'
import json
from pathlib import Path

out_dir = Path("target/performance/inline-cache-smoke")
off = [json.loads(path.read_text(encoding="utf-8")) for path in sorted(out_dir.glob("*.off.counters.json"))]
on = [json.loads(path.read_text(encoding="utf-8")) for path in sorted(out_dir.glob("*.on.counters.json"))]
if not off or not on:
    raise SystemExit("[fail] missing inline-cache counter samples")

required_fields = [
    "inline_cache_observations",
    "inline_cache_slots",
    "inline_cache_function_slots",
    "inline_cache_method_slots",
    "inline_cache_property_slots",
    "inline_cache_property_assign_slots",
    "inline_cache_dim_slots",
    "inline_cache_class_relation_slots",
    "inline_cache_hits",
    "inline_cache_misses",
    "inline_cache_invalidations",
    "inline_cache_guard_failures",
    "inline_cache_fallback_calls",
    "inline_cache_megamorphic",
    "inline_cache_disabled",
    "method_ic_hits",
    "method_ic_misses",
    "method_ic_polymorphic_hits",
    "method_ic_guard_failures",
    "method_direct_dispatch_hits",
    "method_direct_dispatch_fallbacks",
    "method_tiny_inline_candidates",
    "method_tiny_inline_rejected_by_reason",
    "property_ic_hits",
    "property_ic_misses",
    "property_ic_guard_failures",
    "property_ic_fallback_reasons",
    "property_assign_ic_hits",
    "property_assign_ic_misses",
    "property_assign_ic_guard_failures",
    "property_assign_ic_shape_exits",
    "property_assign_ic_visibility_exits",
    "property_assign_ic_type_exits",
    "property_assign_ic_readonly_exits",
    "property_assign_ic_hook_magic_exits",
    "property_assign_ic_reference_exits",
    "property_assign_ic_dynamic_exits",
    "property_assign_ic_fallback_reasons",
    "class_static_ic_hits",
    "class_static_ic_misses",
    "class_static_ic_guard_failures",
    "class_relation_cache_hits",
    "class_relation_cache_misses",
    "class_relation_cache_invalidations",
    "instanceof_cache_hits",
    "instanceof_cache_misses",
    "method_override_cache_hits",
    "method_override_cache_misses",
    "include_path_ic_hits",
    "include_path_ic_misses",
    "include_path_ic_invalidations",
    "include_path_ic_guard_failures",
    "autoload_class_lookup_ic_hits",
    "autoload_class_lookup_ic_misses",
    "autoload_class_lookup_ic_invalidations",
    "autoload_class_lookup_ic_guard_failures",
    "function_call_ic_hits",
    "function_call_ic_misses",
    "builtin_call_ic_hits",
    "builtin_call_ic_misses",
    "builtin_fast_stub_hits",
    "builtin_fast_stub_misses",
    "builtin_fast_stub_fallback_by_reason",
    "builtin_intrinsic_candidates",
    "intrinsic_hits",
    "intrinsic_misses",
    "intrinsic_fallback_by_reason",
    "specialized_builtin_opcode_hits",
    "slow_path_calls_by_reason",
    "call_frame_layout_observed",
    "tiny_frame_candidates",
    "specialized_frame_hits",
    "generic_frame_fallback_by_reason",
    "arg_array_avoided",
    "heap_frame_avoided",
    "call_ic_megamorphic_fallbacks",
]
for sample in off + on:
    for field in required_fields:
        if field not in sample:
            raise SystemExit(f"[fail] inline-cache counter missing from sample: {field}")

def total(samples, field):
    return sum(sample.get(field, 0) for sample in samples)

def total_map(samples, field, name):
    total_value = 0
    for sample in samples:
        value = sample.get(field, {})
        if isinstance(value, dict):
            total_value += value.get(name, 0)
    return total_value

off_slots = total(off, "inline_cache_slots")
off_observations = total(off, "inline_cache_observations")
off_function_call_hits = total(off, "function_call_ic_hits")
off_function_call_misses = total(off, "function_call_ic_misses")
off_builtin_call_hits = total(off, "builtin_call_ic_hits")
off_builtin_call_misses = total(off, "builtin_call_ic_misses")
off_class_relation_hits = total(off, "class_relation_cache_hits")
off_class_relation_misses = total(off, "class_relation_cache_misses")
off_instanceof_hits = total(off, "instanceof_cache_hits")
off_instanceof_misses = total(off, "instanceof_cache_misses")
off_method_override_hits = total(off, "method_override_cache_hits")
off_method_override_misses = total(off, "method_override_cache_misses")
on_slots = total(on, "inline_cache_slots")
on_observations = total(on, "inline_cache_observations")
on_function_slots = total(on, "inline_cache_function_slots")
on_method_slots = total(on, "inline_cache_method_slots")
on_property_slots = total(on, "inline_cache_property_slots")
on_property_assign_slots = total(on, "inline_cache_property_assign_slots")
on_dim_slots = total(on, "inline_cache_dim_slots")
on_class_relation_slots = total(on, "inline_cache_class_relation_slots")
on_hits = total(on, "inline_cache_hits")
on_misses = total(on, "inline_cache_misses")
on_invalidations = total(on, "inline_cache_invalidations")
on_guard_failures = total(on, "inline_cache_guard_failures")
on_fallback_calls = total(on, "inline_cache_fallback_calls")
on_megamorphic = total(on, "inline_cache_megamorphic")
on_disabled = total(on, "inline_cache_disabled")
on_method_hits = total(on, "method_ic_hits")
on_method_misses = total(on, "method_ic_misses")
on_method_polymorphic_hits = total(on, "method_ic_polymorphic_hits")
on_method_guard_failures = total(on, "method_ic_guard_failures")
on_method_direct_dispatch_hits = total(on, "method_direct_dispatch_hits")
on_method_direct_dispatch_fallbacks = total(on, "method_direct_dispatch_fallbacks")
on_method_tiny_inline_candidates = total(on, "method_tiny_inline_candidates")
on_property_hits = total(on, "property_ic_hits")
on_property_misses = total(on, "property_ic_misses")
on_property_guard_failures = total(on, "property_ic_guard_failures")
on_property_assign_hits = total(on, "property_assign_ic_hits")
on_property_assign_misses = total(on, "property_assign_ic_misses")
on_property_assign_guard_failures = total(on, "property_assign_ic_guard_failures")
on_property_assign_visibility_exits = total(on, "property_assign_ic_visibility_exits")
on_property_assign_type_exits = total(on, "property_assign_ic_type_exits")
on_property_assign_hook_magic_exits = total(on, "property_assign_ic_hook_magic_exits")
on_property_assign_dynamic_exits = total(on, "property_assign_ic_dynamic_exits")
on_class_static_hits = total(on, "class_static_ic_hits")
on_class_static_misses = total(on, "class_static_ic_misses")
on_class_static_guard_failures = total(on, "class_static_ic_guard_failures")
on_class_relation_hits = total(on, "class_relation_cache_hits")
on_class_relation_misses = total(on, "class_relation_cache_misses")
on_class_relation_invalidations = total(on, "class_relation_cache_invalidations")
on_instanceof_hits = total(on, "instanceof_cache_hits")
on_instanceof_misses = total(on, "instanceof_cache_misses")
on_method_override_hits = total(on, "method_override_cache_hits")
on_method_override_misses = total(on, "method_override_cache_misses")
on_include_path_hits = total(on, "include_path_ic_hits")
on_include_path_misses = total(on, "include_path_ic_misses")
on_include_path_guard_failures = total(on, "include_path_ic_guard_failures")
on_autoload_class_lookup_hits = total(on, "autoload_class_lookup_ic_hits")
on_autoload_class_lookup_misses = total(on, "autoload_class_lookup_ic_misses")
on_autoload_class_lookup_guard_failures = total(on, "autoload_class_lookup_ic_guard_failures")
on_function_call_hits = total(on, "function_call_ic_hits")
on_function_call_misses = total(on, "function_call_ic_misses")
on_builtin_call_hits = total(on, "builtin_call_ic_hits")
on_builtin_call_misses = total(on, "builtin_call_ic_misses")
on_call_ic_megamorphic_fallbacks = total(on, "call_ic_megamorphic_fallbacks")
on_tiny_frame_candidates = total(on, "tiny_frame_candidates")
on_specialized_frame_hits = total(on, "specialized_frame_hits")
on_arg_array_avoided = total(on, "arg_array_avoided")
on_heap_frame_avoided = total(on, "heap_frame_avoided")

if off_slots != 0 or off_observations != 0:
    raise SystemExit(f"[fail] inline-caches=off recorded slots={off_slots} observations={off_observations}")
if off_function_call_hits or off_function_call_misses or off_builtin_call_hits or off_builtin_call_misses:
    raise SystemExit("[fail] inline-caches=off recorded function/builtin IC counters")
if (
    off_class_relation_hits
    or off_class_relation_misses
    or off_instanceof_hits
    or off_instanceof_misses
    or off_method_override_hits
    or off_method_override_misses
):
    raise SystemExit("[fail] inline-caches=off recorded class-relation/method-override cache counters")
for builtin in ["strlen", "count", "is_int", "is_string", "is_array", "array_key_exists"]:
    if total_map(off, "builtin_fast_stub_hits", builtin) or total_map(off, "builtin_fast_stub_misses", builtin):
        raise SystemExit(f"[fail] inline-caches=off recorded builtin fast-stub counters for {builtin}")
if any(sample.get("builtin_fast_stub_fallback_by_reason", {}) for sample in off):
    raise SystemExit("[fail] inline-caches=off recorded builtin fast-stub fallback reasons")
if total(off, "builtin_intrinsic_candidates") or any(sample.get("intrinsic_hits", {}) or sample.get("intrinsic_misses", {}) for sample in off):
    raise SystemExit("[fail] inline-caches=off recorded builtin intrinsic counters")
if any(sample.get("intrinsic_fallback_by_reason", {}) for sample in off):
    raise SystemExit("[fail] inline-caches=off recorded intrinsic fallback reasons")
if any(sample.get("property_assign_ic_fallback_reasons", {}) for sample in off):
    raise SystemExit("[fail] inline-caches=off recorded property assignment fallback reasons")
if on_slots <= 0:
    raise SystemExit("[fail] inline-caches=on recorded no slots")
if on_observations < on_slots:
    raise SystemExit(f"[fail] inline-cache observations {on_observations} below slots {on_slots}")
if on_function_slots <= 0:
    raise SystemExit("[fail] inline-caches=on recorded no function call slots")
if on_method_slots <= 0:
    raise SystemExit("[fail] inline-caches=on recorded no method call slots")
if on_property_slots <= 0:
    raise SystemExit("[fail] inline-caches=on recorded no property fetch slots")
if on_property_assign_slots <= 0:
    raise SystemExit("[fail] inline-caches=on recorded no property assignment slots")
if on_dim_slots <= 0:
    raise SystemExit("[fail] inline-caches=on recorded no dim fetch slots")
if on_class_relation_slots <= 0:
    raise SystemExit("[fail] inline-caches=on recorded no class-relation slots")
if on_hits <= 0:
    raise SystemExit("[fail] function-call inline cache recorded no hits")
if on_misses <= 0:
    raise SystemExit("[fail] function-call inline cache recorded no misses")
if on_function_call_hits <= 0:
    raise SystemExit("[fail] function-call IC counter recorded no hits")
if on_function_call_misses <= 0:
    raise SystemExit("[fail] function-call IC counter recorded no misses")
if on_builtin_call_hits <= 0:
    raise SystemExit("[fail] builtin-call IC counter recorded no hits")
if on_builtin_call_misses <= 0:
    raise SystemExit("[fail] builtin-call IC counter recorded no misses")
for builtin in ["strlen", "count", "is_int", "is_string", "is_array", "array_key_exists"]:
    if total_map(on, "builtin_fast_stub_hits", builtin) <= 0:
        raise SystemExit(f"[fail] builtin fast stub recorded no hits for {builtin}")
for intrinsic in ["strtolower", "str_contains", "str_starts_with", "str_ends_with", "array_key_exists"]:
    if total_map(on, "intrinsic_hits", intrinsic) <= 0:
        raise SystemExit(f"[fail] builtin intrinsic recorded no hits for {intrinsic}")
if total(on, "builtin_intrinsic_candidates") <= 0:
    raise SystemExit("[fail] builtin intrinsic candidate counter recorded no candidates")
if on_tiny_frame_candidates <= 0:
    raise SystemExit("[fail] call-frame layout counter recorded no tiny-frame candidates")
if on_specialized_frame_hits <= 0:
    raise SystemExit("[fail] specialized call-frame counter recorded no hits")
if on_arg_array_avoided <= 0:
    raise SystemExit("[fail] specialized call-frame counter recorded no avoided argument arrays")
if on_heap_frame_avoided <= 0:
    raise SystemExit("[fail] specialized call-frame counter recorded no avoided heap frames")
for layout in ["tiny_leaf_frame", "known_method_frame", "closure_frame", "variadic_named_argument_frame", "generator_frame", "include_eval_frame"]:
    if total_map(on, "call_frame_layout_observed", layout) <= 0:
        raise SystemExit(f"[fail] call-frame layout counter recorded no {layout}")
for reason in ["not_tiny_leaf", "class_context", "closure", "named_or_variadic", "by_ref_param", "generator", "include_eval"]:
    if total_map(on, "generic_frame_fallback_by_reason", reason) <= 0:
        raise SystemExit(f"[fail] specialized frame fallback counter recorded no {reason}")
if total_map(on, "builtin_fast_stub_misses", "strlen") <= 0:
    raise SystemExit("[fail] builtin fast stub recorded no strlen misses")
if total_map(on, "builtin_fast_stub_fallback_by_reason", "strlen.type") <= 0:
    raise SystemExit("[fail] builtin fast stub recorded no strlen type fallback reason")
if total_map(on, "builtin_fast_stub_misses", "array_key_exists") <= 0:
    raise SystemExit("[fail] builtin fast stub recorded no array_key_exists misses")
if total_map(on, "builtin_fast_stub_fallback_by_reason", "array_key_exists.type") <= 0:
    raise SystemExit("[fail] builtin fast stub recorded no array_key_exists type fallback reason")
if on_method_hits <= 0:
    raise SystemExit("[fail] method-call inline cache recorded no hits")
if on_method_misses <= 0:
    raise SystemExit("[fail] method-call inline cache recorded no misses")
if on_method_polymorphic_hits <= 0:
    raise SystemExit("[fail] method-call inline cache recorded no polymorphic hits")
if on_method_guard_failures <= 0:
    raise SystemExit("[fail] method-call inline cache recorded no guard failures")
if on_method_direct_dispatch_hits <= 0:
    raise SystemExit("[fail] method-call direct dispatch recorded no hits")
if on_method_direct_dispatch_fallbacks <= 0:
    raise SystemExit("[fail] method-call direct dispatch recorded no fallbacks")
if on_method_tiny_inline_candidates <= 0:
    raise SystemExit("[fail] method-call tiny-inline metadata recorded no candidates")
for reason in ["not_final_or_private", "not_tiny_leaf_return"]:
    if total_map(on, "method_tiny_inline_rejected_by_reason", reason) <= 0:
        raise SystemExit(f"[fail] method-call tiny-inline metadata recorded no {reason} rejection")
if on_property_hits <= 0:
    raise SystemExit("[fail] property-fetch inline cache recorded no hits")
if on_property_misses <= 0:
    raise SystemExit("[fail] property-fetch inline cache recorded no misses")
if on_property_assign_hits <= 0:
    raise SystemExit("[fail] property-assignment inline cache recorded no hits")
if on_property_assign_misses <= 0:
    raise SystemExit("[fail] property-assignment inline cache recorded no misses")
if on_property_assign_visibility_exits <= 0:
    raise SystemExit("[fail] property-assignment inline cache recorded no visibility exits")
if on_property_assign_type_exits <= 0:
    raise SystemExit("[fail] property-assignment inline cache recorded no type exits")
if on_property_assign_hook_magic_exits <= 0:
    raise SystemExit("[fail] property-assignment inline cache recorded no hook/magic exits")
if on_property_assign_dynamic_exits <= 0:
    raise SystemExit("[fail] property-assignment inline cache recorded no dynamic exits")
for reason in [
    "dynamic_property_fallback",
    "magic_set_metadata",
    "property_hook_present",
    "type_mismatch",
    "visibility_mismatch",
]:
    if total_map(on, "property_assign_ic_fallback_reasons", reason) <= 0:
        raise SystemExit(f"[fail] property-assignment IC recorded no {reason} fallback reason")
if on_class_static_hits <= 0:
    raise SystemExit("[fail] class-constant/static-property inline cache recorded no hits")
if on_class_static_misses <= 0:
    raise SystemExit("[fail] class-constant/static-property inline cache recorded no misses")
if on_class_relation_hits <= 0:
    raise SystemExit("[fail] class-relation cache recorded no hits")
if on_class_relation_misses <= 0:
    raise SystemExit("[fail] class-relation cache recorded no misses")
if on_class_relation_invalidations <= 0:
    raise SystemExit("[fail] class-relation cache recorded no invalidations")
if on_instanceof_hits <= 0:
    raise SystemExit("[fail] instanceof cache recorded no hits")
if on_instanceof_misses <= 0:
    raise SystemExit("[fail] instanceof cache recorded no misses")
if on_method_override_hits <= 0:
    raise SystemExit("[fail] method-override cache recorded no hits")
if on_method_override_misses <= 0:
    raise SystemExit("[fail] method-override cache recorded no misses")
if on_include_path_hits <= 0:
    raise SystemExit("[fail] include-path inline cache recorded no hits")
if on_include_path_misses <= 0:
    raise SystemExit("[fail] include-path inline cache recorded no misses")
if on_autoload_class_lookup_hits <= 0:
    raise SystemExit("[fail] autoload class lookup inline cache recorded no hits")
if on_autoload_class_lookup_misses <= 0:
    raise SystemExit("[fail] autoload class lookup inline cache recorded no misses")
if on_invalidations <= 0:
    raise SystemExit("[fail] inline-cache smoke recorded no IC invalidation")
if on_property_guard_failures <= 0:
    raise SystemExit("[fail] property-fetch inline cache recorded no guard failures for shape fallback fixture")
expected_guard_failures = (
    on_method_guard_failures
    + on_property_guard_failures
    + on_property_assign_guard_failures
)
if on_guard_failures != expected_guard_failures:
    raise SystemExit(
        f"[fail] inline-cache guard failures {on_guard_failures} differ from expected method/property-family guard failures {expected_guard_failures}"
    )
if on_fallback_calls < on_misses:
    raise SystemExit(f"[fail] inline-cache fallback calls {on_fallback_calls} below misses {on_misses}")
if on_class_static_guard_failures != 0:
    raise SystemExit(f"[fail] class-constant/static-property inline cache recorded unexpected guard failures: {on_class_static_guard_failures}")
if on_include_path_guard_failures != 0:
    raise SystemExit(f"[fail] include-path inline cache recorded unexpected guard failures: {on_include_path_guard_failures}")
if on_autoload_class_lookup_guard_failures != 0:
    raise SystemExit(f"[fail] autoload class lookup inline cache recorded unexpected guard failures: {on_autoload_class_lookup_guard_failures}")
if on_disabled != 0:
    raise SystemExit(f"[fail] function-call inline cache recorded unexpected disabled transitions: {on_disabled}")
if on_call_ic_megamorphic_fallbacks <= 0:
    raise SystemExit("[fail] function-call IC recorded no megamorphic fallback for dynamic call fixture")
PY

cargo test -p php_vm \
    inline_cache::lifecycle_tests::typed_slot_layout_is_smaller_than_legacy_option_layout \
    -- --exact --nocapture
cargo test -p php_vm \
    inline_cache::dense_contract_tests::dense_slot_binding_reaches_all_payloads_without_coordinate_map_access \
    -- --exact

printf '[pass] inline-cache smoke compared %s fixture(s)\n' "$fixture_count"
