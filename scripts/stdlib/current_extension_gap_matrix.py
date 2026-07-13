#!/usr/bin/env python3
"""Generate the current extension parity gap matrix from source inputs."""

from __future__ import annotations

import json
import subprocess
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "docs/generated/current-extension-gap-matrix.md"

REGISTRY_INPUTS = [
    "crates/php_runtime/Cargo.toml",
    "crates/php_runtime/build.rs",
    "crates/php_runtime/src/builtins/registry.rs",
    "crates/php_runtime/src/builtins/modules",
    "crates/php_vm/src/vm/builtin_classes.rs",
    "fixtures/stdlib/extensions",
    "crates/php_std/src/generated/extensions",
    "tests/phpt/manifests/modules",
    "tests/phpt/generated",
]


PROMPT_ORDER = [
    "fileinfo",
    "hash",
    "zlib",
    "mbstring",
    "iconv",
    "curl",
    "zip",
    "pdo",
    "pdo_sqlite",
    "pdo_mysql",
    "pdo_pgsql",
    "mysqli",
    "pgsql",
    "sqlite3",
    "xml",
    "dom",
    "simplexml",
    "xmlreader",
    "xmlwriter",
    "intl",
    "openssl",
    "sodium",
    "gd",
    "imagick",
    "exif",
    "json",
    "pcre",
    "apcu",
    "redis",
    "memcached",
    "igbinary",
    "msgpack",
    "ftp",
    "ldap",
    "imap",
    "ssh2",
    "soap",
    "sockets",
    "posix",
    "pcntl",
    "shmop",
    "sysvmsg",
    "sysvsem",
    "sysvshm",
    "readline",
    "spl",
    "reflection",
    "opcache",
    "phar",
    "xsl",
    "standard",
    "core",
]


@dataclass(frozen=True)
class ExtensionHint:
    backend: str
    behavior: str
    missing: str
    next_prompt: str


