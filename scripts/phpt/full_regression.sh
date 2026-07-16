#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
source "$script_dir/common.sh"

php_src="${PHP_SRC_DIR:-}"
if [[ -z "$php_src" ]]; then
  if [[ -d third_party/php-src-8.5.7 ]]; then
    php_src="third_party/php-src-8.5.7"
  else
    php_src="third_party/php-src"
  fi
fi

corpus="${PHPT_CORPUS_MANIFEST:-tests/phpt/manifests/phpt-corpus.jsonl}"
known_failures="${PHPT_KNOWN_FAILURES:-tests/phpt/manifests/full-known-failures.jsonl}"
baseline_metadata="${PHPT_BASELINE_METADATA:-tests/phpt/manifests/full-baseline-metadata.json}"
module_counts="${PHPT_BASELINE_MODULE_COUNTS:-tests/phpt/manifests/full-baseline-module-counts.jsonl}"
known_gap_catalog="${PHPT_KNOWN_GAP_CATALOG:-tests/phpt/manifests/known-gap-catalog.jsonl}"
report="${PHPT_BASELINE_REPORT:-target/phpt-work/reports/full-baseline.md}"
work_root="${PHPT_WORK_DIR:-target/phpt-work}"
timestamp="${PHPT_BASELINE_TIMESTAMP:-$(date -u +%Y%m%dT%H%M%SZ)}"
run_dir="$work_root/full-runs/$timestamp"
default_phpt_tool="${CARGO_TARGET_DIR:-target}/debug/php-phpt-tools"
phpt_tool="${PHPT_TOOLS_BIN:-$default_phpt_tool}"

if [[ "${PHPT_RUN_FULL:-0}" != "1" ]]; then
  printf '%s\n' 'Refusing to start the full PHPT corpus without PHPT_RUN_FULL=1.' >&2
  printf '%s\n' 'This gate can run 20k+ PHPTs. Use focused targets while iterating:' >&2
  printf '%s\n' '  just phpt-fast MODULE=<module> [FILE=<path>|PATTERN=<text>]' >&2
  printf '%s\n' '  just phpt-dev-module MODULE=<module>' >&2
  printf '%s\n' '  just phpt-rerun-failures MODULE=<module>' >&2
  printf '%s\n' 'For an intentional full gate, run:' >&2
  printf '%s\n' '  PHPT_RUN_FULL=1 just phpt-full-regression' >&2
  exit 2
fi

target_php="${TARGET_PHP:-target/debug/phrust-php}"
target_mode="${PHPT_TARGET_MODE:-}"
if [[ -z "$target_mode" ]]; then
  if [[ "$(basename "$target_php")" == "php-vm" ]]; then
    target_mode="php-vm"
  else
    target_mode="php-cli"
  fi
fi

needs_default_tool=0
needs_default_target=0
if [[ -z "${PHPT_TOOLS_BIN:-}" && "$phpt_tool" == "$default_phpt_tool" ]]; then
  needs_default_tool=1
fi
if [[ -z "${TARGET_PHP:-}" && "$target_php" == "target/debug/phrust-php" ]]; then
  needs_default_target=1
fi

if [[ -n "${PHPT_SKIP_BUILD:-}" ]]; then
  if [[ ! -x "$phpt_tool" ]]; then
    printf 'PHPT tools executable is not built: %s\n' "$phpt_tool" >&2
    printf '%s\n' 'Run: just phpt-dev-build' >&2
    exit 1
  fi
elif [[ "$needs_default_tool" -eq 1 && "$needs_default_target" -eq 1 ]]; then
  cargo build -p php_phpt_tools --bin php-phpt-tools -p php_vm_cli --bin phrust-php
  needs_default_tool=0
  needs_default_target=0
elif [[ "$needs_default_tool" -eq 1 ]]; then
  cargo build -p php_phpt_tools --bin php-phpt-tools
elif [[ ! -x "$phpt_tool" ]]; then
  printf 'PHPT tools executable is not built: %s\n' "$phpt_tool" >&2
  exit 1
fi

if [[ ! -s "$corpus" ]]; then
  printf '%s\n' '[info] PHPT corpus manifest is missing; generating it first.'
  "$phpt_tool" phpt-index --php-src "$php_src"
fi

if [[ -n "${PHPT_SKIP_BUILD:-}" ]]; then
  if [[ ! -x "$target_php" ]]; then
    printf 'Target PHP executable is not built: %s\n' "$target_php" >&2
    printf '%s\n' 'Run: just phpt-dev-build' >&2
    exit 1
  fi
elif [[ "$needs_default_target" -eq 1 ]]; then
  cargo build -p php_vm_cli --bin phrust-php
elif [[ ! -x "$target_php" ]]; then
  printf 'Target PHP executable is not built: %s\n' "$target_php" >&2
  exit 1
fi

mkdir -p "$run_dir"

previous_results="${PHPT_PREVIOUS_RESULTS:-}"
if [[ -n "$previous_results" && ! -s "$previous_results" ]]; then
  printf 'Explicit PHPT_PREVIOUS_RESULTS does not exist or is empty: %s\n' "$previous_results" >&2
  exit 1
