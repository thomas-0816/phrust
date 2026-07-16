#!/usr/bin/env python3
"""Attribute PNA artifact bytes and Linux process memory mappings."""

from __future__ import annotations

import argparse
import json
import re
import struct
from collections import defaultdict
from pathlib import Path
from typing import Any

SECTION_NAMES = {
    1: "identity",
    2: "code",
    3: "rodata",
    4: "function_entries",
    5: "continuations",
    6: "relocations",
    7: "helper_imports",
    8: "internal_symbols",
    9: "traps",
    10: "exception_metadata",
    11: "root_maps",
    12: "resume_entries",
    13: "signature_metadata",
}
SMAPS_HEADER = re.compile(r"^[0-9a-f]+-[0-9a-f]+\s+(\S+)\s+\S+\s+\S+\s+\S+\s*(.*)$")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--cache-dir", type=Path, required=True)
    parser.add_argument("--pid", type=int)
    parser.add_argument("--output", type=Path)
    return parser.parse_args()


def parse_artifact(path: Path) -> dict[str, Any]:
    data = path.read_bytes()
    if len(data) < 64 or data[:4] not in {b"PNA1", b"PNA2"}:
        raise ValueError(f"{path}: bad PNA header")
    version, header_len = struct.unpack_from("<HH", data, 4)
    if (data[:4], version) not in {(b"PNA1", 1), (b"PNA2", 2)}:
        raise ValueError(f"{path}: unsupported PNA magic/version pair")
    total_len, section_count, record_len = struct.unpack_from("<QII", data, 8)
    if header_len != 64 or record_len != 32 or total_len != len(data):
        raise ValueError(f"{path}: inconsistent PNA lengths")
    sections: dict[str, int] = {}
    payload = 0
    previous_end = header_len + section_count * record_len
    padding = 0
    for index in range(section_count):
        start = header_len + index * record_len
        kind = struct.unpack_from("<H", data, start)[0]
        offset, length = struct.unpack_from("<QQ", data, start + 8)
        if offset < previous_end or offset + length > len(data):
            raise ValueError(f"{path}: invalid section range")
        padding += offset - previous_end
        previous_end = offset + length
        name = SECTION_NAMES.get(kind, f"unknown_{kind}")
        sections[name] = sections.get(name, 0) + length
        payload += length
    padding += len(data) - previous_end
    return {
        "path": str(path),
        "format": data[:4].decode("ascii"),
        "version": version,
        "bytes": len(data),
        "header_and_table_bytes": header_len + section_count * record_len,
        "padding_bytes": padding,
        "payload_bytes": payload,
        "sections": sections,
    }


def mapping_category(perms: str, path: str) -> str:
    if path.startswith("[stack"):
        return "worker_stacks"
    if path == "[heap]":
        return "anonymous_heap"
    if "rust" in path.lower() or "jemalloc" in path.lower():
        return "rust_allocator"
    if path.startswith("[anon:php-jit-rodata"):
        return "jit_writable_rodata"
    if path.startswith("[anon:php-jit-code"):
        return "jit_executable"
    if path.startswith("[anon:"):
        return "anonymous_heap"
    if not path and "x" in perms:
        return "jit_executable"
    if not path and "w" in perms:
        return "anonymous_allocator_and_jit_writable"
    if path.endswith(".pna"):
        return "native_artifact_mappings"
    if ".so" in path or "/lib" in path:
        return "shared_libraries"
    if path:
        return "file_mappings"
    return "anonymous_other"


def parse_smaps(pid: int) -> dict[str, Any]:
    path = Path(f"/proc/{pid}/smaps")
    if not path.exists():
        return {"available": False, "reason": f"{path} is unavailable"}
    totals: dict[str, dict[str, int]] = defaultdict(lambda: defaultdict(int))
    category = "anonymous_other"
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        header = SMAPS_HEADER.match(line)
        if header:
            category = mapping_category(header.group(1), header.group(2).strip())
            continue
        if ":" not in line:
            continue
        name, value = line.split(":", 1)
        fields = value.split()
        if fields and fields[0].isdigit() and name in {"Size", "Rss", "Pss", "Private_Dirty"}:
            totals[category][f"{name.lower()}_bytes"] += int(fields[0]) * 1024
    rollup_path = Path(f"/proc/{pid}/smaps_rollup")
    rollup: dict[str, int] = {}
    if rollup_path.exists():
        for line in rollup_path.read_text(encoding="utf-8", errors="replace").splitlines():
            if ":" not in line:
                continue
            name, value = line.split(":", 1)
            fields = value.split()
            if fields and fields[0].isdigit():
                rollup[f"{name.lower()}_bytes"] = int(fields[0]) * 1024
    return {
        "available": True,
        "pid": pid,
        "smaps_rollup": rollup,
        "categories": {key: dict(value) for key, value in sorted(totals.items())},
    }


def main() -> int:
    args = parse_args()
    if not args.cache_dir.is_dir():
        raise SystemExit(f"native footprint report: cache directory does not exist: {args.cache_dir}")
    try:
        artifacts = [parse_artifact(path) for path in sorted(args.cache_dir.glob("*.pna"))]
    except (OSError, ValueError, struct.error) as error:
        raise SystemExit(f"native footprint report: {error}") from error
    section_totals: dict[str, int] = defaultdict(int)
    for artifact in artifacts:
        for name, size in artifact["sections"].items():
            section_totals[name] += size
        section_totals["header_and_table"] += artifact["header_and_table_bytes"]
        section_totals["padding"] += artifact["padding_bytes"]
    report = {
        "schema_version": 2,
        "cache_dir": str(args.cache_dir),
        "artifact_count": len(artifacts),
        "artifact_bytes": sum(item["bytes"] for item in artifacts),
        "section_bytes": dict(sorted(section_totals.items())),
        "artifacts": artifacts,
        "process_memory": parse_smaps(args.pid) if args.pid else {"available": False, "reason": "no pid supplied"},
    }
    rendered = json.dumps(report, indent=2, sort_keys=True) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(rendered, encoding="utf-8")
    print(rendered, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
