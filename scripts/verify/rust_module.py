"""Helpers for deterministic source checks across split Rust modules."""

from __future__ import annotations

from pathlib import Path


def read_rust_module(entry: Path) -> str:
    """Read a Rust module file and every source file in its module directory."""
    paths = [entry]
    module_directory = entry.with_suffix("")
    if module_directory.is_dir():
        paths.extend(sorted(module_directory.rglob("*.rs")))
    return "\n".join(path.read_text(encoding="utf-8") for path in paths)
