#!/usr/bin/env bash
set -euo pipefail

module="${MODULE:-}"
while [[ $# -gt 0 ]]; do
  case "$1" in
    MODULE=*)
      module="${1#MODULE=}"
      shift
      ;;
    --module)
      module="${2:-}"
      shift 2
      ;;
    --module=*)
      module="${1#--module=}"
      shift
      ;;
    *)
      printf 'unknown phpt-module argument: %s\n' "$1" >&2
      exit 2
      ;;
  esac
done

if [[ -z "$module" ]]; then
  printf '%s\n' 'MODULE is required, for example: just phpt-module MODULE=zend.basic' >&2
  exit 2
fi

safe_module="$(printf '%s' "$module" | tr '[:upper:]' '[:lower:]' | sed -E 's/[^a-z0-9._-]+/-/g; s/^-+//; s/-+$//')"
selected_manifest="tests/phpt/manifests/modules/${safe_module}.selected.jsonl"
generated_manifest="tests/phpt/manifests/${safe_module}-generated.jsonl"
manifest="${PHPT_MANIFEST:-$generated_manifest}"

if [[ -z "${PHPT_MANIFEST:-}" && -s "$selected_manifest" ]]; then
  manifest="$selected_manifest"
fi

if [[ -n "${PHPT_MANIFEST:-}" && ! -s "$manifest" ]]; then
  printf 'PHPT_MANIFEST does not exist or is empty: %s\n' "$manifest" >&2
  exit 1
fi

if [[ ! -s "$manifest" ]]; then
  scripts/phpt/generate_module.sh "MODULE=$module"
fi

php_src="${PHP_SRC_DIR:-}"
if [[ -z "$php_src" ]]; then
  if [[ -d third_party/php-src-8.5.7 ]]; then
    php_src="third_party/php-src-8.5.7"
  else
    php_src="third_party/php-src"
  fi
fi

reference_php="${REFERENCE_PHP:-$php_src/sapi/cli/php}"
if [[ ! -x "$reference_php" ]]; then
  printf 'Reference PHP CLI is not built: %s\n' "$reference_php" >&2
  printf '%s\n' 'Run: nix develop -c just build-ref-php' >&2
  exit 1
fi

target_php="${TARGET_PHP:-target/debug/phrust-php}"
default_phpt_tool="${CARGO_TARGET_DIR:-target}/debug/php-phpt-tools"
phpt_tool="${PHPT_TOOLS_BIN:-$default_phpt_tool}"
target_mode="${PHPT_TARGET_MODE:-}"
if [[ -z "$target_mode" ]]; then
  if [[ "$(basename "$target_php")" == "php-vm" ]]; then
    target_mode="php-vm"
  else
    target_mode="php-cli"
  fi
fi

if [[ -n "${PHPT_SKIP_BUILD:-}" ]]; then
  if [[ ! -x "$phpt_tool" ]]; then
    printf 'PHPT tools executable is not built: %s\n' "$phpt_tool" >&2
    printf '%s\n' 'Run: just phpt-dev-build' >&2
    exit 1
  fi
  if [[ ! -x "$target_php" ]]; then
    printf 'Target PHP executable is not built: %s\n' "$target_php" >&2
    printf '%s\n' 'Run: just phpt-dev-build' >&2
    exit 1
  fi
elif [[ -z "${PHPT_TOOLS_BIN:-}" && "$phpt_tool" == "$default_phpt_tool" && -z "${TARGET_PHP:-}" && "$target_php" == "target/debug/phrust-php" ]]; then
  cargo build -q -p php_phpt_tools --bin php-phpt-tools -p php_vm_cli --bin phrust-php
else
  if [[ -z "${PHPT_TOOLS_BIN:-}" && "$phpt_tool" == "$default_phpt_tool" ]]; then
    cargo build -q -p php_phpt_tools --bin php-phpt-tools
  elif [[ ! -x "$phpt_tool" ]]; then
    printf 'PHPT tools executable is not built: %s\n' "$phpt_tool" >&2
    exit 1
  fi
  if [[ -z "${TARGET_PHP:-}" && "$target_php" == "target/debug/phrust-php" ]]; then
    cargo build -q -p php_vm_cli --bin phrust-php
  elif [[ ! -x "$target_php" ]]; then
    printf 'Target PHP executable is not built: %s\n' "$target_php" >&2
    exit 1
  fi
fi

work_root="${PHPT_WORK_DIR:-target/phpt-work}"
reference_dir="$work_root/module-runs/${safe_module}/reference"
target_dir="$work_root/module-runs/${safe_module}/target"

job_args=()
if [[ -n "${PHPT_JOBS:-}" ]]; then
  job_args=(--jobs "$PHPT_JOBS")
fi

reference_reuse_args=()
if [[ "${PHPT_DISABLE_REFERENCE_REUSE:-0}" != "1" && -s "$reference_dir/results.jsonl" ]]; then
  reference_reuse_args=(--reuse-results "$reference_dir/results.jsonl")
fi

target_reuse_args=()
if [[ "${PHPT_REUSE_LAST:-1}" != "0" && -s "$target_dir/results.jsonl" ]]; then
  target_reuse_args=(--reuse-results "$target_dir/results.jsonl")
fi

"$phpt_tool" run \
  --target "$reference_php" \
  --target-mode php-cli \
  --manifest "$manifest" \
  --out "$reference_dir/results.jsonl" \
  --summary "$reference_dir/summary.md" \
  --php-src "$php_src" \
  --work-dir "$reference_dir/work" \
  --timeout-seconds "${PHPT_TIMEOUT_SECONDS:-10}" \
  ${reference_reuse_args[@]+"${reference_reuse_args[@]}"} \
  ${job_args[@]+"${job_args[@]}"}

set +e
"$phpt_tool" run \
  --target "$target_php" \
  --target-mode "$target_mode" \
  --manifest "$manifest" \
  --out "$target_dir/results.jsonl" \
  --summary "$target_dir/summary.md" \
  --php-src "$php_src" \
  --work-dir "$target_dir/work" \
  --timeout-seconds "${PHPT_TIMEOUT_SECONDS:-10}" \
  ${target_reuse_args[@]+"${target_reuse_args[@]}"} \
  ${job_args[@]+"${job_args[@]}"}
target_status=$?
set -e

if [[ "$target_status" -gt 1 ]]; then
  printf 'target module run failed before producing a report: status %s\n' "$target_status" >&2
  exit "$target_status"
fi

scripts/phpt/verify_source_integrity.sh

printf '[ok] module PHPT reports for %s\n' "$module"
printf '[ok] reference: %s\n' "$reference_dir"
printf '[ok] target: %s\n' "$target_dir"
