set shell := ["bash", "-euo", "pipefail", "-c"]

help:
    @printf '%s\n' \
      'Available commands:' \
      '  just help                 Show this help' \
      '  just check                Format, lint, and run workspace tests' \
      '  just verify               Run the full local verification gate' \
      '  just verify-frontend      Lexer, parser, CST, AST, semantics, CLI snapshots' \
      '  just verify-runtime       IR, VM, runtime fixtures, hardening checks' \
      '  just verify-stdlib        Builtins, streams, JSON/PCRE/date, SPL/reflection' \
      '  just verify-server        Integrated HTTP server tests and smoke gate' \
      '  just verify-performance   Optimizer, cache, quickening, IC, JIT smoke gates' \
      '  just verify-phpt          PHPT tooling, manifests, and source-integrity checks' \
      '  just known-gaps           Validate checked known-gap manifests' \
      '  just source-integrity      Check module wiring and generated metadata' \
      '  just verify-generated-arginfo Strict php-src arginfo drift check' \
      '  just fmt                  Check Rust formatting' \
      '  just lint                 Run Rust linting' \
      '  just test                 Run Rust workspace tests' \
      '  just diagnostics-audit    Run diagnostic quality ratchet' \
      '  just diagnostics-smoke    Run structured diagnostic smoke checks' \
      '  just debug-smoke          Run debug-mode JSONL smoke checks' \
      '  just quality              Run additive Rust quality/tooling gates' \
      '  just quality-fast         Run required cheap integrity/dependency/docs gates' \
      '  just quality-deps         Check advisories, licenses, bans, sources' \
      '  just quality-unused-deps  Check for unused Cargo dependencies' \
      '  just quality-coverage     Run opt-in cargo-llvm-cov coverage' \
      '  just quality-mutants      Run opt-in cargo-mutants mutation testing' \
      '  just quality-fuzz         Run deterministic fuzz/property smokes' \
      '  just quality-docs         Treat rustdoc warnings and doctests as failures' \
      '  just quality-api          Check public API semver against a git baseline' \
      '  just quality-lints        Report pedantic/nursery Clippy findings' \
      '  just sonar-coverage       Generate LCOV coverage for SonarQube' \
      '  just sonar-scan           Run SonarQube scanner with Rust coverage' \
      '  just bootstrap-ref        Clone/pin the PHP reference checkout' \
      '  just verify-ref           Verify PHP reference checkout against lockfile' \
      '' \
      'Frontend and syntax:' \
      '  just lexer-fixtures       Run lexer fixture diff' \
      '  just parser-fixtures      Run parser fixture oracle harness' \
      '  just parser-diff          Compare parser acceptance with php -l' \
      '  just cst-roundtrip        Check exact CST reconstruction' \
      '  just semantic-fixtures    Run semantic fixture harness' \
      '  just semantic-diff        Compare semantic acceptance with PHP reference' \
      '  just frontend-snapshots   Run frontend CLI/API snapshot smoke tests' \
      '' \
      'Runtime and VM:' \
      '  just bytecode-snapshots   Run bytecode snapshot checks' \
      '  just bytecode-exec-smoke  Run dense bytecode execution A/B smoke' \
      '  just bytecode-layout-smoke Run dense bytecode block layout A/B smoke' \
      '  just superinstruction-smoke Run dense bytecode superinstruction A/B smoke' \
      '  just vm-smoke             Run VM CLI smoke checks' \
      '  just vm-trace-smoke       Run VM trace/debug smoke checks' \
      '  just runtime-fixtures     Run runtime fixture checks' \
      '  just runtime-diff         Compare runtime output with PHP reference when configured' \
      '  just runtime-known-gaps   Validate runtime known-gap catalog' \
      '  just runtime-gap-report   Regenerate runtime gap closure report' \
      '  just wp-language-vm       Run WordPress language/VM core fixtures' \
      '  just wordpress-preflight  Classify local real WordPress smoke prerequisites' \
      '  just wordpress-real-smoke Run no-DB real WordPress frontpage smoke' \
      '  just wordpress-real-install-smoke Run DB-backed real WordPress install smoke' \
      '  just mysqli-integration   Run explicit live MySQLi integration gate' \
      '  just wordpress-smoke-report Generate web/db diagnostics report' \
      '' \
      'Server:' \
      '  just verify-server        Run integrated web server verification' \
      '  just server-smoke         Run integrated web server smoke checks' \
      '  just server-compat-smoke [SECTION=all] Run Wave 2 server compatibility smoke checks' \
      '  just server-tls-smoke     Run integrated HTTPS server smoke checks' \
      '  just server-benchmark-smoke Run short optional server benchmark smoke' \
      '' \
      'Standard library and compatibility:' \
      '  just generate-arginfo     Regenerate stdlib arginfo from php-src stubs' \
      '  just verify-generated-arginfo Regenerate and diff committed arginfo' \
      '  just diff-stdlib          Run standard-library differential gate' \
      '  just diff-streams         Run streams differential gate' \
      '  just diff-json-pcre-date  Run JSON/PCRE/Date differential gate' \
      '  just diff-spl-reflection  Run SPL/Reflection differential gate' \
      '  just composer-smoke       Run package-manager compatibility smoke gate' \
      '' \
      'Performance:' \
      '  just perf-flag-matrix     Run performance flag A/B matrix' \
      '  just cache-roundtrip      Run bytecode-cache roundtrip gate' \
      '  just optimizer-diff       Run optimizer differential gate' \
      '  just superinstruction-patterns Mine dense opcode pairs/triples' \
      '  just quickening-smoke     Run quickening smoke gate' \
      '  just inline-cache-smoke   Run inline-cache smoke gate' \
      '  just jit-smoke            Run default-off JIT smoke gate' \
      '  just framework-smoke      Run offline framework-like performance smoke' \
      '  just app-flow-smoke      Run CI-safe app-flow engine comparison smoke' \
      '  just app-flow-matrix     Run full application-flow Phrust/reference matrix' \
      '  just release-benchmark-smoke Run production release performance smoke' \
      '  just pgo-benchmark-smoke  Run optional PGO performance smoke' \
      '  just bolt-benchmark-smoke Run optional Linux BOLT performance smoke' \
      '  just fastest-engine-matrix Generate baseline/release/reference comparison matrix' \
      '  just fastest-hotpath-report Generate fastest-engine hotpath report' \
      '  just perf-report          Generate performance report' \
      '' \
      'PHPT:' \
      '  just phpt-index           Index the PHPT corpus' \
      '  just phpt-source-index    Index pinned php-src source and PHPT hashes' \
      '  just phpt-runner-smoke    Run PHPT runner tests' \
      '  just phpt-reference-smoke Run Reference PHP smoke' \
      '  just phpt-target-smoke    Run Target PHP smoke' \
      '  just phpt-build           Build PHPT runner and target binaries' \
      '  just phpt-dev-build       Build PHPT binaries with local incremental mode' \
      '  just phpt-dev-shell       Open one nix shell after building PHPT dev binaries' \
      '  just phpt-generate-module MODULE=<module>  Generate module tests' \
      '  just phpt-module MODULE=<module>           Run a module batch' \
      '  just phpt-dev-module MODULE=<module>       Run module batch after dev build' \
      '  just phpt-module-target MODULE=<module>    Run target-only module batch' \
      '  just phpt-fast MODULE=<module> [FILE=<path>|PATTERN=<text>]  Run fast target-only module loop' \
      '  just phpt-dev-fast MODULE=<module> [...]   Run fast loop with explicit dev PASS reuse' \
      '  just phpt-rerun-failures MODULE=<module>   Rerun only last non-green module outcomes' \
      '  just phpt-triage         Generate PHPT triage report and module plan' \
      '  just phpt-full-regression Run the full PHPT no-regression gate with PHPT_RUN_FULL=1' \
      '  just phpt-full-fast      Run full PHPT gate after dev build with explicit local reuse' \
      '  just phpt-verify-baseline Verify committed PHPT full baseline files' \
      '  just phpt-verify-source-integrity Verify pinned php-src was not mutated' \
      '  just install-hooks       Install versioned git hooks' \
      '  just ci-local            Run the local GitHub Actions parity gate'