HINTS = {
    "fileinfo": ExtensionHint(
        "libmagic via build.rs/pkg-config FFI",
        "library-backed-but-thin",
        "Flag combinations, finfo object edge cases, and broader libmagic PHPT promotion",
        "FILEINFO-1",
    ),
    "hash": ExtensionHint(
        "RustCrypto/hash crates plus patched tiger crate",
        "library-backed-but-thin",
        "Algorithm/context parity, HMAC edge behavior, streaming file/hash contexts",
        "HASH-1",
    ),
    "zlib": ExtensionHint(
        "flate2",
        "library-backed-but-thin",
        "Streaming inflate/deflate contexts, gzip file handles, and warning parity",
        "ZLIB-1",
    ),
    "mbstring": ExtensionHint(
        "encoding_rs plus custom UTF-8 helpers",
        "library-backed-but-thin",
        "Alias map breadth, stateful mb_* settings, regex-free string edge cases",
        "MBSTRING-1",
    ),
    "iconv": ExtensionHint(
        "encoding_rs plus custom MIME helpers",
        "library-backed-but-thin",
        "Charset aliases, transliteration/ignore options, MIME folding parity",
        "ICONV-1",
    ),
    "curl": ExtensionHint(
        "curl crate for version metadata; execution still uses custom transport",
        "custom-subset",
        "Move curl_exec and getinfo/error data to libcurl Easy2",
        "CURL-1",
    ),
    "zip": ExtensionHint(
        "zip crate",
        "library-backed-but-thin",
        "ZipArchive write/update/delete/comment/stat coverage and libzip-like errors",
        "ZIP-1",
    ),
    "pdo": ExtensionHint(
        "custom VM PDO facade with rusqlite/mysql/postgres-adjacent support",
        "custom-subset",
        "PDO core object model, attributes, exceptions, statement lifecycle",
        "PDO-1",
    ),
    "pdo_sqlite": ExtensionHint(
        "rusqlite through VM PDO path",
        "library-backed-but-thin",
        "SQLite statement binding/fetch modes/transactions/error modes",
        "PDO-SQLITE-1",
    ),
    "pdo_mysql": ExtensionHint(
        "mysql crate where live DSN support is present",
        "library-backed-but-thin",
        "Real PDO MySQL driver behavior beyond platform/live smoke coverage",
        "PDO-MYSQL-1",
    ),
    "pdo_pgsql": ExtensionHint(
        "postgres crate where live DSN support is present",
        "library-backed-but-thin",
        "Real PDO PgSQL driver behavior beyond platform/live smoke coverage",
        "PDO-PGSQL-1",
    ),
    "mysqli": ExtensionHint(
        "mysql crate plus sqlite-compatible fallback paths",
        "library-backed-but-thin",
        "mysqlnd-compatible result, prepared statement, and error semantics",
        "MYSQLI-1",
    ),
    "pgsql": ExtensionHint(
        "postgres crate",
        "library-backed-but-thin",
        "Connection/resource lifecycle, query/result APIs, and live PHPT expansion",
        "PGSQL-1",
    ),
    "sqlite3": ExtensionHint(
        "rusqlite",
        "library-backed-but-thin",
        "SQLite3 class completeness, statement/result APIs, busy/error behavior",
        "SQLITE3-1",
    ),
    "xml": ExtensionHint(
        "custom bounded XML parser/tree",
        "custom-subset",
        "libxml-compatible SAX errors, encodings, namespaces, parser options",
        "XML-1",
    ),
    "dom": ExtensionHint(
        "custom VM DOM classes over bounded XML data",
        "custom-subset",
        "DOMDocument mutation/import/xpath/schema behavior and libxml errors",
        "DOM-1",
    ),
    "simplexml": ExtensionHint(
        "custom SimpleXML object model",
        "custom-subset",
        "Namespace/xpath/array-cast/reference-cell behavior",
        "SIMPLEXML-1",
    ),
    "xmlreader": ExtensionHint(
        "custom bounded XML reader facade",
        "custom-subset",
        "Streaming reader state, attributes, namespaces, and error semantics",
        "XMLREADER-1",
    ),
    "xmlwriter": ExtensionHint(
        "custom XML writer facade",
        "custom-subset",
        "Full writer API, memory/document modes, invalid name and encoding errors",
        "XMLWRITER-1",
    ),
    "intl": ExtensionHint(
        "manual subset; no ICU backend detected",
        "custom-subset",
        "ICU-backed Normalizer, Collator, transliteration, locale data",
        "INTL-1",
    ),
    "openssl": ExtensionHint(
        "openssl crate",
        "library-backed-but-thin",
        "Cipher/method breadth, certificate/key/resource APIs, warning parity",
        "OPENSSL-1",
    ),
    "sodium": ExtensionHint(
        "pure Rust crypto crates; no libsodium backend detected",
        "custom-subset",
        "libsodium-compatible primitives, key validation, and constant parity",
        "SODIUM-1",
    ),
    "gd": ExtensionHint(
        "image crate",
        "library-backed-but-thin",
        "Image resource model, drawing/text/color APIs, codec/error parity",
        "GD-1",
    ),
    "imagick": ExtensionHint(
        "no ImageMagick/MagickWand backend detected",
        "stub/fake-success-risk",
        "Replace class metadata/backend gate with real MagickWand-backed behavior",
        "IMAGICK-1",
    ),
    "exif": ExtensionHint(
        "custom JPEG/EXIF parser",
        "custom-subset",
        "TIFF/IFD breadth, malformed data warnings, image-type helpers",
        "EXIF-1",
    ),
    "json": ExtensionHint(
        "serde_json",
        "library-backed-and-broad",
        "Remaining numeric/string flag edge cases and error-message parity",
        "JSON-1",
    ),
    "pcre": ExtensionHint(
        "patched pcre2 crate",
        "library-backed-but-thin",
        "PCRE option matrix, callbacks, error offsets, and delimiter edge cases",
        "PCRE-1",
    ),
    "apcu": ExtensionHint(
        "request-local custom cache state",
        "custom-subset",
        "TTL/SMA/cache-info semantics and request/persistent lifecycle parity",
        "APCU-1",
    ),
    "redis": ExtensionHint(
        "deterministic in-memory VM fake; no Redis protocol backend",
        "stub/fake-success-risk",
        "Real phpredis client semantics or explicit fake/backend boundary",
        "REDIS-1",
    ),
    "memcached": ExtensionHint(
        "deterministic in-memory VM fake; no libmemcached backend",
        "stub/fake-success-risk",
        "Real Memcached protocol/options/result-code behavior",
        "MEMCACHED-1",
    ),
    "igbinary": ExtensionHint(
        "custom serializer-compatible subset",
        "custom-subset",
        "Binary format parity, object/reference behavior, session serializer hooks",
        "IGBINARY-1",
    ),
    "msgpack": ExtensionHint(
        "custom serializer-compatible subset",
        "custom-subset",
        "MessagePack binary compatibility, options, and object/reference behavior",
        "MSGPACK-1",
    ),
    "ftp": ExtensionHint(
        "suppaftp backend behind request-local FTP state",
        "library-backed-but-thin",
        "FTPS, broader transfer/listing modes, passive mode edge cases, and FTP error parity",
        "FTP-1",
    ),
    "ldap": ExtensionHint(
        "ldap3 sync backend behind request-local LDAP state",
        "library-backed-but-thin",
        "Modify/TLS controls, result traversal breadth, option parity, and LDAP error stacks",
        "LDAP-1",
    ),
    "imap": ExtensionHint(
        "imap crate with native-tls connection backend",
        "library-backed-but-thin",
        "MIME/message structure parsing, fetch/search breadth, mailbox flags, and error stack parity",
        "IMAP-1",
    ),
    "ssh2": ExtensionHint(
        "ssh2 crate/libssh2 backend behind request-local SSH2 state",
        "library-backed-but-thin",
        "Shell/tunnel/publickey behavior, stream metadata, and broader SFTP operation parity",
        "SSH2-1",
    ),
    "soap": ExtensionHint(
        "custom SOAP value/class facade",
        "custom-subset",
        "WSDL/client/server XML serialization and transport behavior",
        "SOAP-1",
    ),
    "sockets": ExtensionHint(
        "libc/std socket wrappers",
        "library-backed-but-thin",
        "Socket options, address families, errors, select/sendmsg coverage",
        "SOCKETS-1",
    ),
    "posix": ExtensionHint(
        "nix/libc",
        "library-backed-but-thin",
        "User/group/process APIs, errno parity, platform-specific skips",
        "POSIX-1",
    ),
    "pcntl": ExtensionHint(
        "libc process-signal wrappers",
        "library-backed-but-thin",
        "Fork/wait/signal/alarm semantics and platform gate parity",
        "PCNTL-1",
    ),
    "shmop": ExtensionHint(
        "custom/platform shared-memory facade",
        "custom-subset",
        "Real System V shared memory semantics and permissions",
        "SHMOP-1",
    ),
    "sysvmsg": ExtensionHint(
        "custom/platform System V facade",
        "custom-subset",
        "Real queue send/receive/stat/remove semantics",
        "SYSVMSG-1",
    ),
    "sysvsem": ExtensionHint(
        "custom/platform System V facade",
        "custom-subset",
        "Semaphore acquire/release/remove/undo semantics",
        "SYSVSEM-1",
    ),
    "sysvshm": ExtensionHint(
        "custom/platform System V facade",
        "custom-subset",
        "Shared memory attach/put/get/remove behavior",
        "SYSVSHM-1",
    ),
    "readline": ExtensionHint(
        "noninteractive custom facade",
        "metadata-only",
        "Interactive readline/history/completion behavior",
        "READLINE-1",
    ),
    "spl": ExtensionHint(
        "custom VM/runtime SPL classes",
        "custom-subset",
        "Iterator/file/autoload/data-structure completeness",
        "SPL-1",
    ),
    "reflection": ExtensionHint(
        "custom VM reflection metadata",
        "custom-subset",
        "Complete reflection metadata, attributes, types, internal signatures",
        "REFLECTION-1",
    ),
    "opcache": ExtensionHint(
        "custom status/config facade",
        "metadata-only",
        "Real opcache semantics are out of runtime scope; keep facade honest",
        "OPCACHE-1",
    ),
    "phar": ExtensionHint(
        "custom read-only facade",
        "metadata-only",
        "Archive metadata, stream wrappers, signatures, write policy",
        "PHAR-1",
    ),
    "xsl": ExtensionHint(
        "no libxslt backend detected",
        "stub/fake-success-risk",
        "libxslt-backed XSLTProcessor behavior",
        "XSL-1",
    ),
    "standard": ExtensionHint(
        "custom runtime modules plus selected Rust crates",
        "custom-subset",
        "Array/string/filesystem/serialization edge parity across upstream PHPTs",
        "STANDARD-1",
    ),
    "core": ExtensionHint(
        "VM/runtime core semantics",
        "custom-subset",
        "Zend language/runtime edge semantics and diagnostics",
        "CORE-1",
    ),
}


