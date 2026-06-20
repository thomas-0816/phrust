#!/usr/bin/env python3
"""Extract a small parser corpus from a local php-src checkout."""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_OUTPUT = ROOT / "target" / "parser-corpus-smoke" / "extracted"
DEFAULT_RELATIVE_ROOTS = (
    "Zend/tests",
    "tests/lang",
    "Zend/tests/attributes",
    "Zend/tests/enum",
    "Zend/tests/namespaces",
    "Zend/tests/readonly_classes",
    "Zend/tests/traits",
    "Zend/tests/type_declarations",
    "Zend/tests/varSyntax",
)
DEFAULT_MAX_FILES = 50


def find_php_src_dir(configured: str | None = None) -> Path | None:
    if configured:
        path = Path(configured)
    elif os.environ.get("PHP_SRC_DIR"):
        path = Path(os.environ["PHP_SRC_DIR"])
    else:
        path = ROOT / "third_party" / "php-src"

    if path.exists() and path.is_dir():
        return path
    return None


def iter_source_candidates(
    php_src: Path,
    relative_roots: tuple[str, ...] = DEFAULT_RELATIVE_ROOTS,
) -> list[Path]:
    seen: set[Path] = set()
    candidates: list[Path] = []
    for relative_root in relative_roots:
        root = php_src / relative_root
        if not root.exists():
            continue
        for path in sorted(root.rglob("*")):
            if path.suffix not in {".php", ".phpt"} or not path.is_file():
                continue
            resolved = path.resolve()
            if resolved in seen:
                continue
            seen.add(resolved)
            candidates.append(path)
    return candidates


def extract_phpt_code(text: str) -> str | None:
    sections: dict[str, list[str]] = {}
    current: str | None = None
    marker = re.compile(r"^--([A-Z_]+)--\s*$")
    for line in text.splitlines(keepends=True):
        match = marker.match(line.rstrip("\r\n"))
        if match:
            current = match.group(1)
            sections.setdefault(current, [])
            continue
        if current is not None:
            sections[current].append(line)

    if "FILE" in sections:
        return "".join(sections["FILE"])
    if "FILEEOF" in sections:
        return "".join(sections["FILEEOF"])
    return None


def output_name_for(source: Path, php_src: Path, index: int) -> str:
    relative = source.relative_to(php_src).as_posix()
    slug = re.sub(r"[^A-Za-z0-9_.-]+", "__", relative)
    if source.suffix == ".phpt":
        slug = f"{slug[:-5]}.php"
    if not slug.endswith(".php"):
        slug = f"{slug}.php"
    return f"{index:04d}__{slug}"


def extract_corpus(
    php_src: Path,
    output: Path,
    *,
    max_files: int = DEFAULT_MAX_FILES,
    clean: bool = True,
) -> list[dict[str, str]]:
    if clean and output.exists():
        shutil.rmtree(output)
    output.mkdir(parents=True, exist_ok=True)

    manifest: list[dict[str, str]] = []
    for source in iter_source_candidates(php_src):
        if len(manifest) >= max_files:
            break
        if source.suffix == ".phpt":
            code = extract_phpt_code(source.read_text(encoding="utf-8", errors="replace"))
            section = "FILE"
            if code is None:
                continue
        else:
            code = source.read_text(encoding="utf-8", errors="replace")
            section = "php"

        if not code.strip():
            continue

        output_path = output / output_name_for(source, php_src, len(manifest) + 1)
        output_path.write_text(code, encoding="utf-8")
        manifest.append(
            {
                "source": source.relative_to(php_src).as_posix(),
                "section": section,
                "extracted": output_path.relative_to(ROOT).as_posix(),
            }
        )

    (output / "manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return manifest


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--php-src", help="php-src checkout; defaults to PHP_SRC_DIR or third_party/php-src")
    parser.add_argument("--out", default=str(DEFAULT_OUTPUT))
    parser.add_argument("--max-files", type=int, default=DEFAULT_MAX_FILES)
    parser.add_argument("--json", action="store_true", help="emit manifest JSON")
    args = parser.parse_args()

    php_src = find_php_src_dir(args.php_src)
    if php_src is None:
        print("[skip] no php-src checkout found; set PHP_SRC_DIR or --php-src")
        return 0

    manifest = extract_corpus(php_src, Path(args.out), max_files=max(args.max_files, 0))
    if args.json:
        print(json.dumps(manifest, indent=2, sort_keys=True))
    else:
        print(f"[info] php-src: {php_src}")
        print(f"[info] extracted {len(manifest)} parser corpus file(s) to {args.out}")
        print(f"[info] manifest: {Path(args.out) / 'manifest.json'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