install-hooks:
    scripts/git/install-hooks.sh

fmt:
    cargo fmt --all --check

lint:
    cargo clippy --workspace --all-targets -- -D warnings

runtime-hardening-lints:
    cargo clippy -p php_runtime -p php_vm --all-targets -- -D warnings -D unsafe-code

test:
    RUST_MIN_STACK="${PHRUST_RUST_MIN_STACK:-8388608}" cargo test --workspace

diagnostics-audit:
    scripts/diagnostics/audit.py

server-smoke:
    scripts/server/smoke.sh

diagnostics-smoke:
    scripts/diagnostics/smoke.sh diagnostics

debug-smoke:
    scripts/diagnostics/smoke.sh debug

verify-server:
    cargo test -p php_executor -p php_server
    @just server-smoke

server-compat-smoke SECTION="all":
    scripts/server/compat_smoke.sh {{SECTION}}

server-tls-smoke:
    scripts/server/tls_smoke.sh

server-benchmark-smoke:
    scripts/server/benchmark_smoke.sh

test-lexer:
    cargo test -p php_lexer

check:
    @just source-integrity
    @just fmt
    @just lint
    @just test

source-integrity:
    scripts/verify/source_integrity.py

quality:
    @just quality-fast
    @just quality-coverage
    @just quality-mutants
    @just quality-fuzz
    @just quality-api
    @just quality-lints

quality-fast:
    @just source-integrity
    @just diagnostics-audit
    @just known-gaps
    @just quality-deps
    @just quality-unused-deps
    cargo check --workspace --all-targets --all-features
    @just quality-docs

quality-deps:
    @if ! command -v cargo-deny >/dev/null 2>&1; then \
      printf '%s\n' '[skip] cargo-deny unavailable; enter nix develop or install cargo-deny.'; \
      exit 0; \
    fi; \
    cargo deny check advisories bans licenses sources

quality-unused-deps:
    @if ! command -v cargo-machete >/dev/null 2>&1; then \
      printf '%s\n' '[skip] cargo-machete unavailable; enter nix develop or install cargo-machete.'; \
      exit 0; \
    fi; \
    cargo machete

quality-coverage:
    @if [[ "${PHRUST_RUN_COVERAGE:-0}" != "1" ]]; then \
      printf '%s\n' '[skip] set PHRUST_RUN_COVERAGE=1 to run cargo-llvm-cov coverage.'; \
      exit 0; \
    fi; \
    if ! command -v cargo-llvm-cov >/dev/null 2>&1; then \
      printf '%s\n' '[skip] cargo-llvm-cov unavailable; enter nix develop or install cargo-llvm-cov.'; \
      exit 0; \
    fi; \
    if cargo nextest --version >/dev/null 2>&1; then \
      cargo llvm-cov nextest --workspace --summary-only; \
    else \
      cargo llvm-cov --workspace --summary-only; \
    fi

sonar-coverage:
    mkdir -p target/sonar
    cargo llvm-cov --workspace --lcov --output-path target/sonar/lcov.info

sonar-scan *ARGS:
    scripts/sonar/scan.sh {{ARGS}}

quality-mutants:
    @if [[ "${PHRUST_RUN_MUTANTS:-0}" != "1" ]]; then \
      printf '%s\n' '[skip] set PHRUST_RUN_MUTANTS=1 to run cargo-mutants mutation testing.'; \
      exit 0; \
    fi; \
    if command -v cargo-mutants >/dev/null 2>&1; then \
      cargo-mutants --workspace; \
    elif cargo mutants --version >/dev/null 2>&1; then \
      cargo mutants --workspace; \
    else \
      printf '%s\n' '[skip] cargo-mutants unavailable; enter nix develop or install cargo-mutants.'; \
      exit 0; \
    fi

quality-fuzz:
    @just fuzz-lexer-smoke
    @just fuzz-parser-smoke
    @just runtime-fuzz-smoke
    @just fuzz-vm-smoke
    @if command -v cargo-fuzz >/dev/null 2>&1 || cargo fuzz --version >/dev/null 2>&1; then \
      printf '%s\n' '[ok] cargo-fuzz is available for coverage-guided fuzz target expansion.'; \
    else \
      printf '%s\n' '[skip] cargo-fuzz unavailable; deterministic fuzz/property smokes ran, but coverage-guided cargo-fuzz is not installed.'; \
    fi

quality-docs:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --lib --no-deps
    cargo test --doc --workspace

quality-api:
    @baseline="${PHRUST_SEMVER_BASELINE:-HEAD}"; \
    if command -v cargo-semver-checks >/dev/null 2>&1 || cargo semver-checks --version >/dev/null 2>&1; then \
      cargo semver-checks check-release --workspace --baseline-rev "$baseline"; \
    else \
      printf '%s\n' '[skip] cargo-semver-checks unavailable; enter nix develop or install cargo-semver-checks.'; \
      exit 0; \
    fi

quality-lints:
    cargo clippy --workspace --all-targets -- \
      -W clippy::pedantic \
      -W clippy::nursery \
      -A clippy::missing_errors_doc \
      -A clippy::missing_panics_doc \
      -A clippy::module_name_repetitions \
      -A clippy::must_use_candidate \
      -A clippy::too_many_lines

verify:
    @just check
    @just verify-frontend
    @just verify-runtime
    @just verify-stdlib
    @just verify-server
    @just verify-performance
    @just verify-phpt

ci-rust:
    @just fmt
    @just lint
    @just test

ci-domain-gates:
    @just verify-frontend
    @just verify-runtime
    @just verify-stdlib
    @just verify-server
    @just verify-performance

ci-phpt-smoke:
    scripts/phpt/ci_smoke.sh

ci-local:
    @just quality-fast
    @just ci-rust
    @just ci-domain-gates
    @just ci-phpt-smoke

verify-frontend:
    @just lexer-fixtures
    @just parser-fixtures
    @just cst-roundtrip
    @just semantic-fixtures
    @just semantic-diff
    @just frontend-snapshots

verify-runtime:
    @just bytecode-snapshots
    @just bytecode-exec-smoke
    @just vm-smoke
    @just vm-trace-smoke
    @just runtime-fixtures
    @just runtime-known-gaps
    @just runtime-semantics-fixtures
    @just runtime-semantics-diff
    @just vm-semantics-oracle
    @just runtime-hardening-lints

verify-stdlib:
    @just stdlib-docs
    @just stdlib-coverage
    @just diff-stdlib
    @just diff-streams
    @just diff-json-pcre-date
    @just diff-spl-reflection

