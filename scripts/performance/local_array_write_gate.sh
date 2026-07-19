#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
target_dir="${CARGO_TARGET_DIR:-${root}/target}"
vm="${target_dir}/debug/php-vm"
fixture="${root}/fixtures/runtime/valid/arrays/local-write-cow.php"
expected="${root}/tests/fixtures/performance/native_tier/local_array_write_cow.php.out"
out_dir="${target_dir}/performance/local-array-write-gate"
actual="${out_dir}/actual.txt"
counters="${out_dir}/counters.json"

mkdir -p "${out_dir}"
cargo test -q -p php_jit --lib \
    baseline_array_append_consumes_plain_local_without_load_or_store_helpers
cargo build -q -p php_vm_cli --bin php-vm

"${vm}" run \
    --engine-preset default \
    --native-cache off \
    --counters-json "${counters}" \
    "${fixture}" >"${actual}"

if ! cmp -s "${expected}" "${actual}"; then
    printf '%s\n' '[fail] local array-write fixture output changed' >&2
    diff -u "${expected}" "${actual}" >&2 || true
    exit 1
fi

python3 - "${counters}" <<'PY'
import json
import sys
from pathlib import Path

document = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
calls = document.get("runtime_helper_calls_by_id", {})
reasons = document.get("runtime_helper_local_read_by_reason", {})
fetches = int(calls.get("local_fetch", 0))
stores = int(calls.get("local_store", 0))
inserts = int(calls.get("array_insert", 0))
reference_fetches = int(reasons.get("reference_dereference", 0))
if stores != 0:
    raise SystemExit(
        f"[fail] local array writes crossed local_store {stores} time(s); expected 0"
    )
if fetches != 2 or reference_fetches != 2:
    raise SystemExit(
        "[fail] plain local array writes crossed local_fetch; "
        f"fetches={fetches}, reference-only fetches={reference_fetches}, expected 2/2"
    )
if inserts != 13:
    raise SystemExit(
        f"[fail] local array-write fixture used {inserts} inserts; expected 13"
    )
PY

printf '%s\n' '[pass] local array writes consume their owner without load/store helpers'
