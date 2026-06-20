#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"

require_file() {
  local path="$1"
  if [[ ! -e "${path}" ]]; then
    printf '[fail] missing required file: %s\n' "${path}" >&2
    exit 1
  fi
  printf '[ok] file exists: %s\n' "${path}"
}

find_reference_php() {
  if [[ -n "${REFERENCE_PHP:-}" ]]; then
    printf '%s\n' "${REFERENCE_PHP}"
    return 0
  fi
  if [[ -x third_party/php-src/sapi/cli/php ]]; then
    printf '%s\n' "third_party/php-src/sapi/cli/php"
    return 0
  fi
  if command -v php >/dev/null 2>&1; then
    command -v php
    return 0
  fi
  return 1
}

if [[ -x scripts/verify-phase0.sh ]]; then
  scripts/verify-phase0.sh
fi

required_files=(
  crates/php_source/src/span.rs
  crates/php_source/src/line_index.rs
  crates/php_lexer/Cargo.toml
  crates/php_lexer/src/lib.rs
  crates/php_lexer/src/token.rs
  crates/php_lexer/src/lexer.rs
  crates/php_lexer/src/cursor.rs
  crates/php_lexer/src/modes.rs
  crates/php_lexer/src/diagnostics.rs
  crates/php_lexer_cli/Cargo.toml
  crates/php_lexer_cli/src/main.rs
  crates/php_testkit/src/lexer_reference.rs
  scripts/dump-reference-tokens.php
  scripts/tokenize-reference.php
  scripts/compare-lexer-fixtures.py
  scripts/lexer-corpus-smoke.py
  tests/fixtures/lexer/000-inline-html.php
  tests/fixtures/lexer/010-tags.php
  tests/fixtures/lexer/020-comments-whitespace.php
  docs/phase-1/phase-1-definition-of-done.md
  docs/phase-1/lexer-architecture.md
  docs/phase-1/token-model.md
  docs/phase-1/token-coverage.md
  docs/phase-1/fixture-catalog.md
  docs/phase-1/diagnostics-policy.md
  docs/phase-1/known-lexer-differences.md
)

for path in "${required_files[@]}"; do
  require_file "${path}"
done

cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p php_lexer lexer_invariants

if php_bin="$(find_reference_php)"; then
  printf '[info] running lexer fixture reference harness with %s\n' "${php_bin}"
  REFERENCE_PHP="${php_bin}" scripts/compare-lexer-fixtures.py
else
  printf '[skip] no PHP binary found; lexer fixture reference harness skipped\n'
fi

printf '[pass] phase1 verification complete\n'
