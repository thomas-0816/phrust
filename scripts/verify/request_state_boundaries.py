#!/usr/bin/env python3
"""Enforce typed request-state ownership and migrated builtin service views."""

from __future__ import annotations

import re
import sys
from pathlib import Path

from rust_module import read_rust_module


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
    vm_jit_abi = read_rust_module(ROOT / "crates/php_vm/src/vm/jit_abi.rs")
    vm_native_builtins = read("crates/php_vm/src/vm/jit_abi/native_builtins.rs")
    extensions = read("crates/php_extensions/src/lib.rs")
    apcu = read("crates/php_extensions/src/apcu.rs")
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

    if re.search(r"^\s+apcu_state:\s*ApcuState", legacy, re.MULTILINE):
        failures.append("migrated APCu still has a fallback-owned context state")
    if "set_apcu_state(" in context:
        failures.append("legacy direct APCu state setter remains")
    for symbol in (
        "extension_request_state: Option<&'a mut RequestState>",
        "apcu_state_slot: Option<ExtensionStateSlot<ApcuState>>",
        "set_apcu_request_state",
    ):
        if symbol not in context:
            failures.append(f"APCu registered request-state borrow is missing {symbol}")

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

    native_execution_state = struct_body(vm_jit_abi, "NativeExecutionContext")
    registered_extension_state = struct_body(
        vm_jit_abi, "NativeRegisteredExtensionRequestState"
    )
    if native_execution_state.count(
        "builtin_request_state: php_runtime::api::BuiltinRequestState"
    ) != 1:
        failures.append("native VM must own exactly one migrated BuiltinRequestState")
    if native_execution_state.count(
        "registered_extensions: NativeRegisteredExtensionRequestState"
    ) != 1:
        failures.append("native VM must own exactly one registered extension state")
    for symbol in (
        "state: php_runtime::api::RequestState",
        "apcu: php_runtime::api::ExtensionStateSlot<php_runtime::api::ApcuState>",
    ):
        if registered_extension_state.count(symbol) != 1:
            failures.append(f"native registered extension owner is missing {symbol}")
    request_state_borrow = "BuiltinContext::with_runtime_request_state("
    if request_state_borrow in vm:
        failures.append("VM facade must not construct builtin request-state services")
    if vm_native_builtins.count(request_state_borrow) != 1:
        failures.append(
            "native builtin adapter must borrow its request-state owner exactly once"
        )
    for symbol in (
        "NativeRegisteredExtensionRequestState",
        'request_state_slot("apcu")',
        "registry.create_request_state()",
        "registered_extensions.bind(&mut builtin)",
    ):
        if symbol not in vm_jit_abi and symbol not in vm_native_builtins:
            failures.append(f"native VM APCu registry integration is missing {symbol}")
    for symbol in ("create_request_state", "request_state_slot"):
        if symbol not in extensions:
            failures.append(f"extension integration registry is missing {symbol}")
    if "context.apcu_state().ok_or_else" not in apcu:
        failures.append("APCu implementation does not fail closed on missing registered state")

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
    legacy_fields.difference_update({"extension_request_state", "apcu_state_slot"})
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
        "registered APCu ownership, and legacy removal inventory"
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except OSError as error:
        print(f"[fail] request-state boundaries: {error}", file=sys.stderr)
        raise SystemExit(1) from error
