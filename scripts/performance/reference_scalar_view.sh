#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
target_dir="${CARGO_TARGET_DIR:-${root}/target}"
vm="${target_dir}/debug/php-vm"
fixture="${root}/fixtures/runtime/valid/references/scalar-view-loop.php"
out_dir="${target_dir}/performance/reference-scalar-view"
counters="${out_dir}/counters.json"
actual="${out_dir}/actual.txt"
expected="${out_dir}/expected.txt"
publication_fixture="${root}/fixtures/runtime/valid/references/non-global-publication-loop.php"
publication_counters="${out_dir}/publication-counters.json"
publication_actual="${out_dir}/publication-actual.txt"
publication_expected="${out_dir}/publication-expected.txt"

mkdir -p "${out_dir}"
cargo build -q -p php_vm_cli --bin php-vm

"${vm}" run \
    --engine-preset default \
    --native-cache off \
    --counters-json "${counters}" \
    "${fixture}" >"${actual}"
printf '3000:2\n' >"${expected}"
if ! cmp -s "${expected}" "${actual}"; then
    printf '%s\n' '[fail] scalar reference-view fixture output changed' >&2
    diff -u "${expected}" "${actual}" >&2 || true
    exit 1
fi

python3 - "${counters}" <<'PY'
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
document = json.loads(path.read_text(encoding="utf-8"))
calls = document.get("runtime_helper_calls_by_id", {})
reasons = document.get("runtime_helper_local_read_by_reason", {})
fetches = int(calls.get("local_fetch", 0))
reference_fetches = int(reasons.get("reference_dereference", 0))
if fetches > 2:
    raise SystemExit(f"[fail] scalar reference loop used {fetches} local-fetch helpers; expected <= 2")
if reference_fetches > 1:
    raise SystemExit(
        f"[fail] scalar reference loop used {reference_fetches} reference fetch helpers; expected <= 1"
    )
PY

"${vm}" run \
    --engine-preset default \
    --native-cache off \
    --counters-json "${publication_counters}" \
    "${publication_fixture}" >"${publication_actual}"
printf '1000\n' >"${publication_expected}"
if ! cmp -s "${publication_expected}" "${publication_actual}"; then
    printf '%s\n' '[fail] non-global reference publication fixture output changed' >&2
    diff -u "${publication_expected}" "${publication_actual}" >&2 || true
    exit 1
fi

python3 - "${publication_counters}" <<'PY'
import json
import sys
from pathlib import Path

document = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
binds = int(document.get("runtime_helper_calls_by_id", {}).get("reference_bind", 0))
if binds > 1010:
    raise SystemExit(
        f"[fail] non-global reference loop used {binds} reference helpers; expected <= 1010"
    )
PY

printf '%s\n' '[pass] scalar reference reads and non-global publication stay on bounded paths'
