#!/usr/bin/env bash
set -euo pipefail

required_files=(
  "docs/phpt/README.md"
  "docs/phpt/source-integrity.md"
  "docs/phpt/source-lookup.md"
  "docs/phpt/binary-discovery.md"
  "docs/phpt/official-runner.md"
  "docs/phpt/extension-policy.md"
  "docs/phpt/known-gaps.md"
  "docs/phpt/full-phpt-gate.md"
)

required_dirs=(
  "docs/phpt/modules"
  "docs/phpt/php-src-behavior"
  "docs/phpt/reports"
  "tests/phpt/generated"
  "tests/phpt/manifests"
)

for path in "${required_files[@]}"; do
  if [[ ! -s "$path" ]]; then
    printf 'PHPT foundation missing required file: %s\n' "$path" >&2
    exit 1
  fi
done

for path in "${required_dirs[@]}"; do
  if [[ ! -d "$path" ]]; then
    printf 'PHPT foundation missing required directory: %s\n' "$path" >&2
    exit 1
  fi
done

require_text() {
  local needle="$1"
  local path="$2"
  if ! grep -q "$needle" "$path"; then
    printf 'PHPT foundation missing required text in %s: %s\n' "$path" "$needle" >&2
    exit 1
  fi
}

require_text 'Module green' docs/phpt/README.md
require_text 'Full-run no-regression' docs/phpt/README.md
require_text 'Final strict green' docs/phpt/README.md
require_text 'read-only input' docs/phpt/source-integrity.md
require_text 'navigation aid' docs/phpt/source-lookup.md
require_text 'phrust-php' docs/phpt/binary-discovery.md
require_text 'official `run-tests.php` wrapper' docs/phpt/official-runner.md
require_text 'Extension PHPTs remain in the corpus' docs/phpt/extension-policy.md
require_text 'PHPT Known Gaps' docs/phpt/known-gaps.md
require_text 'complete discovered PHPT corpus' docs/phpt/full-phpt-gate.md

if [[ -f tests/phpt/manifests/php-src-hashes.jsonl && ! -s tests/phpt/manifests/php-src-hashes.jsonl ]]; then
  printf '%s\n' 'PHPT source hash manifest exists but is empty.' >&2
  exit 1
fi

if [[ -f tests/phpt/manifests/php-src-symbols.jsonl && ! -s tests/phpt/manifests/php-src-symbols.jsonl ]]; then
  printf '%s\n' 'PHPT source symbol manifest exists but is empty.' >&2
  exit 1
fi

if [[ -f tests/phpt/manifests/phpt-corpus.jsonl && ! -s tests/phpt/manifests/phpt-corpus.jsonl ]]; then
  printf '%s\n' 'PHPT corpus manifest exists but is empty.' >&2
  exit 1
fi

if [[ ! -s tests/phpt/manifests/full-baseline-module-counts.jsonl ]]; then
  printf '%s\n' 'PHPT baseline module-count manifest is missing or empty.' >&2
  exit 1
fi

if [[ ! -s tests/phpt/manifests/known-gap-catalog.jsonl ]]; then
  printf '%s\n' 'PHPT known-gap catalog is missing or empty.' >&2
  exit 1
fi

if [[ -f docs/phpt/reports/phpt-corpus-summary.md && ! -s docs/phpt/reports/phpt-corpus-summary.md ]]; then
  printf '%s\n' 'PHPT corpus summary exists but is empty.' >&2
  exit 1
fi

printf '%s\n' '[ok] PHPT foundation docs and directories are present.'