def run_registry_dump() -> dict:
    result = subprocess.run(
        ["cargo", "run", "-q", "-p", "php_std", "--bin", "dump_stdlib_registry"],
        cwd=ROOT,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    start = result.stdout.find("{")
    if start < 0:
        raise RuntimeError("dump_stdlib_registry did not emit JSON")
    return json.loads(result.stdout[start:])


def count_lines(path: Path) -> int:
    if not path.exists():
        return 0
    with path.open("r", encoding="utf-8") as handle:
        return sum(1 for line in handle if line.strip())


def count_generated_phpts(extension: str) -> int:
    generated_root = ROOT / "tests/phpt/generated"
    total = 0
    for root in generated_root.iterdir() if generated_root.exists() else []:
        if not root.is_dir() or not phpt_module_belongs_to_extension(root.name, extension):
            continue
        total += len(sorted(root.glob("*.phpt")))
    return total


def module_file_exists(extension: str) -> bool:
    return (ROOT / "crates/php_runtime/src/builtins/modules" / f"{extension}.rs").exists()


def registry_by_name(registry: dict) -> dict[str, dict]:
    return {entry["name"]: entry for entry in registry["extensions"]}


def all_extension_names(registry: dict) -> list[str]:
    names = set(PROMPT_ORDER)
    names.update(entry["name"] for entry in registry["extensions"])
    return sorted(names, key=lambda name: (PROMPT_ORDER.index(name) if name in PROMPT_ORDER else 999, name))


def counts_for(extension: str, registry: dict[str, dict]) -> tuple[int, int, int, int]:
    entry = registry.get(extension)
    if entry is None:
        return 0, 0, 0, 0
    functions = entry["functions"]
    runtime_functions = [function for function in functions if function["runtime_builtin"]]
    return len(functions), len(entry["classes"]), len(entry["constants"]), len(runtime_functions)


def phpt_module_belongs_to_extension(module: str, extension: str) -> bool:
    if module == extension:
        return True
    if module.startswith(f"{extension}."):
        return extension in {"standard", "spl", "reflection", "pdo"}
    return False


def manifest_counts(extension: str) -> tuple[int, int]:
    manifests = ROOT / "tests/phpt/manifests/modules"
    manifest = 0
    selected = 0
    for path in manifests.glob("*.json"):
        if phpt_module_belongs_to_extension(path.stem, extension):
            manifest += count_lines(path)
    for path in manifests.glob("*.selected.jsonl"):
        module = path.name.removesuffix(".selected.jsonl")
        if phpt_module_belongs_to_extension(module, extension):
            selected += count_lines(path)
    return manifest, selected


def hint_for(extension: str, registry: dict[str, dict]) -> ExtensionHint:
    if extension in HINTS:
        return HINTS[extension]
    entry = registry.get(extension)
    if entry:
        runtime_count = sum(1 for function in entry["functions"] if function["runtime_builtin"])
        backend = "custom runtime/VM implementation or metadata"
        behavior = "custom-subset" if module_file_exists(extension) or runtime_count else "metadata-only"
        missing = "No prompt-pack-specific next step; keep PHPT promotion source-derived"
        return ExtensionHint(backend, behavior, missing, "Backlog")
    return ExtensionHint(
        "none detected",
        "missing",
        "Register only with real behavior or explicit PHP-like unsupported diagnostics",
        "Backlog",
    )


def markdown_escape(value: str) -> str:
    return value.replace("|", "\\|")


def render_report(registry: dict) -> str:
    registry_map = registry_by_name(registry)
    lines: list[str] = []
    lines.append("# Current Extension Gap Matrix")
    lines.append("")
    lines.append("Generated from current source inputs by `scripts/stdlib/current_extension_gap_matrix.py`.")
    lines.append("This is an audit artifact only; it does not change runtime behavior.")
    lines.append("")
    lines.append("## Source Inputs")
    lines.append("")
    for source in REGISTRY_INPUTS:
        path = ROOT / source
        status = "present" if path.exists() else "missing"
        lines.append(f"- `{source}` ({status})")
    lines.append("")
    lines.append("## Matrix")
    lines.append("")
    lines.append(
        "| Extension | Registered functions/classes/constants | Backend library currently used | Current behavior class | Highest-value missing behavior | Recommended next prompt |"
    )
    lines.append("| --- | --- | --- | --- | --- | --- |")
    for extension in all_extension_names(registry):
        functions, classes, constants, runtime_functions = counts_for(extension, registry_map)
        manifest_total, manifest_selected = manifest_counts(extension)
        generated = count_generated_phpts(extension)
        hint = hint_for(extension, registry_map)
        module_note = "runtime module" if module_file_exists(extension) else "no runtime module"
        counts = (
            f"functions={functions} ({runtime_functions} runtime), "
            f"classes={classes}, constants={constants}; {module_note}"
        )
        missing = (
            f"{hint.missing}. PHPT manifest/selected/generated: "
            f"{manifest_total}/{manifest_selected}/{generated}"
        )
        lines.append(
            "| "
            + " | ".join(
                [
                    f"`{markdown_escape(extension)}`",
                    markdown_escape(counts),
                    markdown_escape(hint.backend),
                    f"`{hint.behavior}`",
                    markdown_escape(missing),
                    markdown_escape(hint.next_prompt),
                ]
            )
            + " |"
        )
    lines.append("")
    lines.append("## Notes")
    lines.append("")
    lines.append(
        "- The registered symbol counts come from `php_std::ExtensionRegistry::standard_library()` via `dump_stdlib_registry`."
    )
    lines.append(
        "- `runtime` counts mean the dumped function has a matching runtime or VM builtin registration."
    )
    lines.append(
        "- Behavior classes and next prompts are conservative annotations from the current prompt pack plus source-level backend evidence."
    )
    lines.append(
        "- PHPT columns are folded into the missing-behavior text as `manifest/selected/generated` counts for the same module name."
    )
    lines.append("")
    return "\n".join(lines)


def main() -> None:
    registry = run_registry_dump()
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(render_report(registry), encoding="utf-8")


if __name__ == "__main__":
    main()
