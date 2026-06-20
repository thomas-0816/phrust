set shell := ["bash", "-euo", "pipefail", "-c"]

help:
    @printf '%s\n' \
      'Available commands:' \
      '  just help           Show this help' \
      '  just fmt            Check Rust formatting' \
      '  just lint           Run Rust linting' \
      '  just test           Run Rust tests' \
      '  just test-lexer     Run lexer crate tests' \
      '  just check          Run local non-recursive checks' \
      '  just verify-phase0  Run Phase 0 verification' \
      '  just verify-phase1  Run Phase 1 verification' \
      '  just bootstrap-ref  Clone/pin the PHP reference checkout' \
      '  just verify-ref     Verify PHP reference checkout against lockfile' \
      '  just dump-reference-tokens  Dump PHP T_* constants as JSON' \
      '  just tokenize-ref FILE  Tokenize FILE with reference PHP' \
      '  just lex FILE        Tokenize FILE with Rust lexer CLI' \
      '  just lexer-ref FILE  Alias for tokenize-ref' \
      '  just lexer-fixtures  Run lexer fixture diff' \
      '  just lexer-diff      Run strict lexer fixture diff' \
      '  just lexer-diff-report  Write lexer diff JSON report' \
      '  just fuzz-lexer-smoke  Run lightweight lexer invariant tests' \
      '  just bench-lexer     Run lexer throughput baseline' \
      '  just lexer-corpus-smoke  Smoke-test extracted php-src corpus' \
      '  just extract-ref-metadata  Extract deterministic PHP reference metadata' \
      '  just build-ref-php  Build optional minimal reference PHP CLI' \
      '  just ref-php-version  Show reference PHP CLI version when built' \
      '  just ref-tokenizer-check  Check token_get_all in reference PHP CLI'

fmt:
    cargo fmt --all --check

lint:
    cargo clippy --workspace --all-targets -- -D warnings

test:
    cargo test --workspace

test-lexer:
    cargo test -p php_lexer

check:
    @just fmt
    @just lint
    @just test

bootstrap-ref:
    scripts/bootstrap-php-reference.sh

verify-ref:
    scripts/verify-php-reference.sh

dump-reference-tokens:
    @php_bin="${REFERENCE_PHP:-}"; \
    if [[ -z "$php_bin" ]]; then \
      if [[ -x third_party/php-src/sapi/cli/php ]]; then php_bin="third_party/php-src/sapi/cli/php"; \
      elif command -v php >/dev/null 2>&1; then php_bin="$(command -v php)"; \
      else printf '%s\n' 'No PHP binary found. Set REFERENCE_PHP or build third_party/php-src/sapi/cli/php.' >&2; exit 1; fi; \
    fi; \
    "$php_bin" scripts/dump-reference-tokens.php

tokenize-ref file:
    @php_bin="${REFERENCE_PHP:-}"; \
    if [[ -z "$php_bin" ]]; then \
      if [[ -x third_party/php-src/sapi/cli/php ]]; then php_bin="third_party/php-src/sapi/cli/php"; \
      elif command -v php >/dev/null 2>&1; then php_bin="$(command -v php)"; \
      else printf '%s\n' 'No PHP binary found. Set REFERENCE_PHP or build third_party/php-src/sapi/cli/php.' >&2; exit 1; fi; \
    fi; \
    "$php_bin" scripts/tokenize-reference.php --file "{{file}}"

lexer-ref file:
    @just tokenize-ref "{{file}}"

lex file:
    cargo run -p php_lexer_cli -- --file "{{file}}" --pretty

lexer-fixtures:
    scripts/compare-lexer-fixtures.py

lexer-diff:
    scripts/compare-lexer-fixtures.py

lexer-diff-report:
    scripts/compare-lexer-fixtures.py --all-diffs --json-report target/lexer-diff-report.json

fuzz-lexer-smoke:
    cargo test -p php_lexer lexer_invariants

bench-lexer:
    cargo bench -p php_lexer --bench lexer_throughput

lexer-corpus-smoke:
    scripts/lexer-corpus-smoke.py

extract-ref-metadata:
    scripts/extract-php-reference-metadata.py --php-src third_party/php-src --out references/php-src.metadata.json

build-ref-php:
    scripts/build-reference-php.sh

ref-php-version:
    @if [[ -x third_party/php-src/sapi/cli/php ]]; then \
      third_party/php-src/sapi/cli/php -v; \
    else \
      printf '%s\n' 'Reference PHP CLI is not built; run `nix develop -c just build-ref-php`.'; \
      exit 1; \
    fi

ref-tokenizer-check:
    @if [[ -x third_party/php-src/sapi/cli/php ]]; then \
      third_party/php-src/sapi/cli/php -r 'var_export(function_exists("token_get_all")); echo "\n";'; \
    else \
      printf '%s\n' 'Reference PHP CLI is not built; run `nix develop -c just build-ref-php`.'; \
      exit 1; \
    fi

verify-phase0:
    scripts/verify-phase0.sh

verify-phase1:
    scripts/verify-phase1.sh