verify-performance:
    @just performance-tests
    @just performance-regression
    @just perf-flag-matrix
    @just benchmark-smoke
    @just framework-smoke
    @just release-benchmark-smoke
    @just acceleration-matrix
    @just fastest-engine-matrix
    @just default-profile-smoke
    @just managed-fast-coverage
    @just fast-preset-smoke
    @just app-flow-smoke
    @just baseline-native-stencil-smoke
    @just copy-patch-stencil-smoke
    @just mid-tier-plan-smoke
    @just cache-roundtrip
    @just optimizer-diff
    @just bytecode-layout-smoke
    @just superinstruction-smoke
    @just templates-smoke
    @just quickening-smoke
    @just inline-cache-smoke
    @just callgrind-smoke
    @just jit-smoke
    @just safety-audit-smoke
    @just hotpath-inventory
    @just fastest-hotpath-report
    @just perf-report
    @printf '%s\n' '[pass] performance verification complete'

verify-phpt:
    @just known-gaps
    scripts/phpt/verify_foundation.sh
    @just phpt-verify-baseline
    scripts/phpt/verify_source_integrity.sh
    cargo test -p php_phpt_tools

verify-foundation:
    scripts/verify/foundation.sh

verify-lexer:
    scripts/verify/lexer.sh

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

parser-lint-oracle:
    scripts/run_parser_fixtures.py

parser-fixtures:
    scripts/run_parser_fixtures.py

parser-diff:
    scripts/compare_parser_acceptance.py --strict

cst-roundtrip:
    cargo test -p php_syntax --test fixture_roundtrip

extract-parser-corpus:
    scripts/extract_parser_corpus.py

parser-corpus-smoke:
    scripts/run_parser_corpus_smoke.py

fuzz-parser-smoke:
    cargo test -p php_syntax --test parser_properties
    PARSER_FUZZ_CASES=1024 cargo test -p php_syntax --test parser_properties -- --ignored

bench-parser:
    cargo test -p php_syntax --test perf_smoke -- --ignored --nocapture

parser-snapshots:
    UPDATE_PARSER_SNAPSHOTS=1 cargo test -p php_syntax --test parser_snapshots
    UPDATE_PARSER_SNAPSHOTS=1 cargo test -p php_syntax --test diagnostic_snapshots

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

phpt-index *args:
    cargo build -q -p php_phpt_tools --bin php-phpt-tools
    ${CARGO_TARGET_DIR:-target}/debug/php-phpt-tools phpt-index {{args}}

phpt-source-index *args:
    cargo build -q -p php_phpt_tools --bin php-phpt-tools
    ${CARGO_TARGET_DIR:-target}/debug/php-phpt-tools source-index {{args}}
    ${CARGO_TARGET_DIR:-target}/debug/php-phpt-tools symbol-index {{args}}

phpt-runner-smoke *args:
    scripts/phpt/runner_smoke.sh

phpt-reference-smoke *args:
    scripts/phpt/binary_smoke.sh reference

phpt-target-smoke *args:
    scripts/phpt/binary_smoke.sh target

phpt-build:
    cargo build -q -p php_phpt_tools --bin php-phpt-tools -p php_vm_cli --bin phrust-php

phpt-dev-build:
    RUSTC_WRAPPER= CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-1}" cargo build -q -p php_phpt_tools --bin php-phpt-tools -p php_vm_cli --bin phrust-php

phpt-dev-shell:
    @if [[ -n "${IN_NIX_SHELL:-}" ]]; then \
      just phpt-dev-build; \
      exec "${SHELL:-bash}"; \
    else \
      nix develop -c bash -lc 'just phpt-dev-build; exec "${SHELL:-bash}"'; \
    fi

phpt-generate-module *args:
    scripts/phpt/generate_module.sh {{args}}

phpt-module *args:
    scripts/phpt/module_run.sh {{args}}

phpt-dev-module *args:
    @PHPT_SKIP_BUILD=1 PHPT_REUSE_LAST="${PHPT_REUSE_LAST:-1}" PHPT_DEV_REUSE_TARGET_PASS="${PHPT_DEV_REUSE_TARGET_PASS:-1}" PHPT_TIMEOUT_SECONDS="${PHPT_TIMEOUT_SECONDS:-10}" PHPT_WORK_DIR="${PHPT_WORK_DIR:-/private/tmp/phrust-phpt-work}" scripts/phpt/module_run.sh {{args}}

phpt-module-target *args:
    scripts/phpt/module_target.sh {{args}}

phpt-fast *args:
    @PHPT_REQUIRE_FOCUS=1 PHPT_SKIP_BUILD=1 PHPT_REUSE_LAST="${PHPT_REUSE_LAST:-1}" PHPT_TIMEOUT_SECONDS="${PHPT_TIMEOUT_SECONDS:-3}" PHPT_WORK_DIR="${PHPT_WORK_DIR:-/private/tmp/phrust-phpt-work}" scripts/phpt/module_target.sh {{args}}

phpt-dev-fast *args:
    @PHPT_REQUIRE_FOCUS=1 PHPT_SKIP_BUILD=1 PHPT_DEV_REUSE_PASS=1 PHPT_REUSE_LAST="${PHPT_REUSE_LAST:-1}" PHPT_TIMEOUT_SECONDS="${PHPT_TIMEOUT_SECONDS:-3}" PHPT_WORK_DIR="${PHPT_WORK_DIR:-/private/tmp/phrust-phpt-work}" scripts/phpt/module_target.sh {{args}}

phpt-rerun-failures *args:
    @PHPT_SKIP_BUILD=1 PHPT_TIMEOUT_SECONDS="${PHPT_TIMEOUT_SECONDS:-3}" PHPT_WORK_DIR="${PHPT_WORK_DIR:-/private/tmp/phrust-phpt-work}" scripts/phpt/rerun_failures.sh {{args}}

phpt-full-regression *args:
    scripts/phpt/full_regression.sh {{args}}

phpt-full-fast *args:
    @PHPT_RUN_FULL=1 PHPT_SKIP_BUILD=1 PHPT_DEV_REUSE_PASS=1 PHPT_TIMEOUT_SECONDS="${PHPT_TIMEOUT_SECONDS:-30}" PHPT_WORK_DIR="${PHPT_WORK_DIR:-/private/tmp/phrust-phpt-work}" scripts/phpt/full_regression.sh {{args}}

phpt-triage *args:
    cargo build -q -p php_phpt_tools --bin php-phpt-tools
    ${CARGO_TARGET_DIR:-target}/debug/php-phpt-tools triage {{args}}

phpt-verify-baseline:
    cargo build -q -p php_phpt_tools --bin php-phpt-tools
    ${CARGO_TARGET_DIR:-target}/debug/php-phpt-tools verify-baseline

phpt-verify-source-integrity:
    scripts/phpt/verify_source_integrity.sh

phpt-source-lookup *args:
    cargo build -q -p php_phpt_tools --bin php-phpt-tools
    ${CARGO_TARGET_DIR:-target}/debug/php-phpt-tools lookup-symbol {{args}}

phpt-official-smoke *args:
    scripts/phpt/official_smoke.sh {{args}}

stdlib-docs:
    scripts/stdlib-docs.sh

stdlib-coverage:
    scripts/stdlib-coverage.sh

generate-arginfo php_src="third_party/php-src" out="crates/php_std/src/generated/arginfo.rs":
    scripts/stdlib/generate_arginfo.py --php-src "{{php_src}}" --overrides fixtures/stdlib/arginfo_overrides.txt --out "{{out}}"
    rustfmt --edition 2024 "{{out}}"

verify-generated-arginfo:
    scripts/stdlib/verify_generated_arginfo.sh

