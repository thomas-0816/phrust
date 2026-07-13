set shell := ["bash", "-euo", "pipefail", "-c"]

# Gates must be hermetic and deterministic under php-vm's default-on caching:
# keep the bytecode cache repo-local (never the user cache directory) and
# disable the default persistent-feedback sidecar, whose seeding shifts
# adaptive counters between otherwise identical runs. Explicit per-gate flags
# and pre-set environment values still win.
export PHRUST_BYTECODE_CACHE_DIR := env_var_or_default("PHRUST_BYTECODE_CACHE_DIR", justfile_directory() / "target" / "gate-bytecode-cache")
export PHRUST_PERSISTENT_FEEDBACK := env_var_or_default("PHRUST_PERSISTENT_FEEDBACK", "off")

help:
    @printf '%s\n' \
      'Available commands:' \
      '  just help                 Show this help' \
      '  just check                Format, lint, and run workspace tests' \
      '  just verify               Run the full local verification gate' \
      '  just verify-frontend      Lexer, parser, CST, AST, semantics, CLI snapshots' \
      '  just verify-runtime       IR, VM, runtime fixtures + oracle diffs' \
      '  just verify-stdlib        Builtins, streams, JSON/PCRE/date, SPL/reflection' \
      '  just verify-server        Integrated HTTP server tests and smoke gate' \
      '  just verify-performance   Optimizer, cache, quickening, IC, JIT smoke gates' \
      '  just verify-performance-extended  Release-profile and hotpath-report gates' \
      '  just verify-phpt          PHPT tooling, manifests, and source-integrity checks' \
      '  just known-gaps           Validate checked known-gap manifests' \
      '  just source-integrity      Check module wiring and generated metadata' \
      '  just architecture-guardrails Enforce all architecture rule classes' \
      '  just architecture-inventory Check source-derived architecture baseline' \
      '  just architecture-performance-baseline Capture compile/runtime architecture metrics' \
      '  just dependency-boundaries Check documented workspace dependency edges' \
      '  just runtime-core-boundaries Check core and extension dependency direction' \
      '  just request-state-boundaries Check typed request-state ownership and views' \
      '  just panic-unwrap-policy   Check production panic/unwrap policy' \
      '  just stdlib-registry-drift Check stdlib/runtime registry drift' \
      '  just verify-generated-arginfo Strict php-src arginfo drift check' \
      '  just verify-generated-extension-surfaces Strict canonical descriptor drift check' \
      '  just oracle-api-index    Generate php-src/reference API oracle JSONL' \
      '  just oracle-api-summary  Print the latest API oracle summary' \
      '  just oracle-probe-generate Generate bounded oracle runtime probes' \
      '  just oracle-probe-smoke Run smoke oracle probes through runtime diff' \
      '  just oracle-gap-report  Generate prioritized oracle gap queue' \
      '  just oracle-next-gap-prompt Emit prompt for highest-priority gap family' \
      '  just oracle-smoke       Run cheap oracle API/probe/gap gate' \
      '  just verify-oracle      Run strict oracle verification when configured' \
      '  just fmt                  Check Rust formatting' \
      '  just lint                 Run Rust linting' \
      '  just test                 Run Rust workspace tests' \
      '  just diagnostics-audit    Run diagnostic quality ratchet' \
      '  just diagnostics-smoke    Run structured diagnostic smoke checks' \
      '  just debug-smoke          Run debug-mode JSONL smoke checks' \
      '  just php ARGS             Run phrust-php with PHP-compatible flags' \
      '  just serve DOCROOT=public [LISTEN=127.0.0.1:8080] Start phrust-php -S' \
      '  just serve-advanced DOCROOT=public [LISTEN=127.0.0.1:8080] Start phrust-server' \
      '  just install-user-bin     Install target/phrust/bin php shim' \
      '  just verify-user-interfaces Run phrust-php CLI and server smoke gates' \
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
      '  just wordpress-real-perf-report Run optional local real WordPress perf report' \
      '  just wordpress-root-profile Run optional local real WordPress request profile' \
      '  just wordpress-arm64-sample Capture external ARM64 WordPress CPU samples' \
      '  just wordpress-arm64-accounting-gate Classify and gate ARM64 CPU samples' \
      '  just wordpress-reference-image Build pinned PHP-FPM/OPcache benchmark image' \
      '  just wordpress-root-benchmark Run clean Phrust vs PHP-FPM WordPress gate' \
      '  just wordpress-root-tranche-gate Run strict c1-p50 performance acceptance gate' \
      '  just wordpress-root-benchmark-feedback-ab Run persistent-feedback A/B' \
      '  just wordpress-root-benchmark-cranelift Run experimental Cranelift arm' \
      '  just worker-adaptive-state-smoke Verify worker-local adaptive reuse and isolation' \
      '  just wordpress-root-diagnostics Run timing-ineligible Phrust diagnostics' \
      '  just wordpress-dense-fallback-report Summarize dense fallback attribution from latest request profile' \
      '  just wordpress-clone-churn-report Summarize clone/COW attribution from latest request profile' \
      '  just wordpress-array-hotpath-report Summarize array hotpath attribution from latest request profile' \
      '  just wordpress-call-hotpath-report Summarize call/frame attribution from latest request profile' \
      '  just wordpress-persistent-metadata-report Summarize persistent metadata attribution from latest request profile' \
      '  just wordpress-native-region-report Summarize native-region attribution from latest request profile' \
      '  just mysqli-integration   Run explicit live MySQLi integration gate' \
      '  just wordpress-smoke-report Generate web/db diagnostics report' \
      '' \
      'Server:' \
      '  just verify-server        Run integrated web server verification' \
      '  just server-smoke         Run integrated web server smoke checks' \
      '  just cli-interface-smoke  Run phrust-php CLI compatibility smoke checks' \
      '  just cli-server-smoke     Run phrust-php -S built-in server smoke checks' \
      '  just server-compat-smoke [SECTION=all] Run Wave 2 server compatibility smoke checks' \
      '  just server-tls-smoke     Run integrated HTTPS server smoke checks' \
      '  just server-benchmark-smoke Run short optional server benchmark smoke' \
      '' \
      'Standard library and compatibility:' \
      '  just generate-arginfo     Regenerate stdlib arginfo from php-src stubs' \
      '  just verify-generated-arginfo Regenerate and diff committed arginfo' \
      '  just generate-extension-surfaces Regenerate canonical extension Rust surfaces' \
      '  just verify-generated-extension-surfaces Regenerate and diff extension surfaces' \
      '  just oracle-api-index    Generate php-src/reference API oracle JSONL' \
      '  just oracle-api-summary  Print the latest API oracle summary' \
      '  just oracle-probe-generate Generate bounded oracle runtime probes' \
      '  just oracle-probe-smoke Run smoke oracle probes through runtime diff' \
      '  just oracle-probe-full  Run full generated oracle probe set' \
      '  just oracle-gap-report  Generate prioritized oracle gap queue' \
      '  just oracle-next-gap-prompt Emit prompt for highest-priority gap family' \
      '  just oracle-smoke       Run cheap oracle API/probe/gap gate' \
      '  just verify-oracle      Run strict oracle verification when configured' \
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
      '  just front-controller-hotpath-smoke Run deterministic server hotpath smoke' \
      '  just app-flow-smoke      Run CI-safe app-flow engine comparison smoke' \
      '  just runtime-layout-performance-smoke Run runtime-layout tranche counter gate' \
      '  just app-flow-matrix     Run full application-flow Phrust/reference matrix' \
      '  just release-benchmark-smoke Run production release performance smoke' \
      '  just pgo-benchmark-smoke  Run optional PGO performance smoke' \
      '  just bolt-benchmark-smoke Run optional Linux BOLT performance smoke' \
      '  just fastest-engine-matrix Generate baseline/release/reference comparison matrix' \
      '  just fastest-hotpath-report Generate fastest-engine hotpath report' \
      '  just perf-decision-baseline Generate startup/compile/execute decision baseline' \
      '  just startup-matrix      Run debug/release startup attribution matrix' \
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
      '  just pre-commit          Run the lightweight local commit gate' \
      '  just pre-push            Run the bounded local push gate' \
      '  just ci-local            Run the local GitHub Actions parity gate'

