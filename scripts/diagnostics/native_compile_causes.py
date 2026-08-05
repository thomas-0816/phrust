#!/usr/bin/env python3
"""Attribute native compile attempts to disjoint causes.

Reads the diagnostic `native_compile_descriptors` already emitted by
`php-vm --counters-json` and assigns every recorded compile to exactly one
cause. This is a report over existing records: it adds no runtime telemetry,
no hot-path branch, and no new counter.

Each attempt lands in the first matching bucket, so the buckets are disjoint
and sum to the recorded attempt count:

  1. unpublished          publication did not yield a usable artifact
  2. replan               a repeated pre-regalloc attempt of the same product
  3. duplicate            an identical product was already compiled
  4. external-signature   same function/tier/key, different external signatures
  5. receiver-layout      same function/tier, different receiver layout
  6. dynamic-source       a distinct dynamic unit (eval/include) source
  7. unique-function      first compile of a function in a tier
  8. remainder            anything the rules above do not explain

Buckets 1-3 and 7 are proven by descriptor identity. Buckets 4-6 are proven to
be *variant* compiles, but naming them the root cause of a workload's compile
count remains a hypothesis until the workload is measured.
"""

from __future__ import annotations

import argparse
import json
import sys
from collections import Counter
from typing import Any

BUCKETS = [
    "unpublished",
    "replan",
    "duplicate",
    "external-signature",
    "receiver-layout",
    "dynamic-source",
    "unique-function",
    "remainder",
]

PROVEN = {"unpublished", "replan", "duplicate", "unique-function"}


def _published(descriptor: dict[str, Any]) -> bool:
    result = str(descriptor.get("publication_result", ""))
    return result.startswith("published")


def _product_key(descriptor: dict[str, Any]) -> tuple[Any, ...]:
    return (
        descriptor.get("source_identity"),
        descriptor.get("function_id"),
        descriptor.get("tier"),
        descriptor.get("generic_key"),
        descriptor.get("specialization"),
        descriptor.get("ir_fingerprint"),
    )


def classify(descriptors: list[dict[str, Any]]) -> list[tuple[dict[str, Any], str]]:
    """Assigns one disjoint cause to every descriptor, in recorded order."""
    seen_products: set[tuple[Any, ...]] = set()
    seen_function_tier: dict[tuple[Any, ...], dict[str, Any]] = {}
    classified: list[tuple[dict[str, Any], str]] = []
    # The first recorded source is the entry unit; every other source identity
    # is a genuinely different unit (include/eval/linked source).
    primary_source = descriptors[0].get("source_identity") if descriptors else None

    for descriptor in descriptors:
        product = _product_key(descriptor)
        function_tier = (
            descriptor.get("source_identity"),
            descriptor.get("function_id"),
            descriptor.get("tier"),
        )
        first = seen_function_tier.get(function_tier)

        if not _published(descriptor):
            cause = "unpublished"
        elif int(descriptor.get("replan_index", 0) or 0) > 0:
            cause = "replan"
        elif product in seen_products:
            cause = "duplicate"
        elif first is not None and descriptor.get(
            "external_signatures_hash"
        ) != first.get("external_signatures_hash"):
            cause = "external-signature"
        elif first is not None and descriptor.get("receiver_layout_hash") != first.get(
            "receiver_layout_hash"
        ):
            cause = "receiver-layout"
        elif first is None and descriptor.get("source_identity") != primary_source:
            cause = "dynamic-source"
        elif first is None:
            cause = "unique-function"
        else:
            cause = "remainder"

        seen_products.add(product)
        if first is None:
            seen_function_tier[function_tier] = descriptor
        classified.append((descriptor, cause))

    return classified


def render(counters: dict[str, Any]) -> str:
    descriptors = counters.get("native_compile_descriptors") or []
    attempts = int(counters.get("native_compile_attempts", len(descriptors)) or 0)
    classified = classify(descriptors)
    causes = Counter(cause for _, cause in classified)
    tiers = Counter(str(d.get("tier", "unknown")) for d in descriptors)
    functions = {
        (d.get("source_identity"), d.get("function_id"), d.get("tier"))
        for d in descriptors
    }

    lines: list[str] = []
    lines.append("# Native compile-cause attribution")
    lines.append("")
    lines.append(f"recorded descriptors: {len(descriptors)}")
    lines.append(f"counted attempts:     {attempts}")
    if attempts != len(descriptors):
        lines.append(
            "  NOTE: descriptor count differs from the attempt counter; "
            "descriptor recording may be disabled or truncated."
        )
    lines.append("")
    lines.append("## Disjoint causes")
    lines.append("")
    lines.append("| cause | compiles | share | evidence |")
    lines.append("|---|---:|---:|---|")
    total = len(descriptors) or 1
    for bucket in BUCKETS:
        count = causes.get(bucket, 0)
        share = 100.0 * count / total
        evidence = "proven" if bucket in PROVEN else "variant (cause hypothesis)"
        lines.append(f"| {bucket} | {count} | {share:.1f}% | {evidence} |")
    lines.append(f"| **total** | **{sum(causes.values())}** | 100.0% | |")
    lines.append("")
    lines.append("## Tiers")
    lines.append("")
    lines.append("| tier | compiles |")
    lines.append("|---|---:|")
    for tier, count in sorted(tiers.items()):
        lines.append(f"| {tier} | {count} |")
    lines.append("")
    lines.append(f"distinct function/tier products: {len(functions)}")
    lines.append("")

    replans = [d for d, cause in classified if cause == "replan"]
    if replans:
        lines.append("## Replanned functions")
        lines.append("")
        for descriptor in replans[:20]:
            lines.append(
                f"- {descriptor.get('function_name')} "
                f"(replan_index={descriptor.get('replan_index')}, "
                f"tier={descriptor.get('tier')})"
            )
        lines.append("")

    unpublished = [d for d, cause in classified if cause == "unpublished"]
    if unpublished:
        lines.append("## Unpublished products")
        lines.append("")
        for descriptor in unpublished[:20]:
            lines.append(
                f"- {descriptor.get('function_name')}: "
                f"{descriptor.get('publication_result')}"
            )
        lines.append("")

    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "counters",
        help="counters JSON produced by php-vm --counters-json",
    )
    parser.add_argument(
        "--out",
        help="write the report here instead of stdout",
    )
    args = parser.parse_args()

    with open(args.counters, encoding="utf-8") as handle:
        counters = json.load(handle)

    report = render(counters)
    if args.out:
        with open(args.out, "w", encoding="utf-8") as handle:
            handle.write(report + "\n")
    else:
        print(report)
    return 0


if __name__ == "__main__":
    sys.exit(main())
