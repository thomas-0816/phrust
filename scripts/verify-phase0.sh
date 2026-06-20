#!/usr/bin/env bash
set -euo pipefail

required_files=(
  "README.md"
  "AGENTS.md"
  "flake.nix"
  "flake.lock"
  "Cargo.toml"
  "justfile"
  "docs/phase-0/compatibility-target.md"
  "docs/phase-0/syntax-sources.md"
  "docs/phase-0/runtime-semantics-map.md"
  "docs/phase-0/test-matrix.md"
  "docs/phase-0/license-and-copying-policy.md"
  "docs/phase-0/risk-register.md"
  "docs/phase-0/phase-0-definition-of-done.md"
  "docs/phase-0/final-audit.md"
  "docs/adr/0001-target-php-version.md"
  "docs/adr/0002-nix-dev-environment.md"
  "docs/adr/0003-reference-oracle.md"
  "docs/adr/0004-no-vendored-php-src.md"
  "docs/adr/0005-phase-boundaries.md"
  "references/README.md"
  "references/php-src.lock.example.toml"
  "tests/README.md"
  "tests/fixtures/README.md"
  "tests/fixtures/lexer/.gitkeep"
  "tests/fixtures/parser/.gitkeep"
  "tests/fixtures/runtime/.gitkeep"
  "tests/fixtures/phpt/.gitkeep"
)

required_content=(
  "8.5.7"
  "php-8.5.7"
  "zend_language_scanner.l"
  "zend_language_parser.y"
  "nix develop"
  "token_get_all"
  ".phpt"
  "php-src"
  "third_party"
  "license"
)

required_scripts=(
  "scripts/bootstrap-php-reference.sh"
  "scripts/verify-php-reference.sh"
)

optional_scripts=(
  "scripts/build-reference-php.sh"
  "scripts/extract-php-reference-metadata.py"
)

critical_reference_files=(
  "Zend/zend_language_scanner.l"
  "Zend/zend_language_parser.y"
  "Zend/zend_vm_def.h"
  "Zend/zend_ast.h"
  "Zend/zend_compile.h"
  "Zend/zend_types.h"
)

ok() {
  printf '[ok] %s\n' "$*"
}

warn() {
  printf '[warn] %s\n' "$*"
}

fail() {
  printf '[fail] %s\n' "$*" >&2
  exit 1
}

for file in "${required_files[@]}"; do
  [[ -f "$file" ]] || fail "missing required file: $file"
  ok "file exists: $file"
done

for needle in "${required_content[@]}"; do
  if rg --glob '!third_party/**' --glob '!target/**' --fixed-strings --quiet -- "$needle" .; then
    ok "required content found: $needle"
  else
    fail "required content not found: $needle"
  fi
done

for script in "${required_scripts[@]}"; do
  [[ -x "$script" ]] || fail "script is not executable: $script"
  ok "script executable: $script"
done

for script in "${optional_scripts[@]}"; do
  if [[ -e "$script" ]]; then
    [[ -x "$script" ]] || fail "optional script exists but is not executable: $script"
    ok "optional script executable: $script"
  fi
done

if [[ -f Cargo.toml ]]; then
  cargo fmt --all --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  ok "Rust workspace checks passed"
fi

if [[ -f references/php-src.lock.toml ]]; then
  scripts/verify-php-reference.sh
  ok "PHP reference lockfile verified"
else
  warn "optional reference not bootstrapped: references/php-src.lock.toml"
fi

if [[ -d third_party/php-src ]]; then
  for path in "${critical_reference_files[@]}"; do
    [[ -f "third_party/php-src/$path" ]] || fail "missing critical reference file: $path"
    ok "reference file exists: $path"
  done
else
  warn "optional reference checkout not present: third_party/php-src"
fi

printf '[pass] phase0 verification complete\n'