install-hooks:
    scripts/git/install-hooks.sh

pre-commit:
    @just fmt
    @just source-integrity

pre-push:
    @just pre-commit
    PHRUST_DIAGNOSTICS_AUDIT_QUIET=1 scripts/diagnostics/audit.py
    @just known-gaps
    @just quality-deps-quiet
    cargo check --workspace --all-targets --all-features

fmt:
    cargo fmt --all --check

lint:
    cargo clippy --workspace --all-targets -- -D warnings

runtime-hardening-lints:
    # unsafe enforcement lives in the crate roots (#![deny(unsafe_code)]);
    # php_runtime carves out only the audited runtime_memory module.
    cargo clippy -p php_runtime -p php_vm --all-targets -- -D warnings

test:
    RUST_MIN_STACK="${PHRUST_RUST_MIN_STACK:-8388608}" cargo test --workspace

diagnostics-audit:
    scripts/diagnostics/audit.py

server-smoke:
    scripts/server/smoke.sh

php *ARGS:
    cargo run -p php_vm_cli --bin phrust-php -- {{ARGS}}

serve DOCROOT="public" LISTEN="127.0.0.1:8080":
    cargo run -p php_vm_cli --bin phrust-php -- -S {{LISTEN}} -t {{DOCROOT}}

serve-advanced DOCROOT="public" LISTEN="127.0.0.1:8080":
    cargo run -p php_server --bin phrust-server -- --docroot {{DOCROOT}} --listen {{LISTEN}}

install-user-bin:
    scripts/install-user-bin.sh

cli-interface-smoke:
    scripts/cli/interface_smoke.sh

cli-server-smoke:
    scripts/cli/builtin_server_smoke.sh

verify-user-interfaces:
    @just cli-interface-smoke
    @just cli-server-smoke

diagnostics-smoke:
    scripts/diagnostics/smoke.sh diagnostics

debug-smoke:
    scripts/diagnostics/smoke.sh debug

