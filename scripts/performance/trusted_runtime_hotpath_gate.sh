#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

cargo test -p php_runtime borrowed_property_writes_preserve_declared_and_dynamic_semantics --lib
cargo test -p php_runtime native_ops --lib
cargo test -p php_vm vm::jit_abi --lib

python3 - <<'PY'
from pathlib import Path

root = Path.cwd()
ops = (root / "crates/php_vm/src/vm/jit_abi/runtime_ops.rs").read_text(encoding="utf-8")
native = (root / "crates/php_runtime/src/native_ops.rs").read_text(encoding="utf-8")

if "fn native_property_name(" in ops or "target.property.clone()" in ops:
    raise SystemExit("native property execution regained request-local name copies")
if "NonNull::new(out)" in ops:
    raise SystemExit("trusted generated output slots regained per-call null validation")
if "catch_unwind" in native:
    raise SystemExit("typed native operations regained per-call panic containment")

print("[ok] trusted runtime hot-path invariants hold")
PY