diff-stdlib:
    cargo build -q -p php_vm_cli --bin php-vm
    scripts/stdlib_diff.py --area stdlib --out target/stdlib/diff-stdlib --vm-binary ${CARGO_TARGET_DIR:-target}/debug/php-vm

diff-streams:
    cargo build -q -p php_vm_cli --bin php-vm
    scripts/stdlib_diff.py --area streams --out target/stdlib/diff-streams --vm-binary ${CARGO_TARGET_DIR:-target}/debug/php-vm

diff-json-pcre-date:
    cargo build -q -p php_vm_cli --bin php-vm
    scripts/stdlib_diff.py --area json-pcre-date --out target/stdlib/diff-json-pcre-date --vm-binary ${CARGO_TARGET_DIR:-target}/debug/php-vm

diff-spl-reflection:
    cargo build -q -p php_vm_cli --bin php-vm
    scripts/stdlib_diff.py --area spl-reflection --out target/stdlib/diff-spl-reflection --vm-binary ${CARGO_TARGET_DIR:-target}/debug/php-vm

composer-smoke:
    cargo build -q -p php_vm_cli --bin php-vm
    scripts/stdlib_diff.py --area composer --out target/stdlib/composer-smoke --vm-binary ${CARGO_TARGET_DIR:-target}/debug/php-vm

composer-fixture-prepare:
    scripts/stdlib/prepare_composer_fixture.sh

composer-smoke-source:
    scripts/stdlib/composer_source_smoke.sh

composer-smoke-autoload:
    cargo build -q -p php_vm_cli --bin php-vm
    scripts/stdlib_diff.py --file tests/fixtures/stdlib/_harness/composer/basic_project_autoload_order.php --out target/stdlib/composer-smoke-autoload --vm-binary ${CARGO_TARGET_DIR:-target}/debug/php-vm

composer-smoke-platform:
    cargo build -q -p php_vm_cli --bin php-vm
    scripts/stdlib_diff.py --file tests/fixtures/stdlib/_harness/composer/basic_project_platform_check.php --file tests/fixtures/stdlib/_harness/composer/platform_version_compare.php --out target/stdlib/composer-smoke-platform --vm-binary ${CARGO_TARGET_DIR:-target}/debug/php-vm

process-capability-smoke:
    scripts/stdlib/process_capability_smoke.sh

semantic-fixtures:
    scripts/run_semantic_fixtures.py
    scripts/run_semantic_fixtures.py --write-snapshots

semantic-reference-smoke:
    scripts/reference_php_frontend_json.py --file fixtures/semantic/valid/minimal.php

semantic-diff:
    scripts/compare_semantic_acceptance.py

semantic-diff-strict:
    scripts/compare_semantic_acceptance.py --strict

frontend-snapshots:
    cargo build -p php_frontend_cli
    ${CARGO_TARGET_DIR:-target}/debug/php-frontend --help >/dev/null
    ${CARGO_TARGET_DIR:-target}/debug/php-frontend analyze fixtures/semantic/valid/hello.php --format json >/dev/null
    ${CARGO_TARGET_DIR:-target}/debug/php-frontend diagnostics fixtures/semantic/functions/duplicate-param-invalid.php --format json >/dev/null
    ${CARGO_TARGET_DIR:-target}/debug/php-frontend symbols fixtures/semantic/classes/basic.php --format json >/dev/null
    ${CARGO_TARGET_DIR:-target}/debug/php-frontend scopes fixtures/semantic/scopes/closure-use.php --format json >/dev/null
    ${CARGO_TARGET_DIR:-target}/debug/php-frontend hir fixtures/semantic/php85/clone-with.php --format json >/dev/null
    ${CARGO_TARGET_DIR:-target}/debug/php-frontend snapshot fixtures/semantic/valid/minimal.php --output target/frontend-minimal.snap >/dev/null
    test -s target/frontend-minimal.snap

semantic-corpus-smoke:
    @printf '%s\n' '[skip] semantic corpus smoke is not configured for Semantic frontend; curated fixtures are covered by semantic-fixtures.'

fuzz-frontend-smoke:
    @printf '%s\n' '[skip] frontend fuzz smoke is not configured for Semantic frontend; parser fuzz smoke remains available via just fuzz-parser-smoke.'

bench-frontend:
    @printf '%s\n' '[skip] frontend benchmarks are not configured for Semantic frontend; no benchmark baseline is defined yet.'

bytecode-snapshots:
    cargo test -p php_ir --test bytecode_snapshots -- --nocapture

bytecode-exec-smoke:
    cargo build -p php_vm_cli
    scripts/performance/bytecode_exec_smoke.sh

bytecode-layout-smoke:
    cargo build -p php_vm_cli --bin php-vm
    scripts/performance/bytecode_layout_smoke.sh

superinstruction-smoke:
    cargo build -p php_vm_cli --bin php-vm
    scripts/performance/superinstruction_smoke.sh

superinstruction-patterns:
    cargo build -p php_vm_cli --bin php-vm
    scripts/performance/superinstruction_patterns.py --summary-doc docs/performance-superinstructions.md

vm-smoke:
    cargo build -p php_vm_cli --bin php-vm
    @tmp_dir="$PWD/target/vm-smoke"; \
    mkdir -p "$tmp_dir"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm compile fixtures/runtime/valid/hello.php --json > "$tmp_dir/hello.json"; \
    grep -q '"ok":true' "$tmp_dir/hello.json"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm dump-ir fixtures/runtime/valid/hello.php > "$tmp_dir/hello.ir"; \
    grep -q 'echo r0' "$tmp_dir/hello.ir"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/hello.php > "$tmp_dir/hello.out"; \
    printf 'hello runtime\n' > "$tmp_dir/hello.expected"; \
    cmp "$tmp_dir/hello.expected" "$tmp_dir/hello.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/runtime/valid/scalars/echo.php > "$tmp_dir/scalar.out"; \
    printf 'scalar echo\n' > "$tmp_dir/scalar.expected"; \
    cmp "$tmp_dir/scalar.expected" "$tmp_dir/scalar.out"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run fixtures/bytecode/lower/valid/empty.php > "$tmp_dir/empty.out"; \
    test ! -s "$tmp_dir/empty.out"; \
    printf '%s\n' '[ok] runtime VM smoke fixtures passed.'

vm-trace-smoke:
    cargo build -p php_vm_cli
    @tmp_dir="$PWD/target/runtime/failures"; \
    mkdir -p "$tmp_dir"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm run --trace fixtures/runtime/valid/variables/assignment.php > "$tmp_dir/trace-smoke.out" 2> "$tmp_dir/trace-smoke.trace"; \
    printf '1\n' > "$tmp_dir/trace-smoke.expected"; \
    cmp "$tmp_dir/trace-smoke.expected" "$tmp_dir/trace-smoke.out"; \
    grep -q 'vm-trace:' "$tmp_dir/trace-smoke.trace"; \
    grep -q 'function=main(0)' "$tmp_dir/trace-smoke.trace"; \
    grep -q 'output_len=' "$tmp_dir/trace-smoke.trace"; \
    ${CARGO_TARGET_DIR:-target}/debug/php-vm dump-ir fixtures/runtime/valid/variables/assignment.php --with-source > "$tmp_dir/trace-smoke.ir"; \
    grep -q '^source path=' "$tmp_dir/trace-smoke.ir"; \
    grep -q '^source 0001:' "$tmp_dir/trace-smoke.ir"; \
    grep -q '^--- ir ---' "$tmp_dir/trace-smoke.ir"; \
    printf '%s\n' '[ok] runtime VM trace/debug smoke passed.'