verify-server:
    #!/usr/bin/env bash
    set -euo pipefail
    # Combined runs (ci-local, verify) execute the workspace test suite,
    # which contains these crates' suites; standalone runs keep them.
    if [[ "${PHRUST_COMBINED_RUN:-0}" == "1" ]]; then
      printf '%s\n' '[skip] php_executor/php_server suites covered by the combined workspace tests'
    else
      cargo test -p php_executor -p php_server
    fi
    just server-smoke

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
    @just architecture-guardrails
    scripts/verify/panic_unwrap_policy.py
    scripts/verify/docs_strategy.py

architecture-guardrails:
    scripts/verify/architecture_guardrails.py --self-test
    scripts/verify/architecture_guardrails.py
    scripts/verify/dependency_boundaries.py
    scripts/verify/validation_strategy.py --self-test
    scripts/verify/validation_strategy.py

architecture-inventory:
    scripts/verify/architecture_inventory.py --check --verify-determinism

architecture-performance-baseline *args:
    scripts/performance/architecture_baseline.py {{args}}

dependency-boundaries:
    scripts/verify/dependency_boundaries.py

runtime-core-boundaries:
    scripts/verify/runtime_core_boundaries.py
    cargo check -p php_runtime --no-default-features

request-state-boundaries:
    scripts/verify/request_state_boundaries.py

panic-unwrap-policy:
    scripts/verify/panic_unwrap_policy.py

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

quality-deps-quiet:
    @if ! command -v cargo-deny >/dev/null 2>&1; then \
        printf '%s\n' '[skip] cargo-deny unavailable; enter nix develop or install cargo-deny.'; \
        exit 0; \
    fi; \
    cargo deny check --hide-inclusion-graph -A duplicate advisories bans licenses sources

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
    @PHRUST_COMBINED_RUN=1 just verify-server
    @just verify-performance
    @just verify-performance-extended
    @PHRUST_COMBINED_RUN=1 just verify-phpt

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
    @PHRUST_COMBINED_RUN=1 just ci-domain-gates
    @just ci-phpt-smoke

verify-frontend:
    @just lexer-fixtures
    @just parser-fixtures
    @just cst-roundtrip
    @just semantic-fixtures
    @just semantic-diff
    @just frontend-snapshots

verify-runtime:
    #!/usr/bin/env bash
    set -euo pipefail
    # Mirror CI: the runtime gate diffs against the pinned PHP 8.5.7 oracle.
    # CI builds it and exports REFERENCE_PHP; locally (pre-push ci-local,
    # manual runs) adopt the built reference automatically when REFERENCE_PHP
    # is unset. An explicitly set REFERENCE_PHP always wins (strict), and the
    # built binary is adopted only when it reports exactly 8.5.7, preserving
    # the skip-unless-8.5.7 contract for php_ref_required fixtures.
    if [[ -z "${REFERENCE_PHP:-}" && -x third_party/php-src/sapi/cli/php ]]; then
      if third_party/php-src/sapi/cli/php --version 2>/dev/null | head -1 | grep -q 'PHP 8\.5\.7'; then
        export REFERENCE_PHP="$PWD/third_party/php-src/sapi/cli/php"
        printf '%s\n' "[verify-runtime] adopted built reference PHP: $REFERENCE_PHP"
      fi
    fi
    just bytecode-snapshots
    just bytecode-exec-smoke
    just vm-smoke
    just vm-trace-smoke
    just runtime-fixtures
    just runtime-known-gaps
    # The per-category fixture gates (runtime-semantics-fixtures) are focused
    # iteration tools; the full diff below runs a strict superset of them, so
    # the aggregate does not repeat the categories. runtime-hardening-lints is
    # likewise a strict subset of `just lint` since ADR 0020 moved unsafe
    # enforcement into the crate roots; combined runs and CI's parallel
    # workspace job already clippy these crates.
    just runtime-semantics-diff
    just vm-semantics-oracle

verify-stdlib:
    @just stdlib-docs
    @just stdlib-coverage
    @just verify-generated-extension-surfaces
    @just stdlib-registry-drift
    @just diff-stdlib
    @just diff-streams
    @just diff-json-pcre-date
    @just diff-spl-reflection

# Correctness-focused performance gates. Sub-gates share one engine build
# through the perf-build dependency (deduplicated within this invocation).
# perf-flag-matrix runs via performance-regression; the release-profile and
# report gates live in verify-performance-extended.
verify-performance: wordpress-benchmark-self-test performance-tests performance-regression benchmark-smoke framework-smoke acceleration-matrix fastest-engine-matrix default-profile-smoke managed-fast-coverage fast-preset-smoke app-flow-smoke baseline-native-stencil-smoke copy-patch-stencil-smoke mid-tier-plan-smoke cache-roundtrip optimizer-diff bytecode-layout-smoke superinstruction-smoke templates-smoke quickening-smoke inline-cache-smoke inline-cache-lookup-benchmark-gate jit-smoke safety-audit-smoke
    @printf '%s\n' '[pass] performance verification complete'

# Heavy release-profile and report gates, split out of verify-performance so
# the serial local gate fits pre-push budgets; CI runs both gates in its
# parallel matrix. Depends on benchmark-smoke for the hotpath report inputs.
verify-performance-extended: benchmark-smoke release-benchmark-smoke callgrind-smoke hotpath-inventory fastest-hotpath-report perf-report
    @printf '%s\n' '[pass] extended performance verification complete'

