#!/usr/bin/env python3
"""Parse macOS ``sample`` call trees into weighted folded stack JSON."""

from __future__ import annotations

import argparse
import json
import platform
import re
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


THREAD_RE = re.compile(r"^\s{4}(\d+)\s+Thread_([^: ]+)(?:(?::|\s{2,})\s*(.*))?$")
FRAME_RE = re.compile(r"^(.*?)(\d+)\s+(.+?)\s*$")
ADDRESS_RE = re.compile(r"\[(0x[0-9a-fA-F]+)\]")
MODULE_RE = re.compile(r"\(in ([^)]+)\)")
SOURCE_RE = re.compile(r"\s+([^\s]+\.(?:rs|c|cc|cpp|h|m|mm)):(\d+)$")
UNRESOLVED_RE = re.compile(r"^(?:\?\?\?|0x[0-9a-fA-F]+)(?:\s|$)")
CALL_GRAPH_MARKER = "Call graph:"
LOAD_ADDRESS_RE = re.compile(r"^Load Address:\s*(0x[0-9a-fA-F]+)\s*$", re.MULTILINE)
CALL_GRAPH_END_MARKERS = (
    "Total number in stack",
    "Sort by top of stack",
    "Binary Images:",
)


@dataclass
class Frame:
    symbol: str
    module: str | None
    address: str | None
    source: str | None
    line: int | None

    def folded_name(self) -> str:
        module = self.module or "unknown"
        return f"{module}`{self.symbol}"

    def as_json(self) -> dict[str, Any]:
        return {
            "symbol": self.symbol,
            "module": self.module,
            "address": self.address,
            "source": self.source,
            "line": self.line,
        }


@dataclass
class Node:
    count: int
    frame: Frame | None = None
    children: list["Node"] = field(default_factory=list)


@dataclass
class ThreadTree:
    thread_id: str
    thread_name: str | None
    count: int
    children: list[Node] = field(default_factory=list)


def parse_frame(text: str) -> Frame:
    address_match = ADDRESS_RE.search(text)
    module_match = MODULE_RE.search(text)
    source_match = SOURCE_RE.search(text)
    symbol_end = len(text)
    for match in (module_match, address_match, source_match):
        if match is not None:
            symbol_end = min(symbol_end, match.start())
    symbol = re.sub(r"\s+\+\s+\d+\s*$", "", text[:symbol_end].strip())
    return Frame(
        symbol=symbol,
        module=module_match.group(1) if module_match else None,
        address=address_match.group(1) if address_match else None,
        source=source_match.group(1) if source_match else None,
        line=int(source_match.group(2)) if source_match else None,
    )


def parse_macos_sample(raw: str) -> list[ThreadTree]:
    lines = raw.splitlines()
    try:
        start = lines.index(CALL_GRAPH_MARKER) + 1
    except ValueError as error:
        raise ValueError("macOS sample has no Call graph section") from error

    threads: list[ThreadTree] = []
    current: ThreadTree | None = None
    stack: list[Node] = []
    for line in lines[start:]:
        if any(line.startswith(marker) for marker in CALL_GRAPH_END_MARKERS):
            break
        thread_match = THREAD_RE.match(line)
        if thread_match:
            current = ThreadTree(
                thread_id=thread_match.group(2),
                thread_name=(thread_match.group(3) or "").strip() or None,
                count=int(thread_match.group(1)),
            )
            threads.append(current)
            stack = []
            continue
        if current is None or not line.strip():
            continue
        frame_match = FRAME_RE.match(line)
        if not frame_match:
            continue
        prefix, count_text, frame_text = frame_match.groups()
        count_column = frame_match.start(2)
        if count_column < 6 or (count_column - 4) % 2:
            continue
        depth = (count_column - 4) // 2
        node = Node(count=int(count_text), frame=parse_frame(frame_text))
        if depth == 1:
            current.children.append(node)
            stack = [node]
            continue
        if depth < 2 or depth > len(stack) + 1:
            continue
        stack = stack[: depth - 1]
        stack[-1].children.append(node)
        stack.append(node)
    if not threads:
        raise ValueError("macOS sample contains no thread call trees")
    return threads