runtime-fixtures:
    cargo build -p php_vm_cli
    scripts/runtime_fixture_runner.py --self-test
    scripts/runtime_fixture_runner.py

runtime-corpus-smoke:
    scripts/runtime-corpus-smoke.sh

runtime-reference-smoke:
    cargo test -p php_testkit runtime_reference_smoke -- --nocapture

runtime-diff:
    cargo build -p php_vm_cli -p php_testkit --bin compare-runtime
    ${CARGO_TARGET_DIR:-target}/debug/compare-runtime --fixtures fixtures/runtime --out target/runtime/runtime-diff --rust-vm ${CARGO_TARGET_DIR:-target}/debug/php-vm

phpt-smoke:
    cargo build -p php_vm_cli -p php_testkit --bin run-phpt-smoke
    ${CARGO_TARGET_DIR:-target}/debug/run-phpt-smoke --fixtures fixtures/phpt_smoke --out target/runtime/phpt-smoke --rust-vm ${CARGO_TARGET_DIR:-target}/debug/php-vm

runtime-known-gaps:
    @just known-gaps
    scripts/runtime_gap_report.py --check
    cargo build -p php_vm_cli
    test -s docs/runtime-known-gaps.md
    grep -q 'E_PHP_RUNTIME_UNSUPPORTED_REFERENCE_SEMANTICS' docs/runtime-known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_GENERATOR' docs/runtime-known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_YIELD_FROM' docs/runtime-known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_FIBER' docs/runtime-known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_EVAL' docs/runtime-known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_AUTOLOAD' docs/runtime-known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_REFLECTION' docs/runtime-known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_TRAIT_RUNTIME' docs/runtime-known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_ENUM_RUNTIME' docs/runtime-known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_PROPERTY_HOOKS' docs/runtime-known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_BY_REF_FOREACH' docs/runtime-known-gaps.md
    grep -q 'E_PHP_RUNTIME_SUPERGLOBALS_FULL_MATRIX' docs/runtime-known-gaps.md
    grep -q 'E_PHP_RUNTIME_GLOBALS_ALIAS_MATRIX' docs/runtime-known-gaps.md
    test -f fixtures/runtime/valid/generators/yield.php
    test -f fixtures/runtime/valid/generators/yield-from.php
    test -f fixtures/runtime/valid/fibers/fiber.php
    test -f fixtures/runtime/valid/eval/eval.php
    test -f fixtures/runtime/known_gaps/autoload/spl-autoload-register.php
    test -f fixtures/runtime/valid/reflection/reflection-class.php
    test -f fixtures/runtime/valid/traits/trait-use.php
    test -f fixtures/runtime/valid/enums/unit-enum.php
    test -f fixtures/runtime/valid/property_hooks/get-hook.php
    test -f fixtures/runtime/valid/references/by-ref-return.php
    test -f fixtures/runtime/valid/references/array-element-ref.php
    test -f fixtures/runtime/known_gaps/foreach/by-ref.php
    test -f fixtures/runtime/valid/superglobals/globals-alias.php
    test -f fixtures/runtime/known_gaps/objects/clone-with-private.php
    test -f fixtures/runtime/known_gaps/objects/clone-with-readonly.php
    test -f fixtures/runtime/invalid/exceptions/nonmatching-catch-type.php

    @tmp_dir="target/runtime-known-gaps"; \
    mkdir -p "$tmp_dir"; \
    for fixture_id in \
      "fixtures/runtime/known_gaps/foreach/by-ref.php:E_PHP_IR_UNSUPPORTED_BY_REF_FOREACH:foreach-by-ref"; do \
      IFS=':' read -r fixture diagnostic name <<< "$fixture_id"; \
      set +e; \
      ${CARGO_TARGET_DIR:-target}/debug/php-vm run "$fixture" > "$tmp_dir/$name.out" 2> "$tmp_dir/$name.err"; \
      code=$?; \
      set -e; \
      test "$code" -eq 2; \
      grep -q "$diagnostic" "$tmp_dir/$name.err"; \
    done; \
    for fixture_id in \
      "fixtures/runtime/known_gaps/autoload/spl-autoload-register.php:E_PHP_VM_UNKNOWN_CLASS:autoload"; do \
      IFS=':' read -r fixture diagnostic name <<< "$fixture_id"; \
      set +e; \
      ${CARGO_TARGET_DIR:-target}/debug/php-vm run "$fixture" > "$tmp_dir/$name.out" 2> "$tmp_dir/$name.err"; \
      code=$?; \
      set -e; \
      test "$code" -eq 3; \
      grep -q "$diagnostic" "$tmp_dir/$name.err"; \
    done; \
    for fixture_id in \
      "fixtures/runtime/known_gaps/objects/clone-with-private.php:Cannot access private property:clone-with-private" \
      "fixtures/runtime/known_gaps/objects/clone-with-readonly.php:Cannot modify protected(set) readonly property:clone-with-readonly"; do \
      IFS=':' read -r fixture diagnostic name <<< "$fixture_id"; \
      set +e; \
      ${CARGO_TARGET_DIR:-target}/debug/php-vm run "$fixture" > "$tmp_dir/$name.out" 2> "$tmp_dir/$name.err"; \
      code=$?; \
      set -e; \
      test "$code" -eq 255; \
      grep -q 'Fatal error:' "$tmp_dir/$name.out"; \
      grep -q "$diagnostic" "$tmp_dir/$name.out"; \
      test ! -s "$tmp_dir/$name.err"; \
    done; \
    printf '%s\n' '[ok] runtime known-gap catalog and reference fixtures passed.'

runtime-gap-report:
    scripts/runtime_gap_report.py

known-gaps:
    scripts/known_gaps/validate.py

bench-vm-smoke:
    cargo build -p php_vm_cli
    mkdir -p target/runtime/bench-vm-smoke
    rustc --edition=2024 tools/bench_vm_smoke.rs -o target/runtime/bench-vm-smoke/bench-vm-smoke
    target/runtime/bench-vm-smoke/bench-vm-smoke

fuzz-vm-smoke:
    cargo build -p php_vm_cli
    mkdir -p target/runtime/fuzz-vm-smoke
    rustc --edition=2024 tools/fuzz_vm_smoke.rs -o target/runtime/fuzz-vm-smoke/fuzz-vm-smoke
    target/runtime/fuzz-vm-smoke/fuzz-vm-smoke

runtime-semantics-fixtures:
    @just refs-cow-fixtures
    @just object-semantics-fixtures
    @just generator-fiber-fixtures
    @just real-world-fixtures
    @just regression-fixtures
    @printf '%s\n' '[ok] runtime semantics fixture gates complete.'

runtime-semantics-diff *args:
    cargo build -p php_vm_cli
    scripts/runtime_semantics_diff.py {{args}}

vm-semantics-oracle *args:
    cargo build -p php_vm_cli --bin php-vm
    scripts/vm_semantics_oracle.py {{args}}