verify-phpt:
    #!/usr/bin/env bash
    set -euo pipefail
    just known-gaps
    scripts/phpt/verify_foundation.sh
    just phpt-verify-baseline
    scripts/phpt/verify_source_integrity.sh
    # See verify-server: combined runs already ran this crate's suite.
    if [[ "${PHRUST_COMBINED_RUN:-0}" == "1" ]]; then
      printf '%s\n' '[skip] php_phpt_tools suite covered by the combined workspace tests'
    else
      cargo test -p php_phpt_tools
    fi

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

stdlib-registry-drift:
    scripts/stdlib/registry_drift.py

generate-arginfo php_src="third_party/php-src" out="crates/php_std/src/generated/arginfo.rs":
    scripts/stdlib/generate_arginfo.py --php-src "{{php_src}}" --overrides fixtures/stdlib/arginfo_overrides.txt --out "{{out}}"
    rustfmt --edition 2024 "{{out}}"

verify-generated-arginfo:
    scripts/stdlib/verify_generated_arginfo.sh

generate-extension-surfaces:
    scripts/stdlib/generate_extension_surfaces.py --schema-dir fixtures/stdlib/extensions --arginfo crates/php_std/src/generated/arginfo.rs --out-root .
    cargo fmt --all

verify-generated-extension-surfaces:
    scripts/stdlib/test_generate_extension_surfaces.py
    scripts/stdlib/verify_generated_extension_surfaces.sh

oracle-api-index:
    cargo build -q -p php_std --bin dump_stdlib_registry
    scripts/oracle/api_index.py --self-test

oracle-api-summary:
    @if [[ ! -f target/oracle/api/php-source-api-summary.md ]]; then \
      just oracle-api-index >/dev/null; \
    fi
    scripts/oracle/api_index.py --summary-only

oracle-probe-generate:
    @if [[ ! -f target/oracle/api/php-source-api-symbols.jsonl ]]; then \
      just oracle-api-index >/dev/null; \
    fi
    scripts/oracle/generate_probes.py --self-test

oracle-probe-smoke:
    @just oracle-probe-generate
    cargo build -q -p php_vm_cli --bin php-vm
    scripts/runtime_semantics_diff.py --dir fixtures/runtime_semantics/oracle_generated/smoke --out target/oracle/probes/smoke --rust-vm ${CARGO_TARGET_DIR:-target}/debug/php-vm

oracle-probe-full:
    @just oracle-probe-generate
    cargo build -q -p php_vm_cli --bin php-vm
    scripts/runtime_semantics_diff.py --dir fixtures/runtime_semantics/oracle_generated --out target/oracle/probes/full --rust-vm ${CARGO_TARGET_DIR:-target}/debug/php-vm

oracle-gap-report *args:
    scripts/oracle/gap_report.py --self-test {{args}}

oracle-next-gap-prompt *args:
    @if [[ ! -f target/oracle/gap-report.json ]]; then \
      just oracle-gap-report --check >/dev/null; \
    fi
    scripts/oracle/next_gap_prompt.py --self-test {{args}}

oracle-smoke:
    @just oracle-api-index
    @reference="${REFERENCE_PHP:-}"; \
    if [[ -z "${reference}" && -x third_party/php-src/sapi/cli/php ]]; then \
      reference="third_party/php-src/sapi/cli/php"; \
    fi; \
    if [[ -n "${reference}" ]]; then \
      REFERENCE_PHP="${reference}" just oracle-probe-smoke; \
    else \
      printf '%s\n' '[skip] REFERENCE_PHP unavailable; oracle probe smoke requires a reference PHP binary.'; \
    fi
    scripts/oracle/gap_report.py --cheap --check --fail-on-unclassified

verify-oracle:
    @just oracle-api-index
    @reference="${REFERENCE_PHP:-}"; \
    if [[ -z "${reference}" && -x third_party/php-src/sapi/cli/php ]]; then \
      reference="third_party/php-src/sapi/cli/php"; \
    fi; \
    if [[ -n "${reference}" ]]; then \
      REFERENCE_PHP="${reference}" just oracle-probe-full; \
    else \
      printf '%s\n' '[skip] REFERENCE_PHP unavailable; strict oracle probes require a reference PHP binary.'; \
    fi
    scripts/oracle/gap_report.py --cheap --check --fail-on-unclassified

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

bytecode-layout-smoke: perf-build
    scripts/performance/bytecode_layout_smoke.sh

superinstruction-smoke: perf-build
    scripts/performance/superinstruction_smoke.sh