def folded_stacks(threads: list[ThreadTree]) -> list[dict[str, Any]]:
    folded: list[dict[str, Any]] = []
    for thread in threads:
        thread_label = f"Thread_{thread.thread_id}"
        if thread.thread_name:
            thread_label += f": {thread.thread_name}"

        def visit(node: Node, path: list[Frame]) -> None:
            assert node.frame is not None
            current_path = [*path, node.frame]
            child_weight = sum(child.count for child in node.children)
            exclusive = node.count - child_weight
            if exclusive < 0:
                raise ValueError(
                    f"child weights exceed parent weight for {node.frame.symbol}: "
                    f"{child_weight} > {node.count}"
                )
            if exclusive:
                folded.append(
                    {
                        "thread_id": thread.thread_id,
                        "thread_name": thread.thread_name,
                        "weight": exclusive,
                        "folded": ";".join(
                            [thread_label, *(frame.folded_name() for frame in current_path)]
                        ),
                        "frames": [frame.as_json() for frame in current_path],
                        "unresolved": any(
                            frame.module is None
                            or UNRESOLVED_RE.match(frame.symbol) is not None
                            for frame in current_path
                        ),
                    }
                )
            for child in node.children:
                visit(child, current_path)

        for child in thread.children:
            visit(child, [])
    return folded


def symbolize_unresolved(
    threads: list[ThreadTree], binary: Path | None, load_address: str | None
) -> dict[str, Any]:
    frames: list[Frame] = []

    def collect(node: Node) -> None:
        if node.frame is not None:
            frame = node.frame
            if frame.address and (
                frame.module is None or UNRESOLVED_RE.match(frame.symbol) is not None
            ):
                frames.append(frame)
        for child in node.children:
            collect(child)

    for thread in threads:
        for child in thread.children:
            collect(child)
    result = {
        "attempted": False,
        "binary": str(binary) if binary else None,
        "load_address": load_address,
        "addresses": len(frames),
        "resolved": 0,
        "error": None,
    }
    if not frames or binary is None or load_address is None or platform.system() != "Darwin":
        return result
    command = ["atos", "-o", str(binary), "-l", load_address, *(frame.address for frame in frames)]
    completed = subprocess.run(command, text=True, capture_output=True, check=False)
    result["attempted"] = True
    if completed.returncode != 0:
        result["error"] = completed.stderr.strip() or f"atos exited {completed.returncode}"
        return result
    symbols = completed.stdout.splitlines()
    for frame, symbol in zip(frames, symbols):
        symbol = symbol.strip()
        if not symbol or symbol == frame.address or symbol.startswith("0x"):
            continue
        frame.symbol = symbol
        frame.module = binary.name
        result["resolved"] += 1
    return result


def parse_file(path: Path, binary: Path | None = None) -> dict[str, Any]:
    raw = path.read_text(encoding="utf-8", errors="replace")
    threads = parse_macos_sample(raw)
    load_match = LOAD_ADDRESS_RE.search(raw)
    symbolization = symbolize_unresolved(
        threads,
        binary.resolve() if binary else None,
        load_match.group(1) if load_match else None,
    )
    stacks = folded_stacks(threads)
    return {
        "schema_version": 1,
        "source_format": "macos-sample-callgraph",
        "source": str(path),
        "threads": [
            {
                "thread_id": thread.thread_id,
                "thread_name": thread.thread_name,
                "sample_count": thread.count,
            }
            for thread in threads
        ],
        "stacks": stacks,
        "stack_weight_total": sum(stack["weight"] for stack in stacks),
        "unresolved_weight": sum(stack["weight"] for stack in stacks if stack["unresolved"]),
        "symbolization": symbolization,
    }


def require_arm64(machine: str | None = None) -> None:
    actual = (machine or platform.machine()).lower()
    if actual not in {"arm64", "aarch64"}:
        raise RuntimeError(f"ARM64 sampler input required; got {actual or 'unknown'}")


def write_json(value: Any, path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def self_test() -> int:
    fixture_dir = Path(__file__).resolve().parent / "fixtures" / "arm64_sample"
    fixtures = sorted(fixture_dir.glob("*.sample"))
    assert fixtures, "missing ARM64 sample fixtures"
    parsed = [parse_file(path) for path in fixtures]
    all_stacks = [stack for report in parsed for stack in report["stacks"]]
    assert sum(stack["weight"] for stack in all_stacks) == 28
    assert any(stack["thread_name"] == "php-worker-0" for stack in all_stacks)
    assert any("_xzm_free" in stack["folded"] for stack in all_stacks)
    assert any(stack["unresolved"] for stack in all_stacks)
    assert any("native-copy-patch" in stack["folded"] for stack in all_stacks)
    idle = [stack for stack in all_stacks if "semaphore_wait_trap" in stack["folded"]]
    assert idle and all(stack["thread_name"] == "php-worker-0" for stack in idle)
    require_arm64("arm64")
    try:
        require_arm64("x86_64")
    except RuntimeError:
        pass
    else:
        raise AssertionError("non-ARM64 input was not rejected")
    print("arm64 sample parser self-test: ok")
    return 0


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    if args.self_test:
        return self_test()
    require_arm64()
    if args.input is None or args.output is None:
        raise SystemExit("--input and --output are required")
    write_json(parse_file(args.input, args.binary), args.output)
    return 0


if __name__ == "__main__":
    sys.exit(main())