fi
if [[ -z "$previous_results" && "${PHPT_DISABLE_REUSE:-0}" != "1" ]]; then
  previous_results="$(
    find "$work_root/full-runs" -mindepth 2 -maxdepth 2 -name results.jsonl -type f \
      ! -path "$run_dir/results.jsonl" \
      | sort \
      | tail -n 1
  )"
  if [[ -n "$previous_results" ]]; then
    printf 'PHPT_PREVIOUS_RESULTS=%s\n' "$previous_results"
  fi
fi

previous_args=()
if [[ -s "$known_failures" && "${PHPT_ACCEPT_BASELINE:-0}" != "1" ]]; then
  cp "$known_failures" "$run_dir/previous-known-failures.jsonl"
  previous_args=(--previous-known-failures "$run_dir/previous-known-failures.jsonl")
  if [[ -n "$previous_results" ]]; then
    previous_args+=(--previous-results "$previous_results")
  fi
fi

reuse_args=()
if [[ -n "$previous_results" ]]; then
  reuse_args=(--reuse-results "$previous_results")
fi

phpt_jobs="$(phpt_default_jobs)"
job_args=(--jobs "$phpt_jobs")

dev_reuse_args=()
if [[ -n "${PHPT_DEV_REUSE_PASS:-}" && "${PHPT_DEV_REUSE_PASS:-}" != "0" ]]; then
  dev_reuse_args=(--dev-reuse-pass)
fi

cleanup_args=()
if [[ "${PHPT_KEEP_WORK:-0}" != "1" ]]; then
  cleanup_args=(--cleanup-work)
fi

printf 'TARGET_PHP=%s\n' "$target_php"
printf 'PHPT_TARGET_MODE=%s\n' "$target_mode"
printf 'PHPT_CORPUS_MANIFEST=%s\n' "$corpus"
printf 'PHPT_RUN_DIR=%s\n' "$run_dir"
printf 'PHPT_JOBS=%s\n' "$phpt_jobs"
printf 'PHPT_REUSE_RESULTS=%s\n' "${reuse_args[*]:-disabled}"
printf 'PHPT_DEV_REUSE_PASS=%s\n' "${PHPT_DEV_REUSE_PASS:-0}"
printf 'PHPT_KEEP_WORK=%s\n' "${PHPT_KEEP_WORK:-0}"

set +e
"$phpt_tool" run \
  --target "$target_php" \
  --target-mode "$target_mode" \
  --manifest "$corpus" \
  --out "$run_dir/results.jsonl" \
  --summary "$run_dir/summary.md" \
  --php-src "$php_src" \
  --work-dir "$run_dir/work" \
  --timeout-seconds "${PHPT_TIMEOUT_SECONDS:-30}" \
  ${reuse_args[@]+"${reuse_args[@]}"} \
  ${dev_reuse_args[@]+"${dev_reuse_args[@]}"} \
  ${cleanup_args[@]+"${cleanup_args[@]}"} \
  ${job_args[@]+"${job_args[@]}"}
run_status=$?
set -e

if [[ "$run_status" -gt 1 ]]; then
  printf 'full PHPT runner failed before producing a comparable result: status %s\n' "$run_status" >&2
  exit "$run_status"
fi

"$phpt_tool" baseline \
  --results "$run_dir/results.jsonl" \
  --corpus "$corpus" \
  --known-failures "$known_failures" \
  --metadata "$baseline_metadata" \
  --module-counts "$module_counts" \
  --report "$report" \
  --timestamp "$timestamp" \
  ${previous_args[@]+"${previous_args[@]}"}

if [[ -n "$previous_results" ]]; then
  python3 "$script_dir/result_delta.py" \
    --baseline "$previous_results" \
    --current "$run_dir/results.jsonl" \
    --out "$run_dir/result-delta.json" \
    --regression-manifest "$run_dir/regression-manifest.jsonl"
fi

"$phpt_tool" triage \
  --corpus "$corpus" \
  --known-failures "$known_failures" \
  --metadata "$baseline_metadata" \
  --module-counts "$module_counts" \
  --known-gap-catalog "$known_gap_catalog"

"$phpt_tool" verify-baseline \
  --corpus "$corpus" \
  --known-failures "$known_failures" \
  --metadata "$baseline_metadata" \
  --module-counts "$module_counts" \
  --known-gap-catalog "$known_gap_catalog" \
  --report "$report"

scripts/phpt/verify_source_integrity.sh

printf '[ok] full PHPT regression baseline artifacts: %s\n' "$run_dir"
printf '[ok] known failures: %s\n' "$known_failures"
printf '[ok] baseline metadata: %s\n' "$baseline_metadata"
printf '[ok] baseline module counts: %s\n' "$module_counts"
printf '[ok] known-gap catalog: %s\n' "$known_gap_catalog"
printf '[ok] baseline report: %s\n' "$report"