superinstruction-patterns:
    cargo build -p php_vm_cli --bin php-vm
    scripts/performance/superinstruction_patterns.py --summary-doc target/performance/superinstructions/summary.md

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
    test -s docs/runtime/known-gaps.md
    grep -q 'E_PHP_RUNTIME_UNSUPPORTED_REFERENCE_SEMANTICS' docs/runtime/known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_GENERATOR' docs/runtime/known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_YIELD_FROM' docs/runtime/known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_FIBER' docs/runtime/known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_EVAL' docs/runtime/known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_AUTOLOAD' docs/runtime/known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_REFLECTION' docs/runtime/known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_TRAIT_RUNTIME' docs/runtime/known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_ENUM_RUNTIME' docs/runtime/known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_PROPERTY_HOOKS' docs/runtime/known-gaps.md
    grep -q 'E_PHP_IR_UNSUPPORTED_BY_REF_FOREACH' docs/runtime/known-gaps.md
    grep -q 'E_PHP_RUNTIME_SUPERGLOBALS_FULL_MATRIX' docs/runtime/known-gaps.md
    grep -q 'E_PHP_RUNTIME_GLOBALS_ALIAS_MATRIX' docs/runtime/known-gaps.md
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
    #!/usr/bin/env bash
    set -euo pipefail
    for tool in cargo rustc rustfmt cargo-clippy just jq python3 rg clang sccache; do
      if ! command -v "$tool" >/dev/null 2>&1; then
        printf '%s\n' "[missing] required runtime semantics devshell tool: $tool" >&2
        exit 1
      fi
    done
    if ! command -v shellcheck >/dev/null 2>&1; then
      case "$(uname -s)" in
        Darwin) printf '%s\n' '[skip] shellcheck unavailable; Darwin devshell omits it to avoid the Haskell closure';;
        *) printf '%s\n' '[missing] required runtime semantics devshell tool: shellcheck' >&2; exit 1;;
      esac
    fi
    test "${PHP_REF_SERIES:-}" = "8.5"
    test "${PHP_REF_VERSION:-}" = "8.5.7"
    test "${PHP_REF_TAG:-}" = "php-8.5.7"
    test -n "${CARGO_TARGET_DIR:-}"
    test -n "${SCCACHE_DIR:-}"
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
    cargo miri test -p php_runtime runtime_memory::tests
    cargo miri test -p php_vm frame_memory::tests

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

wordpress-real-perf-report:
    cargo build -p php_server --bin phrust-server
    PHRUST_SERVER="${PHRUST_SERVER:-${CARGO_TARGET_DIR:-target}/debug/phrust-server}" scripts/wordpress/real_perf_report.py

wordpress-root-profile:
    cargo build -p php_server --bin phrust-server
    PHRUST_SERVER="${PHRUST_SERVER:-${CARGO_TARGET_DIR:-target}/debug/phrust-server}" scripts/wordpress/root_profile.py

# External, instrumentation-free ARM64 CPU sampling of one pinned PHP worker.
wordpress-arm64-sample *args:
    scripts/performance/arm64_work_accounting.py {{args}}

# Exclusive, single-owner accounting over the latest external ARM64 sample run.
wordpress-arm64-accounting-gate *args:
    scripts/performance/arm64_stack_classifier.py --gate {{args}}

wordpress-benchmark-self-test:
    scripts/performance/wordpress_root_benchmark.py --self-test

# Build the pinned stock PHP-FPM 8.5.7 + mysqli + OPcache reference image.
wordpress-reference-image:
    docker build --build-arg PHP_VERSION=8.5.7 --tag phrust-php-fpm:8.5.7 docker/performance/php-fpm
    docker pull nginx:1.28.0-alpine

# Clean timing: release Phrust and stock PHP-FPM/OPcache. This never enables
# request profiles, VM counters, or trace collection.
wordpress-root-benchmark *args:
    if [ -z "${PHRUST_WORDPRESS_PHRUST_URL:-${PHRUST_WORDPRESS_URL:-}}" ]; then cargo build --release -p php_server --bin phrust-server --no-default-features --features jit-copy-patch; fi
    PHRUST_SERVER="${PHRUST_SERVER:-${CARGO_TARGET_DIR:-target}/release/phrust-server}" scripts/performance/wordpress_root_benchmark.py --mode clean {{args}}

# Explicit experimental-JIT arm for the clean WordPress A/B matrix. The
# default benchmark remains copy-patch-only and is the control arm.
wordpress-root-benchmark-cranelift *args:
    if [ -z "${PHRUST_WORDPRESS_PHRUST_URL:-${PHRUST_WORDPRESS_URL:-}}" ]; then cargo build --release -p php_server --bin phrust-server --no-default-features --features jit-copy-patch,jit-cranelift; fi
    PHRUST_SERVER="${PHRUST_SERVER:-${CARGO_TARGET_DIR:-target}/release/phrust-server}" scripts/performance/wordpress_root_benchmark.py --mode clean --engine-preset experimental-jit {{args}}

# Isolated persistent-feedback A/B using the same lean binary and benchmark
# contract. Both arms and their joint ratio report share one result directory.
wordpress-root-benchmark-feedback-ab *args:
    if [ -z "${PHRUST_WORDPRESS_PHRUST_URL:-${PHRUST_WORDPRESS_URL:-}}" ]; then cargo build --release -p php_server --bin phrust-server --no-default-features --features jit-copy-patch; fi
    PHRUST_SERVER="${PHRUST_SERVER:-${CARGO_TARGET_DIR:-target}/release/phrust-server}" scripts/performance/wordpress_root_benchmark.py --mode clean --feedback-ab {{args}}

# Instrumented Phrust-only attribution. Its samples are marked timing-ineligible.
wordpress-root-diagnostics *args:
    if [ -z "${PHRUST_WORDPRESS_PHRUST_URL:-${PHRUST_WORDPRESS_URL:-}}" ]; then cargo build --release -p php_server --bin phrust-server; fi
    PHRUST_SERVER="${PHRUST_SERVER:-${CARGO_TARGET_DIR:-target}/release/phrust-server}" scripts/performance/wordpress_root_benchmark.py --mode diagnostic {{args}}

