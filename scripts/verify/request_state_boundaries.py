#!/usr/bin/env python3
"""Enforce typed request-state ownership and migrated builtin service views."""

from __future__ import annotations

import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
RUNTIME = ROOT / "crates/php_runtime/src"
MIGRATED = ("pcre", "json", "curl")


def read(relative: str) -> str:
    return (ROOT / relative).read_text(encoding="utf-8")


def struct_body(source: str, name: str) -> str:
    match = re.search(rf"struct\s+{re.escape(name)}(?:<'[^>]+>)?\s*\{{", source)
    if match is None:
        return ""
    start = match.end()
    depth = 1
    for index in range(start, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return source[start:index]
    return ""


def main() -> int:
    failures: list[str] = []
    layout = read("crates/php_runtime/src/request_state.rs")
    builtin_state = read("crates/php_runtime/src/builtins/request_state.rs")
    context = read("crates/php_runtime/src/builtins/context.rs")
    views = read("crates/php_runtime/src/builtins/context/service_views.rs")
    vm = read("crates/php_vm/src/vm/mod.rs")
    vm_builtin_adapter = read("crates/php_vm/src/vm/builtin_adapter.rs")
    vm_execution_state = read("crates/php_vm/src/vm/execution_state.rs")
    vm_request_lifecycle = read("crates/php_vm/src/vm/request_lifecycle.rs")
    migration = read("docs/runtime/request-state-slots.md")

    if "HashMap" in layout or "BTreeMap" in layout:
        failures.append("request-state slot access must not use a name-keyed map")
    if "unsafe" in layout or "unsafe" in views:
        failures.append("request-state layout and service views must use safe borrowing")

    required_layout = (
        "ExtensionStateLayoutBuilder",
        "ExtensionStateSlot",
        "layout_id",
        "values: Vec<Box<dyn Any>>",
        "get_pair_mut",
    )
    for symbol in required_layout:
        if symbol not in layout:
            failures.append(f"typed request-state layout is missing {symbol}")

    legacy = struct_body(context, "BuiltinExtensionState")
    for extension in MIGRATED:
        if extension in legacy.lower():
            failures.append(f"migrated {extension} state remains in BuiltinExtensionState")

    forbidden_accessors = (
        "curl_state",
        "curl_state_ref",
        "pcre_cache",
        "set_preg_last_error",
        "clear_preg_last_error",
        "preg_last_error",
        "set_json_last_error",
        "json_last_error",
    )
    for accessor in forbidden_accessors:
        if re.search(rf"pub\s+fn\s+{accessor}\s*\(", context):
            failures.append(f"legacy BuiltinContext accessor remains: {accessor}")

    for symbol in (
        "JsonBuiltinServices",
        "PcreBuiltinServices",
        "PcreCallbackServices",
        "CurlBuiltinServices",
    ):
        if symbol not in views:
            failures.append(f"narrow migrated service view is missing {symbol}")

    required_state = (
        "ExtensionStateSlot<PcreRequestState>",
        "ExtensionStateSlot<JsonRequestState>",
        "ExtensionStateSlot<CurlState>",
        "get_pair_mut",
    )
    for symbol in required_state:
        if symbol not in builtin_state:
            failures.append(f"builtin request owner is missing {symbol}")

    execution_state = struct_body(vm_execution_state, "ExecutionState")
    vm_adapter_state = struct_body(vm_builtin_adapter, "BuiltinAdapterState")
    request_lifecycle_state = struct_body(
        vm_request_lifecycle, "RequestLifecycleState"
    )
    if execution_state.count("builtins: BuiltinAdapterState") != 1:
        failures.append("ExecutionState must own exactly one builtin adapter subsystem")
    if "builtin_request_state" in execution_state:
        failures.append("ExecutionState must not directly own migrated builtin request slots")
    if execution_state.count("request: RequestLifecycleState") != 1:
        failures.append("ExecutionState must own exactly one request lifecycle subsystem")
    lifecycle_fields = (
        "http_response",
        "upload_registry",
        "session",
        "session_loader",
        "sapi_name",
        "php_binary",
    )
    for field in lifecycle_fields:
        if re.search(
            rf"^\s+(?:pub\(super\)\s+)?{field}:", execution_state, re.MULTILINE
        ):
            failures.append(f"ExecutionState directly owns request field: {field}")
        if not re.search(rf"^\s+pub\(super\) {field}:", request_lifecycle_state, re.MULTILINE):
            failures.append(f"RequestLifecycleState is missing request field: {field}")
    if (
        vm_adapter_state.count(
            "builtin_request_state: php_runtime::BuiltinRequestState"
        )
        != 1
    ):
        failures.append("VM must own exactly one migrated BuiltinRequestState")
    request_state_borrow = "BuiltinContext::with_runtime_request_state("
    if request_state_borrow in vm:
        failures.append("VM facade must not construct builtin request-state services")
    if vm_builtin_adapter.count(request_state_borrow) != 1:
        failures.append(
            "builtin adapter must borrow its request-state owner exactly once"
        )

    modules = {
        "json": read("crates/php_runtime/src/builtins/modules/json.rs"),
        "pcre": read("crates/php_runtime/src/builtins/modules/pcre.rs"),
        "curl": read("crates/php_runtime/src/builtins/modules/curl.rs"),
    }
    expected_views = {
        "json": "JsonBuiltinServices",
        "pcre": "PcreBuiltinServices",
        "curl": "CurlBuiltinServices",
    }
    for name, source in modules.items():
        if expected_views[name] not in source:
            failures.append(f"{name} implementations do not use their narrow service view")

    legacy_fields = {
        match.group(1)
        for match in re.finditer(r"^\s+([a-z][a-z0-9_]+):", legacy, re.MULTILINE)
    }
    undocumented = sorted(field for field in legacy_fields if f"`{field}`" not in migration)
    if undocumented:
        failures.append(
            "legacy adapter removal list is missing: " + ", ".join(undocumented)
        )

    if failures:
        print("[fail] request-state boundaries:", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        return 1
    print(
        "[ok] typed request slots, sole VM ownership, narrow migrated views, "
        "and legacy removal inventory"
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except OSError as error:
        print(f"[fail] request-state boundaries: {error}", file=sys.stderr)
        raise SystemExit(1) from error
