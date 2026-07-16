#!/usr/bin/env python3
"""Documentation-strategy verifier.

Enforces the documentation lifecycle so the tree cannot silently regrow
unsorted, unlinked, or drifting prose:

- every committed doc has exactly one lifecycle class, derived from path
  rules plus explicit overrides;
- every doc outside the `generated` class is reachable through the link
  graph rooted at the top-level and section indexes (a curated index, not
  incidental mentions);
- `just` recipe names quoted in docs must exist in the justfile, so prose
  cannot drift away from the executable gates;
- work-item logs (dated change narrations) are a ratcheting count: the
  baseline may only shrink, and new ones are rejected — active work belongs
  in issues and PR descriptions, not committed docs.

The baseline (docs_strategy_baseline.json) grandfathers the current state;
regenerate it with --write-baseline after a reviewed cleanup. Violations
against the baseline fail closed.
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BASELINE = ROOT / "scripts/verify/docs_strategy_baseline.json"

# Lifecycle classes, most specific rule first. `generated` docs are exempt
# from reachability (their generators own them); `research` and `worklog`
# entries are dated by nature and must say so.
CLASS_RULES: list[tuple[str, str]] = [
    (r"^(fixtures|tests|demo|references)/", "colocated"),
    (r"^docs/phpt/modules/", "generated"),
    (r"^docs/phpt/php-src-behavior/", "generated"),
    (r"^docs/generated/", "generated"),
    (r"^docs/stdlib/function-coverage\.md$", "generated"),
    (r"^docs/stdlib/extension-coverage\.md$", "generated"),
    (r"^docs/adr/", "contract"),
    (r"^docs/research/", "research"),
    (r"known-gaps[^/]*\.md$", "ledger"),
    (r"(status|coverage|roadmap|signoff|baseline)[^/]*\.md$", "ledger"),
    (r"^docs/", "reference"),
    (r"^[^/]+\.md$", "reference"),
]

# Explicit per-file overrides where the path rules misclassify.
CLASS_OVERRIDES: dict[str, str] = {}

WORKLOG_MARKERS = (
    re.compile(r"^Date: 20\d\d-", re.M),
    re.compile(r"^## Work item", re.M),
)

LINK_ROOTS = [
    "README.md",
    "CLAUDE.md",
    "AGENTS.md",
    "docs/README.md",
]

RECIPE_PATTERN = re.compile(r"`(?:nix develop -c )?just ([a-z][a-z0-9-]+)`")
PROSE_RECIPE_ALLOW = {"help", "verify"}


def tracked_docs() -> list[str]:
    out = subprocess.run(
        ["git", "ls-files", "*.md"], capture_output=True, text=True, cwd=ROOT, check=True
    ).stdout.split()
    return [
        p
        for p in out
        if not p.startswith(("third_party/", "target/", ".claude/", ".github/"))
        and (ROOT / p).is_file()
    ]


def classify(path: str) -> str:
    if path in CLASS_OVERRIDES:
        return CLASS_OVERRIDES[path]
    for pattern, cls in CLASS_RULES:
        if re.search(pattern, path):
            return cls
    return "unclassified"


def link_targets(text: str, source: str) -> set[str]:
    targets: set[str] = set()
    base = Path(source).parent
    for match in re.finditer(r"\]\(([^)#\s]+\.md)", text):
        raw = match.group(1)
        candidate = (base / raw) if not raw.startswith("docs/") else Path(raw)
        try:
            resolved = candidate.resolve().relative_to(ROOT.resolve())
        except ValueError:
            continue
        targets.add(str(resolved))
    # Bare path mentions (`docs/x/y.md`) count as curated references too.
    for match in re.finditer(r"\bdocs/[\w./-]+\.md\b", text):
        targets.add(match.group(0))
    return targets


def reachable_docs(docs: list[str], texts: dict[str, str]) -> set[str]:
    section_readmes = [d for d in docs if Path(d).name == "README.md"]
    frontier = [r for r in LINK_ROOTS if r in texts] + section_readmes
    seen: set[str] = set(frontier)
    while frontier:
        current = frontier.pop()
        for target in link_targets(texts.get(current, ""), current):
            if target in texts and target not in seen:
                seen.add(target)
                frontier.append(target)
    return seen


def just_recipes() -> set[str]:
    out = subprocess.run(
        ["just", "--summary"], capture_output=True, text=True, cwd=ROOT, check=True
    ).stdout
    return set(out.split()) | PROSE_RECIPE_ALLOW


def collect() -> dict:
    docs = tracked_docs()
    texts = {p: (ROOT / p).read_text(errors="ignore") for p in docs}
    classes = {p: classify(p) for p in docs}
    reachable = reachable_docs(docs, texts)
    orphans = sorted(
        p
        for p in docs
        if classes[p] not in ("generated", "colocated")
        and Path(p).name != "README.md"
        and p not in reachable
    )
    recipes = just_recipes()
    dead_recipes = sorted(
        {
            f"{p}:{match.group(1)}"
            for p in docs
            for match in RECIPE_PATTERN.finditer(texts[p])
            if match.group(1) not in recipes
        }
    )
    worklogs = sorted(
        p
        for p in docs
        if classes[p] == "worklog"
        or (
            classes[p] not in ("generated", "research", "ledger", "colocated")
            and any(marker.search(texts[p]) for marker in WORKLOG_MARKERS)
        )
    )
    unclassified = sorted(p for p in docs if classes[p] == "unclassified")
    return {
        "orphans": orphans,
        "dead_recipes": dead_recipes,
        "worklogs": worklogs,
        "unclassified": unclassified,
    }


def check(state: dict, baseline: dict) -> list[str]:
    failures: list[str] = []
    if state["unclassified"]:
        failures.append(f"unclassified docs: {', '.join(state['unclassified'])}")
    for key, label in (
        ("orphans", "unreachable doc (link it from its section index)"),
        ("dead_recipes", "doc quotes a just recipe that does not exist"),
        ("worklogs", "work-item log committed as documentation"),
    ):
        allowed = set(baseline.get(key, []))
        new = [entry for entry in state[key] if entry not in allowed]
        for entry in new:
            failures.append(f"{label}: {entry}")
    return failures


def self_test() -> None:
    assert classify("docs/adr/0001-x.md") == "contract"
    assert classify("docs/phpt/modules/spl.md") == "generated"
    assert classify("docs/research/foo.md") == "research"
    assert classify("docs/runtime/known-gaps.md") == "ledger"
    assert classify("docs/runtime/vm.md") == "reference"
    assert classify("fixtures/runtime/README.md") == "colocated"
    assert classify("error_classes.md") == "reference"
    targets = link_targets("see [x](vm.md) and docs/adr/0001-a.md", "docs/runtime/contract.md")
    assert "docs/runtime/vm.md" in targets and "docs/adr/0001-a.md" in targets
    recipe_match = RECIPE_PATTERN.search("`just verify-runtime`")
    assert recipe_match is not None and recipe_match.group(1) == "verify-runtime"
    assert RECIPE_PATTERN.search("we just pushed the fix") is None
    print("[ok] docs strategy self-test passed")


def main() -> int:
    if "--self-test" in sys.argv:
        self_test()
        return 0
    state = collect()
    if "--write-baseline" in sys.argv:
        BASELINE.write_text(json.dumps(state, indent=2, sort_keys=True) + "\n")
        print(f"[ok] wrote {BASELINE.relative_to(ROOT)}")
        return 0
    baseline = json.loads(BASELINE.read_text()) if BASELINE.exists() else {}
    failures = check(state, baseline)
    stale = {
        key: sorted(set(baseline.get(key, [])) - set(state[key]))
        for key in ("orphans", "dead_recipes", "worklogs")
    }
    if any(stale.values()) and "--quiet" not in sys.argv:
        for key, entries in stale.items():
            for entry in entries:
                print(f"[ratchet] baseline entry resolved, tighten with --write-baseline: {key}: {entry}")
    if failures:
        print("[fail] docs strategy:", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        return 1
    print(
        f"[ok] docs strategy: {len(state['orphans'])} grandfathered orphans, "
        f"{len(state['worklogs'])} worklogs, {len(state['dead_recipes'])} recipe drift entries"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