# Strict regression gate: compare both engines and a recorded Phrust baseline;
# missing WordPress or reference PHP is a failure.
wordpress-root-regression-gate *args:
    if [ -z "${PHRUST_WORDPRESS_PHRUST_URL:-${PHRUST_WORDPRESS_URL:-}}" ]; then cargo build --release -p php_server --bin phrust-server --no-default-features --features jit-copy-patch; fi
    PHRUST_SERVER="${PHRUST_SERVER:-${CARGO_TARGET_DIR:-target}/release/phrust-server}" scripts/performance/wordpress_root_benchmark.py --mode clean --strict --compare "${PHRUST_WORDPRESS_ROOT_BASELINE_JSON:-target/performance/wordpress-root/baseline.json}" {{args}}

# Prompt-pack tranche acceptance: the leading result is the warm,
# instrumentation-free WordPress concurrency-1 p50. The ordinary regression
# recipe remains a no-regression CI guard and does not require a speedup.
wordpress-root-tranche-gate baseline *args:
    if [ -z "${PHRUST_WORDPRESS_PHRUST_URL:-${PHRUST_WORDPRESS_URL:-}}" ]; then cargo build --release -p php_server --bin phrust-server --no-default-features --features jit-copy-patch; fi
    PHRUST_SERVER="${PHRUST_SERVER:-${CARGO_TARGET_DIR:-target}/release/phrust-server}" scripts/performance/wordpress_root_benchmark.py --mode clean --strict --baseline "{{baseline}}" --min-c1-p50-improvement-pct 3 {{args}}

# Anti-theater guard: fail performance branches that only add docs, reports,
# counters, or metric renames without production Rust changes or gates.
perf-pr-guard *args:
    scripts/verify/perf_pr_guard.py {{args}}

# Worker-local adaptive state: direct quickening/IC reuse must preserve the
# kill switch, compiled-generation isolation, and PHP-visible request isolation.
worker-adaptive-state-smoke:
    cargo test -p php_executor worker_ -- --nocapture

# Profiler containment: unprofiled requests after a profiled request must stay
# within 5% of clean unprofiled requests in the same server process.
profiler-overhead-gate:
    if [ -z "${PHRUST_WORDPRESS_URL:-}" ]; then cargo build -p php_server --bin phrust-server; fi
    PHRUST_SERVER="${PHRUST_SERVER:-${CARGO_TARGET_DIR:-target}/debug/phrust-server}" scripts/performance/profiler_overhead_gate.py

wordpress-dense-fallback-report:
    scripts/performance/dense_fallback_report.py

wordpress-clone-churn-report:
    scripts/performance/clone_churn_report.py

wordpress-array-hotpath-report:
    scripts/performance/array_hotpath_report.py

wordpress-call-hotpath-report:
    scripts/performance/call_hotpath_report.py

wordpress-persistent-metadata-report:
    scripts/performance/persistent_metadata_report.py

wordpress-native-region-report:
    scripts/performance/native_region_report.py

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

# Shared debug engine build for the performance gates. Declared as a recipe
# dependency so one `just verify-performance` invocation builds once instead
# of re-invoking cargo per sub-gate.
perf-build:
    cargo build -p php_vm_cli --bin php-vm

performance-tests:
    scripts/performance/compare_perf_json.py --self-test
    scripts/performance/hotpath_inventory.py --self-test
    scripts/performance/fastest_hotpath_report.py --self-test
    scripts/performance/bench_matrix.py --self-test
    scripts/performance/perf_report.py --self-test
    scripts/performance/app_flow_matrix.py --self-test
    scripts/performance/array_hotpath_report.py --self-test
    scripts/performance/call_hotpath_report.py --self-test
    scripts/performance/clone_churn_report.py --self-test
    scripts/performance/dense_fallback_report.py --self-test
    scripts/performance/front_controller_hotpath_smoke.py --self-test
    scripts/performance/native_region_report.py --self-test
    scripts/performance/persistent_metadata_report.py --self-test
    scripts/performance/wordpress_root_benchmark.py --self-test
    scripts/wordpress/root_profile.py --self-test
    scripts/performance/decision_baseline.py --self-test
    scripts/performance/startup_matrix.py --self-test

performance-regression: perf-build
    scripts/performance_regression_smoke.sh
    scripts/performance/regression_smoke.sh
    @just perf-flag-matrix
    @just polymorphic-inline-cache-smoke

perf-flag-matrix:
    scripts/performance/perf_flag_matrix.py

default-profile-smoke: perf-build
    scripts/performance/default_profile_smoke.py

managed-fast-coverage: perf-build
    scripts/performance/managed_fast_coverage.py

fast-preset-smoke: perf-build
    scripts/performance/fast_preset_smoke.py

baseline-native-stencil-smoke: perf-build
    scripts/performance/baseline_native_stencil_smoke.py

copy-patch-stencil-smoke: perf-build
    scripts/performance/copy_patch_stencil_smoke.py