runtime-toolchain-audit:
    @for tool in cargo rustc rustfmt cargo-clippy just jq python3 rg clang sccache; do \
      if ! command -v "$tool" >/dev/null 2>&1; then \
        printf '%s\n' "[missing] required runtime semantics devshell tool: $tool" >&2; \
        exit 1; \
      fi; \
    done; \
    @if ! command -v shellcheck >/dev/null 2>&1; then \
      case "$$(uname -s)" in \
        Darwin) printf '%s\n' '[skip] shellcheck unavailable; Darwin devshell omits it to avoid the Haskell closure';; \
        *) printf '%s\n' '[missing] required runtime semantics devshell tool: shellcheck' >&2; exit 1;; \
      esac; \
    fi; \
    test "${PHP_REF_SERIES:-}" = "8.5"; \
    test "${PHP_REF_VERSION:-}" = "8.5.7"; \
    test "${PHP_REF_TAG:-}" = "php-8.5.7"; \
    test -n "${CARGO_TARGET_DIR:-}"; \
    test -n "${SCCACHE_DIR:-}"; \
    printf '%s\n' '[ok] runtime semantics devshell toolchain audit passed'

runtime-miri-smoke:
    @if ! command -v cargo-miri >/dev/null 2>&1 && ! cargo miri --version >/dev/null 2>&1; then \
      printf '%s\n' '[skip] cargo-miri is not available in this toolchain; install a Miri-capable Rust toolchain to run this opt-in smoke.'; \
      exit 0; \
    fi; \
    if ! cargo miri --version >/dev/null 2>&1; then \
      printf '%s\n' '[skip] cargo-miri is present but not usable for the active toolchain; this opt-in smoke is not part of verify-runtime.'; \
      exit 0; \
    fi; \
    cargo miri test -p php_runtime reference::tests::slot_alias_and_copy_semantics_are_distinct

runtime-sanitizer-smoke:
    @if [[ "${PHRUST_RUN_SANITIZER:-0}" != "1" ]]; then \
      printf '%s\n' '[skip] set PHRUST_RUN_SANITIZER=1 to run the opt-in sanitizer smoke.'; \
      exit 0; \
    fi; \
    if [[ "$$(uname -s)" != "Linux" ]]; then \
      printf '%s\n' '[skip] sanitizer smoke is currently supported only on Linux devshells.'; \
      exit 0; \
    fi; \
    if ! command -v clang >/dev/null 2>&1; then \
      printf '%s\n' '[skip] clang is required for sanitizer smoke.'; \
      exit 0; \
    fi; \
    if ! rustc --version | grep -qi nightly; then \
      printf '%s\n' '[skip] sanitizer smoke requires a nightly Rust toolchain with -Zsanitizer support.'; \
      exit 0; \
    fi; \
    RUSTFLAGS="-Zsanitizer=address" cargo test -p php_runtime gc::tests::gc_scans_roots_and_refcount_metadata_without_panics

runtime-fuzz-smoke:
    cargo build -p php_vm_cli
    scripts/runtime_semantics_fuzz_smoke.py

runtime-bench-smoke:
    cargo build -p php_vm_cli
    scripts/runtime_semantics_bench_smoke.py

runtime-composer-smoke:
    @if [ -z "${PHPRUST_COMPOSER_FIXTURE_DIR:-}" ]; then \
        scripts/runtime_composer_smoke.py; \
    else \
        cargo build -p php_vm_cli; \
        scripts/runtime_composer_smoke.py; \
    fi

refs-cow-fixtures:
    cargo build -p php_vm_cli
    scripts/runtime_semantics_diff.py --category refs --category cow --out target/runtime-semantics/refs-cow

object-semantics-fixtures:
    cargo build -p php_vm_cli
    scripts/runtime_semantics_diff.py --category objects --category traits --category enums --category magic --category properties --category property_hooks --category clone_with --out target/runtime-semantics/object-semantics

generator-fiber-fixtures:
    cargo build -p php_vm_cli
    scripts/runtime_semantics_diff.py --category generators --category fibers --out target/runtime-semantics/generator-fiber

real-world-fixtures:
    cargo build -p php_vm_cli
    scripts/runtime_semantics_diff.py --category real_world --out target/runtime-semantics/real-world

wordpress-blockers:
    cargo build -p php_vm_cli
    scripts/runtime_semantics_diff.py --category wordpress_blockers --out target/runtime-semantics/wordpress-blockers

wp-language-vm:
    cargo build -p php_vm_cli
    scripts/runtime_semantics_diff.py --category wp_language_vm --out target/runtime-semantics/wp-language-vm

wp-autoload-stdlib:
    cargo build -p php_vm_cli
    scripts/runtime_semantics_diff.py --category wp_autoload_stdlib --out target/runtime-semantics/wp-autoload-stdlib
    scripts/wordpress_builtin_heatmap.py --input target/runtime-semantics/wp-autoload-stdlib/runtime-semantics-diff-report.json --out target/wordpress-bringup

wordpress-preflight:
    scripts/wordpress/preflight.py --wordpress-dir "${PHRUST_WORDPRESS_DIR:-}" --docroot "${PHRUST_WORDPRESS_DOCROOT:-${PHRUST_WORDPRESS_DIR:-}}" --reference-php "${REFERENCE_PHP:-}" --phrust-binary "${PHP_VM_CLI:-target/debug/php-vm}" --phrust-server "${PHRUST_SERVER:-target/debug/phrust-server}" --out target/wordpress-real/preflight.json

wordpress-real-smoke:
    cargo build -p php_vm_cli -p php_server
    scripts/wordpress/smoke.py --phase web-frontpage --wordpress-dir "${PHRUST_WORDPRESS_DIR:-}" --docroot "${PHRUST_WORDPRESS_DOCROOT:-${PHRUST_WORDPRESS_DIR:-}}" --reference-php "${REFERENCE_PHP:-}" --phrust-binary "${PHP_VM_CLI:-target/debug/php-vm}" --phrust-server "${PHRUST_SERVER:-target/debug/phrust-server}" --stop-on-fail

wordpress-real-install-smoke:
    cargo build -p php_vm_cli -p php_server
    scripts/wordpress/smoke.py --phase db-install --phase admin-login-page --phase post-install-frontpage --wordpress-dir "${PHRUST_WORDPRESS_DIR:-}" --docroot "${PHRUST_WORDPRESS_DOCROOT:-${PHRUST_WORDPRESS_DIR:-}}" --reference-php "${REFERENCE_PHP:-}" --phrust-binary "${PHP_VM_CLI:-target/debug/php-vm}" --phrust-server "${PHRUST_SERVER:-target/debug/phrust-server}" --stop-on-fail

wordpress-real-extract-first-failure:
    scripts/wordpress/extract_failure.py --failure "${PHRUST_WORDPRESS_FIRST_FAILURE:-}"

mysqli-integration:
    scripts/mysqli_integration.py

wordpress-smoke-report:
    scripts/wordpress_smoke_report.py

regression-fixtures:
    cargo build -p php_vm_cli
    scripts/runtime_semantics_diff.py --category regressions --out target/runtime-semantics/regressions

local-composer-smoke *paths:
    @if [ -z "{{paths}}" ]; then \
        printf '%s\n' '[skip] provide one or more local package-manager project paths.'; \
        exit 0; \
    fi; \
    cargo build -p php_vm_cli; \
    args=''; \
    for path in {{paths}}; do args="$$args --dir $$path"; done; \
    scripts/runtime_semantics_diff.py $$args --out target/runtime-semantics/local-composer-smoke

runtime-phpt-smoke:
    cargo build -p php_vm_cli -p php_testkit --bin run-phpt-smoke
    ${CARGO_TARGET_DIR:-target}/debug/run-phpt-smoke --fixtures /private/tmp/phrust-empty-phpt-smoke --out target/runtime-semantics/phpt-smoke --rust-vm ${CARGO_TARGET_DIR:-target}/debug/php-vm --allowlist fixtures/runtime_semantics/phpt_allowlist.toml

