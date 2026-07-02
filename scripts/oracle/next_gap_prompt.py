#!/usr/bin/env python3
"""Emit a family-level implementation prompt from the oracle gap queue."""

from __future__ import annotations

import argparse
import json
import sys
from collections import defaultdict
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_REPORT = REPO_ROOT / "target/oracle/gap-report.json"
PRIORITY_RANK = {"P0": 0, "P1": 1, "P2": 2, "P3": 3, "P4": 4}
STATUS_RANK = {"unclassified_failure": 0, "known_gap": 1}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--report", type=Path, default=DEFAULT_REPORT)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--self-test-only", action="store_true")
    args = parser.parse_args()

    try:
        if args.self_test or args.self_test_only:
            run_self_tests()
        if args.self_test_only:
            return 0

        report = read_json(args.report)
        prompt = render_prompt(select_family(report))
        if args.out:
            args.out.parent.mkdir(parents=True, exist_ok=True)
            args.out.write_text(prompt, encoding="utf-8")
            print(f"[ok] wrote {relative(args.out)}")
        else:
            print(prompt, end="")
    except Exception as error:  # noqa: BLE001 - script boundary.
        print(f"next gap prompt error: {error}", file=sys.stderr)
        return 1
    return 0


def select_family(report: dict[str, Any]) -> dict[str, Any]:
    entries = [
        item
        for item in report.get("entries", [])
        if item.get("status") not in {"matched", "closed", "implemented"}
    ]
    if not entries:
        raise ValueError("gap report has no open entries")

    groups: dict[tuple[str, str, str, str], list[dict[str, Any]]] = defaultdict(list)
    for item in entries:
        key = (
            item.get("priority") or "P4",
            item.get("layer") or "unknown",
            item.get("pattern_family") or "unclassified",
            item.get("extension") or "",
        )
        groups[key].append(item)

    _, items = min(groups.items(), key=family_sort_key)
    items = sorted(items, key=item_sort_key)
    first = items[0]
    return {
        "priority": first.get("priority") or "P4",
        "layer": first.get("layer") or "unknown",
        "pattern_family": first.get("pattern_family") or "unclassified",
        "extension": first.get("extension"),
        "owner": first.get("suggested_owner") or owner_for_layer(first.get("layer")),
        "items": items,
    }


def family_sort_key(group: tuple[tuple[str, str, str, str], list[dict[str, Any]]]) -> tuple[int, int, int, str, str]:
    key, items = group
    priority, layer, pattern, _extension = key
    status_rank = min(STATUS_RANK.get(item.get("status"), 9) for item in items)
    return (
        PRIORITY_RANK.get(priority, 9),
        status_rank,
        -len(items),
        layer,
        pattern,
    )


def item_sort_key(item: dict[str, Any]) -> tuple[int, int, str]:
    return (
        PRIORITY_RANK.get(item.get("priority"), 9),
        STATUS_RANK.get(item.get("status"), 9),
        item.get("gap_id") or "",
    )


def render_prompt(family: dict[str, Any]) -> str:
    items = family["items"]
    first = items[0]
    fixtures = unique_values(item.get("fixture") for item in items)
    sources = unique_values(item.get("source") for item in items)
    references = unique_values(item.get("oracle_reference") for item in items)
    diagnostics = unique_values(item.get("diagnostic_id") for item in items)
    symbols = unique_values(item.get("symbol") for item in items)

    lines = [
        f"Close oracle gap family: {family['priority']} {family['layer']}/{family['pattern_family']}",
        "",
        "Context:",
        f"- Layer owner: {family['owner']}",
        f"- Open rows in family: {len(items)}",
        f"- Representative gap ID: {first.get('gap_id')}",
        f"- Status mix: {status_mix(items)}",
        f"- Source provenance: {comma_or_none(sources)}",
        f"- Oracle reference: {comma_or_none(references)}",
        f"- Fixtures: {comma_or_none(fixtures)}",
        f"- Symbols: {comma_or_none(symbols)}",
        f"- Diagnostics: {comma_or_none(diagnostics)}",
        "",
        "Expected reference behavior:",
        expected_behavior(items),
        "",
        "Implementation constraints:",
        "- Fix the generic owning layer, not only the generated probe.",
        "- Preserve the php_lexer -> php_syntax -> php_ast -> php_semantics/HIR -> php_ir -> php_runtime -> php_vm pipeline.",
        "- Keep parser/CST, semantic, runtime, and stdlib responsibilities separate.",
        "- Do not hardcode numeric PHP token IDs or vendored php-src data.",
        "- Update or remove known-gap metadata only after a focused fixture proves the behavior.",
        "",
        "Required proof:",
    ]
    lines.extend(f"- {gate}" for gate in gates_for_family(family))
    lines.extend(
        [
            "- `nix develop -c just oracle-gap-report --check`",
            "- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php nix develop -c just oracle-probe-smoke`",
            "",
            "Relevant rows:",
        ]
    )
    for item in items[:10]:
        lines.append(
            "- "
            f"{item.get('gap_id')} "
            f"{item.get('status')} "
            f"{item.get('symbol') or item.get('fixture') or ''} "
            f"reason={item.get('reason') or 'n/a'}"
        )
    if len(items) > 10:
        lines.append(f"- ... {len(items) - 10} more rows in this family")
    lines.append("")
    return "\n".join(lines)