# Differential check that the native copy-patch tier matches the interpreter and
# the pinned PHP 8.5.7 reference. Builds php-vm with the feature so native
# actually engages (aarch64); the harness SKIPs cleanly on unsupported hosts.
copy-patch-native-diff:
    cargo build -p php_vm_cli --bin php-vm --features jit-copy-patch
    scripts/performance/copy_patch_native_diff.py

mid-tier-plan-smoke: perf-build
    scripts/performance/mid_tier_plan_smoke.py

ir-verify:
    cargo test -p php_ir verify --lib

benchmark-smoke: perf-build
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

# Production binaries with telemetry recorders compiled out (runtime
# request-profiling and layout counters are unavailable in these builds;
# use the default release build for diagnosis). Measured on microbenches:
# property reads ~11% faster, concatenation ~5%.
release-lean:
    cargo build --release -p php_server --no-default-features --features jit-copy-patch
    cargo build --release -p php_vm_cli --bin php-vm --no-default-features --features jit-copy-patch

release-benchmark-smoke:
    scripts/performance/release_profiles.py release

pgo-benchmark-smoke:
    scripts/performance/release_profiles.py pgo

bolt-benchmark-smoke:
    scripts/performance/release_profiles.py bolt

framework-smoke: perf-build
    scripts/performance/framework_micro_smoke.py

front-controller-hotpath-smoke:
    cargo build -p php_server --bin phrust-server
    scripts/performance/front_controller_hotpath_smoke.py --server "${CARGO_TARGET_DIR:-target}/debug/phrust-server" --out target/performance/front-controller-hotpath/report.json

app-flow-smoke: perf-build
    scripts/performance/app_flow_matrix.py --smoke --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm" --timeout "${PHRUST_APP_FLOW_TIMEOUT:-30.0}"

app-flow-matrix:
    cargo build -p php_vm_cli --bin php-vm
    cargo build --release -p php_vm_cli --bin php-vm
    scripts/performance/app_flow_matrix.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm" --release-engine "${CARGO_TARGET_DIR:-target}/release/php-vm" --iterations "${PHRUST_APP_FLOW_ITERATIONS:-5}" --warmups "${PHRUST_APP_FLOW_WARMUPS:-1}" --scale "${PHRUST_APP_FLOW_SCALE:-2}" --timeout "${PHRUST_APP_FLOW_TIMEOUT:-30.0}"

# Runtime-layout tranche gate: focused fast-path tests, app-flow smoke, and
# the counter-existence/ratchet checker. Counter regressions are reported by
# default; PHRUST_RATCHET_ENFORCE=1 makes them hard failures.
runtime-layout-performance-smoke:
    cargo build -p php_vm_cli --bin php-vm
    cargo test -p php_runtime string_intrinsics
    cargo test -p php_runtime json_fast
    cargo test -p php_runtime array_intrinsics
    cargo test -p php_vm dense_bytecode
    cargo test -p php_vm superinstruction
    @just app-flow-smoke
    scripts/performance/runtime_layout_smoke.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm"

perf-ratchet-prereq:
    cargo build -p php_vm_cli --bin php-vm
    scripts/performance/ratchet_prereq.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm"

cli-speed-ratchet-smoke:
    cargo build -p php_vm_cli --bin php-vm
    scripts/performance/cli_speed_suite.py --smoke --out target/performance/ratchet/cli/smoke.json --markdown-out target/performance/ratchet/cli/smoke.md

cli-speed-ratchet:
    cargo build -p php_vm_cli --bin php-vm
    scripts/performance/cli_speed_suite.py --out target/performance/ratchet/cli/current.json --markdown-out target/performance/ratchet/cli/current.md

app-flow-ratchet-smoke:
    cargo build -p php_vm_cli --bin php-vm
    scripts/performance/app_flow_matrix.py --smoke --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm" --timeout "${PHRUST_APP_FLOW_TIMEOUT:-30.0}" --ratchet-out target/performance/ratchet/app-flow/smoke.json --ratchet-markdown-out target/performance/ratchet/app-flow/smoke.md

app-flow-ratchet:
    cargo build -p php_vm_cli --bin php-vm
    scripts/performance/app_flow_matrix.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm" --iterations "${PHRUST_RATCHET_ITERATIONS:-10}" --warmups "${PHRUST_RATCHET_WARMUPS:-3}" --scale "${PHRUST_RATCHET_SCALE:-2}" --timeout "${PHRUST_APP_FLOW_TIMEOUT:-30.0}" --allow-missing-reference --ratchet-out target/performance/ratchet/app-flow/current.json --ratchet-markdown-out target/performance/ratchet/app-flow/current.md

server-responsiveness-ratchet-smoke:
    cargo build -p php_server --bin phrust-server
    cargo build -p php_vm_cli --bin php-vm
    scripts/performance/server_responsiveness.py --smoke --server "${CARGO_TARGET_DIR:-target}/debug/phrust-server" --out target/performance/ratchet/server/smoke.json --markdown-out target/performance/ratchet/server/smoke.md

server-responsiveness-ratchet:
    cargo build --release -p php_server --bin phrust-server
    cargo build -p php_vm_cli --bin php-vm
    scripts/performance/server_responsiveness.py --server "${CARGO_TARGET_DIR:-target}/release/phrust-server" --out target/performance/ratchet/server/current.json --markdown-out target/performance/ratchet/server/current.md