extension-phpt-smoke:
    cargo build -p php_vm_cli -p php_testkit --bin run-phpt-smoke
    scripts/stdlib/phpt_extension_selector.py

compat-corpus-smoke:
    cargo build -p php_vm_cli
    scripts/stdlib_diff.py --fixtures tests/fixtures/stdlib/corpus --area corpus --out target/stdlib/corpus

performance-tests:
    RUST_MIN_STACK="${PHRUST_RUST_MIN_STACK:-8388608}" cargo test --workspace
    scripts/performance/compare_perf_json.py --self-test
    scripts/performance/hotpath_inventory.py --self-test
    scripts/performance/fastest_hotpath_report.py --self-test
    scripts/performance/perf_report.py --self-test
    scripts/performance/app_flow_matrix.py --self-test

performance-regression:
    scripts/performance_regression_smoke.sh
    cargo build -p php_vm_cli --bin php-vm
    scripts/performance/regression_smoke.sh
    @just perf-flag-matrix
    @just polymorphic-inline-cache-smoke

perf-flag-matrix:
    scripts/performance/perf_flag_matrix.py

default-profile-smoke:
    cargo build -p php_vm_cli --bin php-vm
    scripts/performance/default_profile_smoke.py

managed-fast-coverage:
    cargo build -p php_vm_cli --bin php-vm
    scripts/performance/managed_fast_coverage.py

fast-preset-smoke:
    cargo build -p php_vm_cli --bin php-vm
    scripts/performance/fast_preset_smoke.py

baseline-native-stencil-smoke:
    cargo build -p php_vm_cli --bin php-vm
    scripts/performance/baseline_native_stencil_smoke.py

copy-patch-stencil-smoke:
    cargo build -p php_vm_cli --bin php-vm
    scripts/performance/copy_patch_stencil_smoke.py

mid-tier-plan-smoke:
    cargo build -p php_vm_cli --bin php-vm
    scripts/performance/mid_tier_plan_smoke.py

ir-verify:
    cargo test -p php_ir verify --lib

benchmark-smoke:
    cargo build -p php_vm_cli --bin php-vm
    scripts/performance/bench_matrix.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm" --out target/performance/benchmark-smoke.json --repetitions "${PHRUST_PERF_BENCH_SMOKE_REPETITIONS:-1}" --warmups "${PHRUST_PERF_BENCH_SMOKE_WARMUPS:-0}" --timeout "${PHRUST_PERF_BENCH_TIMEOUT:-10.0}"

callgrind-smoke:
    scripts/performance/callgrind_smoke.sh

rust-hotpath-bench:
    CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-target}" cargo bench --manifest-path crates/php_bench/Cargo.toml --bench perf_hotpaths

benchmark-suite:
    cargo build -p php_vm_cli --bin php-vm
    scripts/performance/bench_matrix.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm" --out target/performance/benchmark-suite.json --repetitions "${PHRUST_PERF_BENCH_REPETITIONS:-5}" --warmups "${PHRUST_PERF_BENCH_WARMUPS:-1}" --timeout "${PHRUST_PERF_BENCH_TIMEOUT:-5.0}"
    @just rust-hotpath-bench

profile-dispatch:
    scripts/performance/profile_smoke.sh dispatch

profile-arrays:
    scripts/performance/profile_smoke.sh arrays

profile-calls:
    scripts/performance/profile_smoke.sh calls

profile-composer:
    scripts/performance/profile_smoke.sh composer

release-profile-plan:
    scripts/performance/release_profile_plan.sh

release-benchmark-smoke:
    scripts/performance/release_profiles.py release

pgo-benchmark-smoke:
    scripts/performance/release_profiles.py pgo

bolt-benchmark-smoke:
    scripts/performance/release_profiles.py bolt

framework-smoke:
    cargo build -p php_vm_cli --bin php-vm
    scripts/performance/framework_micro_smoke.py

app-flow-smoke:
    cargo build -p php_vm_cli --bin php-vm
    scripts/performance/app_flow_matrix.py --smoke --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm" --timeout "${PHRUST_APP_FLOW_TIMEOUT:-30.0}"

app-flow-matrix:
    cargo build -p php_vm_cli --bin php-vm
    scripts/performance/app_flow_matrix.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm" --iterations "${PHRUST_APP_FLOW_ITERATIONS:-5}" --warmups "${PHRUST_APP_FLOW_WARMUPS:-1}" --scale "${PHRUST_APP_FLOW_SCALE:-2}" --timeout "${PHRUST_APP_FLOW_TIMEOUT:-30.0}"

acceleration-matrix:
    cargo build -p php_vm_cli --bin php-vm
    scripts/performance/acceleration_matrix.py

fastest-engine-matrix:
    cargo build -p php_vm_cli --bin php-vm
    scripts/performance/fastest_engine_matrix.py

hotpath-inventory:
    scripts/performance/hotpath_inventory.py target/performance/benchmark-smoke.json --json-out target/performance/hotpaths.json --markdown-out docs/hotpath-inventory.md

fastest-hotpath-report:
    cargo build -p php_vm_cli --bin php-vm
    @if [ ! -f target/performance/benchmark-smoke.json ]; then \
        scripts/performance/bench_matrix.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm" --out target/performance/benchmark-smoke.json --repetitions "${PHRUST_PERF_BENCH_SMOKE_REPETITIONS:-1}" --warmups "${PHRUST_PERF_BENCH_SMOKE_WARMUPS:-0}" --timeout "${PHRUST_PERF_BENCH_TIMEOUT:-10.0}"; \
    fi
    scripts/performance/fastest_hotpath_report.py --benchmark target/performance/benchmark-smoke.json --framework target/performance/framework-smoke/summary.json --acceleration target/performance/acceleration/summary.json --json-out target/performance/fastest/hotpath-report.json --markdown-out target/performance/fastest/hotpath-report.md --summary-doc docs/performance-fastest-hotpaths.md

perf-baseline:
    cargo build -p php_vm_cli --bin php-vm
    scripts/performance/bench_matrix.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm" --out target/performance/baseline.json --repetitions "${PHRUST_PERF_BASELINE_REPETITIONS:-3}" --warmups "${PHRUST_PERF_BASELINE_WARMUPS:-1}" --timeout "${PHRUST_PERF_BENCH_TIMEOUT:-5.0}"

perf-compare:
    @if [ ! -f target/performance/baseline.json ]; then \
        printf '%s\n' '[skip] target/performance/baseline.json missing; run `just perf-baseline` first'; \
        exit 0; \
    fi
    @just benchmark-smoke
    scripts/performance/compare_perf_json.py target/performance/baseline.json target/performance/benchmark-smoke.json --out target/performance/perf-compare.md --json-out target/performance/perf-compare.json

cache-roundtrip:
    @just cache-fingerprint-smoke
    @just dependency-units-smoke
    cargo test -p php_bytecode_cache bytecode_cache
    cargo test -p php_vm_cli bytecode_cache

cache-fingerprint-smoke:
    scripts/performance/cache_fingerprint_smoke.sh

dependency-units-smoke:
    scripts/performance/dependency_units_smoke.sh

templates-smoke:
    scripts/performance/templates_smoke.sh

optimizer-diff:
    @just ir-verify
    scripts/performance/optimizer_diff_smoke.sh

quickening-smoke:
    cargo build -p php_vm_cli --bin php-vm
    scripts/performance/quickening_smoke.sh

inline-cache-smoke:
    cargo build -p php_vm_cli --bin php-vm
    scripts/performance/inline_cache_smoke.sh