def expected_behavior(items: list[dict[str, Any]]) -> str:
    reasons = unique_values(item.get("reason") for item in items)
    if reasons:
        return "\n".join(f"- Match reference behavior: {reason}" for reason in reasons[:5])
    references = unique_values(item.get("oracle_reference") for item in items)
    if references:
        return "\n".join(f"- Match behavior recorded by {reference}." for reference in references[:5])
    return "- Match the reference PHP output, diagnostics, exit status, and source positions."


def gates_for_family(family: dict[str, Any]) -> list[str]:
    layer = family["layer"]
    if layer in {"frontend_lowering", "ir_lowering", "semantic_folding"}:
        return [
            "`nix develop -c cargo test -p php_ir`",
            "`REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php nix develop -c just oracle-probe-full`",
        ]
    if layer in {"runtime_semantics", "vm_runtime"}:
        return [
            "`nix develop -c cargo test -p php_vm`",
            "`REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php nix develop -c just runtime-semantics-diff --category oracle_generated`",
        ]
    if layer in {"stdlib_metadata", "stdlib_runtime", "reflection_metadata", "runtime_api"}:
        return [
            "`nix develop -c cargo test -p php_std`",
            "`REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php nix develop -c just diff-stdlib`",
            "`nix develop -c just verify-stdlib`",
        ]
    return ["`nix develop -c just verify-runtime`"]


def owner_for_layer(layer: str | None) -> str:
    if layer in {"frontend_lowering", "semantic_folding", "ir_lowering"}:
        return "php_semantics/php_ir"
    if layer in {"runtime_semantics", "vm_runtime"}:
        return "php_runtime/php_vm"
    if layer in {"stdlib_metadata", "stdlib_runtime", "reflection_metadata", "runtime_api"}:
        return "php_std/php_runtime"
    return "owning runtime layer"


def status_mix(items: list[dict[str, Any]]) -> str:
    counts: dict[str, int] = {}
    for item in items:
        status = item.get("status") or "unknown"
        counts[status] = counts.get(status, 0) + 1
    return ", ".join(f"{status}={count}" for status, count in sorted(counts.items()))


def unique_values(values: Any) -> list[str]:
    seen: set[str] = set()
    result: list[str] = []
    for value in values:
        if value is None or value == "":
            continue
        text = str(value)
        if text in seen:
            continue
        seen.add(text)
        result.append(text)
    return result


def comma_or_none(values: list[str]) -> str:
    return ", ".join(values[:8]) if values else "none"


def read_json(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def relative(path: Path | str) -> str:
    path = Path(path)
    try:
        return path.relative_to(REPO_ROOT).as_posix()
    except ValueError:
        return str(path)


def run_self_tests() -> None:
    report = {
        "entries": [
            {
                "gap_id": "LOW",
                "status": "known_gap",
                "layer": "stdlib_metadata",
                "pattern_family": "method_metadata",
                "priority": "P3",
                "suggested_owner": "php_std/php_runtime",
                "fixture": "fixtures/runtime_semantics/oracle_generated/full/low.php",
                "source": "target/oracle/api/php-source-api-symbols.jsonl",
                "oracle_reference": "api oracle",
            },
            {
                "gap_id": "HIGH-1",
                "status": "unclassified_failure",
                "layer": "runtime_semantics",
                "pattern_family": "reference_binding",
                "priority": "P1",
                "suggested_owner": "php_runtime/php_vm",
                "fixture": "fixtures/runtime_semantics/oracle_generated/smoke/high.php",
                "source": "target/oracle/probes/smoke/runtime-semantics-diff-report.json",
                "oracle_reference": "oracle probe diff",
                "reason": "by-ref behavior differs",
            },
            {
                "gap_id": "HIGH-2",
                "status": "known_gap",
                "layer": "runtime_semantics",
                "pattern_family": "reference_binding",
                "priority": "P1",
                "suggested_owner": "php_runtime/php_vm",
                "fixture": "fixtures/runtime_semantics/oracle_generated/smoke/high-2.php",
                "source": "target/oracle/probes/smoke/runtime-semantics-diff-report.json",
                "oracle_reference": "oracle probe diff",
            },
        ]
    }
    family = select_family(report)
    assert family["layer"] == "runtime_semantics"
    assert family["pattern_family"] == "reference_binding"
    prompt = render_prompt(family)
    assert "Layer owner: php_runtime/php_vm" in prompt
    assert "Open rows in family: 2" in prompt
    assert "Required proof:" in prompt


if __name__ == "__main__":
    raise SystemExit(main())