counter-ratchet:
    scripts/performance/counter_ratchet.py --benchmark target/performance/benchmark-smoke.json --app-flow target/performance/ratchet/app-flow/current.json --server target/performance/ratchet/server/current.json --out target/performance/ratchet/counters/current.json --markdown-out target/performance/ratchet/counters/current.md

perf-ratchet-smoke:
    @just perf-ratchet-prereq
    @just cli-speed-ratchet-smoke
    @just app-flow-ratchet-smoke
    @just server-responsiveness-ratchet-smoke
    scripts/performance/counter_ratchet.py --benchmark target/performance/benchmark-smoke.json --app-flow target/performance/ratchet/app-flow/smoke.json --server target/performance/ratchet/server/smoke.json --out target/performance/ratchet/counters/smoke.json --markdown-out target/performance/ratchet/counters/smoke.md
    scripts/performance/ratchet_schema.py --validate target/performance/ratchet/cli/smoke.json target/performance/ratchet/app-flow/smoke.json target/performance/ratchet/server/smoke.json target/performance/ratchet/counters/smoke.json

perf-ratchet-current:
    @just cli-speed-ratchet
    @just app-flow-ratchet
    @just server-responsiveness-ratchet
    @just counter-ratchet
    scripts/performance/perf_ratchet.py combine --run-id perf-ratchet-current --out target/performance/ratchet/current.json --markdown-out target/performance/ratchet/current.md

perf-ratchet-baseline:
    @just cli-speed-ratchet
    @just app-flow-ratchet
    @just server-responsiveness-ratchet
    @just counter-ratchet
    scripts/performance/perf_ratchet.py combine --run-id perf-ratchet-baseline --out target/performance/ratchet/baseline.json --markdown-out target/performance/ratchet/baseline.md

perf-ratchet-compare:
    scripts/performance/ratchet_compare.py target/performance/ratchet/baseline.json target/performance/ratchet/current.json --out target/performance/ratchet/compare.md --json-out target/performance/ratchet/compare.json

perf-ratchet-report:
    scripts/performance/perf_ratchet.py report --run-id perf-ratchet-report --out target/performance/ratchet/report.json --markdown-out target/performance/ratchet/report.md

perf-ratchet-next-prompt:
    scripts/performance/ratchet_next_prompt.py --ratchet target/performance/ratchet/cli/current.json --ratchet target/performance/ratchet/app-flow/current.json --ratchet target/performance/ratchet/server/current.json --compare target/performance/ratchet/compare.json --out target/performance/ratchet/next-performance-prompt.md

perf-ratchet-accept-local:
    scripts/performance/perf_ratchet.py accept-local

acceleration-matrix: perf-build
    scripts/performance/acceleration_matrix.py

fastest-engine-matrix: perf-build
    scripts/performance/fastest_engine_matrix.py

hotpath-inventory:
    scripts/performance/hotpath_inventory.py target/performance/benchmark-smoke.json --json-out target/performance/hotpaths.json --markdown-out target/performance/hotpath-inventory.md

fastest-hotpath-report: perf-build
    @if [ ! -f target/performance/benchmark-smoke.json ]; then \
        scripts/performance/bench_matrix.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm" --out target/performance/benchmark-smoke.json --repetitions "${PHRUST_PERF_BENCH_SMOKE_REPETITIONS:-1}" --warmups "${PHRUST_PERF_BENCH_SMOKE_WARMUPS:-0}" --timeout "${PHRUST_PERF_BENCH_TIMEOUT:-10.0}"; \
    fi
    scripts/performance/fastest_hotpath_report.py --benchmark target/performance/benchmark-smoke.json --framework target/performance/framework-smoke/summary.json --acceleration target/performance/acceleration/summary.json --json-out target/performance/fastest/hotpath-report.json --markdown-out target/performance/fastest/hotpath-report.md --summary-doc target/performance/fastest/hotpath-report.md

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

quickening-smoke: perf-build
    scripts/performance/quickening_smoke.sh

inline-cache-smoke: perf-build
    scripts/performance/inline_cache_smoke.sh

inline-cache-lookup-benchmark-gate:
    scripts/performance/inline_cache_lookup_gate.py --self-test
    cargo bench --manifest-path crates/php_bench/Cargo.toml --bench perf_hotpaths -- inline_cache_function_hit
    scripts/performance/inline_cache_lookup_gate.py

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
    @if rg -n '\bunsafe\b' crates/php_jit/src --glob '!lib.rs' --glob '!helpers.rs' --glob '!cranelift_lowering.rs' --glob '!code_memory.rs'; then \
        printf '%s\n' '[fail] performance default JIT surface contains unaudited Rust unsafe' >&2; \
        exit 1; \
    fi
    @test -f docs/performance/cranelift/safety-audit.md
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

perf-decision-baseline:
    scripts/performance/decision_baseline.py --engine "${CARGO_TARGET_DIR:-target}/debug/php-vm" --smoke

startup-matrix:
    cargo build -p php_vm_cli --bin php-vm
    cargo build --release -p php_vm_cli --bin php-vm
    scripts/performance/startup_matrix.py --debug-engine "${CARGO_TARGET_DIR:-target}/debug/php-vm" --release-engine "${CARGO_TARGET_DIR:-target}/release/php-vm"
