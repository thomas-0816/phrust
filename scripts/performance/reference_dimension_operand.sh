#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
target_dir="${CARGO_TARGET_DIR:-${root}/target}"
vm="${target_dir}/debug/php-vm"
fixture="${root}/fixtures/runtime/valid/references/array-operand-loop.php"
out_dir="${target_dir}/performance/reference-dimension-operand"
counters="${out_dir}/counters.json"
actual="${out_dir}/actual.txt"
expected="${out_dir}/expected.txt"

mkdir -p "${out_dir}"
cargo build -q -p php_vm_cli --bin php-vm

"${vm}" run \
    --engine-preset default \
    --native-cache off \
    --counters-json "${counters}" \
    "${fixture}" >"${actual}"
printf '3000\n3000\n' >"${expected}"
if ! cmp -s "${expected}" "${actual}"; then
    printf '%s\n' '[fail] reference dimension-operand fixture output changed' >&2
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
dimension_fetches = int(calls.get("array_fetch", 0))
reference_fetches = int(reasons.get("reference_dereference", 0))
if fetches != 0 or reference_fetches != 0:
    raise SystemExit(
        "[fail] reference dimension loop crossed the local-fetch helper "
        f"{fetches} time(s), including {reference_fetches} reference dereference(s)"
    )
if dimension_fetches != 2002:
    raise SystemExit(
        f"[fail] reference dimension loops used {dimension_fetches} array fetches; expected 2002"
    )
PY

printf '%s\n' '[pass] reference-backed and plain dimension operands avoid local-fetch helpers'