polymorphic-inline-cache-smoke:
    cargo build -p php_vm_cli --bin php-vm
    scripts/performance/polymorphic_inline_cache_smoke.sh

jit-smoke:
    scripts/performance/jit_smoke.sh

jit-cranelift-smoke:
    @set +e; scripts/performance/cranelift/platform_check.py --out target/performance/cranelift/platform.json; status=$?; set -e; if [ "$status" -eq 77 ]; then exit 0; elif [ "$status" -ne 0 ]; then exit "$status"; fi
    cargo check --workspace
    @if cargo tree -p php_jit --no-default-features -e features | rg 'cranelift-' >/dev/null; then \
        printf '%s\n' '[fail] default php_jit dependency tree unexpectedly includes Cranelift crates' >&2; \
        cargo tree -p php_jit --no-default-features -e features >&2; \
        exit 1; \
    fi
    cargo check --workspace --features jit-cranelift
    cargo test -p php_jit --features jit-cranelift
    cargo test -p php_vm --features jit-cranelift jit_
    cargo test -p php_vm --features jit-cranelift cranelift_
    cargo build -p php_vm_cli --bin php-vm --features jit-cranelift
    @printf '%s\n' '[pass] Cranelift feature-gating smoke passed'

jit-cranelift-diff:
    @set +e; scripts/performance/cranelift/platform_check.py --out target/performance/cranelift/platform.json; status=$?; set -e; if [ "$status" -eq 77 ]; then exit 0; elif [ "$status" -ne 0 ]; then exit "$status"; fi
    cargo build -p php_vm_cli --bin php-vm --features jit-cranelift
    scripts/performance/cranelift/jit_diff.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm"

jit-cranelift-bench-smoke:
    @set +e; scripts/performance/cranelift/platform_check.py --out target/performance/cranelift/platform.json; status=$?; set -e; if [ "$status" -eq 77 ]; then exit 0; elif [ "$status" -ne 0 ]; then exit "$status"; fi
    @just jit-cranelift-diff
    cargo build -p php_vm_cli --bin php-vm --features jit-cranelift
    scripts/performance/cranelift/jit_bench_matrix.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm" --out target/performance/cranelift/bench-smoke.json --smoke

jit-cranelift-report:
    @set +e; scripts/performance/cranelift/platform_check.py --out target/performance/cranelift/platform.json; status=$?; set -e; if [ "$status" -eq 77 ]; then exit 0; elif [ "$status" -ne 0 ]; then exit "$status"; fi
    @just jit-cranelift-diff
    cargo build -p php_vm_cli --bin php-vm --features jit-cranelift
    scripts/performance/cranelift/jit_bench_matrix.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm" --out target/performance/cranelift/big_wins_report.json

cranelift-guard-report:
    @set +e; scripts/performance/cranelift/platform_check.py --out target/performance/cranelift/platform.json; status=$?; set -e; if [ "$status" -eq 77 ]; then exit 0; elif [ "$status" -ne 0 ]; then exit "$status"; fi
    @just jit-cranelift-report
    scripts/performance/cranelift/guard_failure_report.py --input target/performance/cranelift/big_wins_report.json --out target/performance/cranelift/guard-report.json --text-out target/performance/cranelift/guard-report.txt

jit-cranelift-disasm:
    @set +e; scripts/performance/cranelift/platform_check.py --out target/performance/cranelift/platform.json; status=$?; set -e; if [ "$status" -eq 77 ]; then exit 0; elif [ "$status" -ne 0 ]; then exit "$status"; fi
    cargo build -p php_vm_cli --bin php-vm --features jit-cranelift
    scripts/performance/cranelift/disasm_dump.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm"

jit-cranelift-fuzz-smoke:
    @set +e; scripts/performance/cranelift/platform_check.py --out target/performance/cranelift/platform.json; status=$?; set -e; if [ "$status" -eq 77 ]; then exit 0; elif [ "$status" -ne 0 ]; then exit "$status"; fi
    cargo build -p php_vm_cli --bin php-vm --features jit-cranelift
    scripts/performance/cranelift/jit_eligible_ir_fuzz_smoke.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm"

jit-cranelift-poly-ic-experiment:
    @set +e; scripts/performance/cranelift/platform_check.py --out target/performance/cranelift/platform.json; status=$?; set -e; if [ "$status" -eq 77 ]; then exit 0; elif [ "$status" -ne 0 ]; then exit "$status"; fi
    cargo build -p php_vm_cli --bin php-vm --features jit-cranelift
    scripts/performance/cranelift/polymorphic_ic_experiment.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm"
    @just jit-cranelift-report
    scripts/performance/cranelift/guard_failure_report.py --input target/performance/cranelift/big_wins_report.json --out target/performance/cranelift/polymorphic-ic/guard-report.json --text-out target/performance/cranelift/polymorphic-ic/guard-report.txt --experimental-ic-report target/performance/cranelift/polymorphic-ic/report.json

jit-cranelift-framework-smoke:
    @set +e; scripts/performance/cranelift/platform_check.py --out target/performance/cranelift/platform.json; status=$?; set -e; if [ "$status" -eq 77 ]; then exit 0; elif [ "$status" -ne 0 ]; then exit "$status"; fi
    cargo build -p php_vm_cli --bin php-vm --features jit-cranelift
    scripts/performance/cranelift/framework_smoke.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm"

verify-cranelift:
    @set +e; scripts/performance/cranelift/platform_check.py --out target/performance/cranelift/platform.json; status=$?; set -e; if [ "$status" -eq 77 ]; then exit 0; elif [ "$status" -ne 0 ]; then exit "$status"; fi
    @just jit-cranelift-smoke
    @just jit-cranelift-diff
    @just jit-cranelift-bench-smoke
    @just jit-cranelift-report
    @just cranelift-guard-report
    @just jit-cranelift-fuzz-smoke
    @printf '%s\n' '[pass] Cranelift verification complete'

dump-cranelift-clif:
    cargo run -p php_vm_cli --bin php-vm --features jit-cranelift -- dump-cranelift-clif

safety-audit-smoke:
    @if rg -n '\bunsafe\b' crates/php_bytecode_cache crates/php_vm/src/inline_cache.rs crates/php_vm/src/quickening.rs crates/php_vm/src/tiering.rs; then \
        printf '%s\n' '[fail] performance cache/JIT/adaptive surface contains Rust unsafe' >&2; \
        exit 1; \
    fi
    @if rg -n '\bunsafe\b' crates/php_jit/src --glob '!lib.rs' --glob '!helpers.rs' --glob '!cranelift_lowering.rs'; then \
        printf '%s\n' '[fail] performance default JIT surface contains unaudited Rust unsafe' >&2; \
        exit 1; \
    fi
    @test -f docs/safety-audit-cranelift.md
    cargo test -p php_bytecode_cache corrupt
    cargo test -p php_vm_cli bytecode_cache
    @if ! command -v cargo-miri >/dev/null 2>&1 && ! cargo miri --version >/dev/null 2>&1; then \
        printf '%s\n' '[skip] cargo-miri is not available in this toolchain; safety audit smoke skipped.'; \
        exit 0; \
    fi; \
    if ! cargo miri --version >/dev/null 2>&1; then \
        printf '%s\n' '[skip] cargo-miri is present but not usable for the active toolchain; safety audit smoke skipped.'; \
        exit 0; \
    fi; \
    cargo miri test -p php_bytecode_cache rejects_corrupt_input

perf-report:
    scripts/performance/perf_report.py
