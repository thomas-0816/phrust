#!/usr/bin/env bash
set -euo pipefail

module="${MODULE:-}"
focus_file="${FILE:-}"
focus_pattern="${PATTERN:-}"
while [[ $# -gt 0 ]]; do
  case "$1" in
    MODULE=*)
      module="${1#MODULE=}"
      shift
      ;;
    FILE=*)
      focus_file="${1#FILE=}"
      shift
      ;;
    PATTERN=*)
      focus_pattern="${1#PATTERN=}"
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
    --file)
      focus_file="${2:-}"
      shift 2
      ;;
    --file=*)
      focus_file="${1#--file=}"
      shift
      ;;
    --pattern)
      focus_pattern="${2:-}"
      shift 2
      ;;
    --pattern=*)
      focus_pattern="${1#--pattern=}"
      shift
      ;;
    *)
      printf 'unknown phpt-module-target argument: %s\n' "$1" >&2
      exit 2
      ;;
  esac
done

if [[ -z "$module" ]]; then
  printf '%s\n' 'MODULE is required, for example: just phpt-module-target MODULE=standard.strings' >&2
  exit 2
fi

if [[ -n "$focus_file" && -n "$focus_pattern" ]]; then
  printf '%s\n' 'Use either FILE or PATTERN, not both.' >&2
  exit 2
fi

if [[ -n "${PHPT_REQUIRE_FOCUS:-}" && "${PHPT_REQUIRE_FOCUS:-}" != "0" && -z "$focus_file" && -z "$focus_pattern" ]]; then
  printf '%s\n' 'Focused PHPT run requires FILE=... or PATTERN=....' >&2
  printf '%s\n' 'Use just phpt-module-target MODULE=<module> for a full target-only module run.' >&2
  exit 2
fi

safe_module="$(printf '%s' "$module" | tr '[:upper:]' '[:lower:]' | sed -E 's/[^a-z0-9._-]+/-/g; s/^-+//; s/-+$//')"
selected_manifest="tests/phpt/manifests/modules/${safe_module}.selected.jsonl"
generated_manifest="tests/phpt/manifests/${safe_module}-generated.jsonl"
manifest="${PHPT_MANIFEST:-$generated_manifest}"
work_root="${PHPT_WORK_DIR:-target/phpt-work}"
module_dir="$work_root/module-runs/${safe_module}"
target_dir="$module_dir/${PHPT_RUN_LABEL:-target}"
module_target_dir="$module_dir/target"

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

if [[ -n "$focus_file" ]]; then
  focus_slug="$(printf 'file-%s' "$focus_file" | tr '[:upper:]' '[:lower:]' | sed -E 's/[^a-z0-9._-]+/-/g; s/^-+//; s/-+$//')"
  target_dir="$module_dir/focus/$focus_slug"
  mkdir -p "$target_dir"
  filtered_manifest="$target_dir/focused-file.jsonl"
  printf '{"path":"%s"}\n' "$focus_file" > "$filtered_manifest"
  manifest="$filtered_manifest"
  printf 'PHPT_FOCUS_FILE=%s\n' "$focus_file"
elif [[ -n "$focus_pattern" ]]; then
  focus_slug="$(printf 'pattern-%s' "$focus_pattern" | tr '[:upper:]' '[:lower:]' | sed -E 's/[^a-z0-9._-]+/-/g; s/^-+//; s/-+$//')"
  target_dir="$module_dir/focus/$focus_slug"
  mkdir -p "$target_dir"
  filtered_manifest="$target_dir/focused-pattern.jsonl"
  if ! grep -F "$focus_pattern" "$manifest" > "$filtered_manifest"; then
    printf 'No PHPT manifest entries matched PATTERN=%s in %s\n' "$focus_pattern" "$manifest" >&2
    exit 2
  fi
  manifest="$filtered_manifest"
  printf 'PHPT_FOCUS_PATTERN=%s\n' "$focus_pattern"
fi

php_src="${PHP_SRC_DIR:-}"
if [[ -z "$php_src" ]]; then
  if [[ -d third_party/php-src-8.5.7 ]]; then
    php_src="third_party/php-src-8.5.7"
  else
    php_src="third_party/php-src"
  fi
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

if [[ -z "${PHPT_SKIP_BUILD:-}" ]]; then
  if [[ -z "${PHPT_TOOLS_BIN:-}" && "$phpt_tool" == "$default_phpt_tool" ]]; then
    cargo build -q -p php_phpt_tools --bin php-phpt-tools
  fi
  if [[ -z "${TARGET_PHP:-}" && "$target_php" == "target/debug/phrust-php" ]]; then
    cargo build -q -p php_vm_cli --bin phrust-php
  fi
fi

if [[ ! -x "$phpt_tool" ]]; then
  printf 'PHPT tools executable is not built: %s\n' "$phpt_tool" >&2
  printf '%s\n' 'Run: cargo build -p php_phpt_tools --bin php-phpt-tools' >&2
  exit 1
fi

if [[ ! -x "$target_php" ]]; then
  printf 'Target PHP executable is not built: %s\n' "$target_php" >&2
  printf '%s\n' 'Run: cargo build -p php_vm_cli --bin phrust-php' >&2
  exit 1
fi

reuse_results=""
if [[ -n "${PHPT_REUSE_LAST:-}" && "${PHPT_REUSE_LAST:-}" != "0" ]]; then
  previous_results="$target_dir/results.jsonl"
  if [[ -s "$previous_results" ]]; then
    reuse_results="$previous_results"
  elif [[ "$target_dir" != "$module_target_dir" && -s "$module_target_dir/results.jsonl" ]]; then
    reuse_results="$module_target_dir/results.jsonl"
  else
    printf '[skip] PHPT_REUSE_LAST requested, but no previous target results exist: %s\n' "$previous_results" >&2
  fi
fi

dev_reuse_args=()
if [[ -n "${PHPT_DEV_REUSE_PASS:-}" && "${PHPT_DEV_REUSE_PASS:-}" != "0" ]]; then
  dev_reuse_args=(--dev-reuse-pass)
fi

job_args=(--jobs "${PHPT_JOBS:-1}")

set +e
if [[ -n "$reuse_results" ]]; then
  "$phpt_tool" run \
    --target "$target_php" \
    --target-mode "$target_mode" \
    --manifest "$manifest" \
    --out "$target_dir/results.jsonl" \
    --summary "$target_dir/summary.md" \
    --php-src "$php_src" \
    --work-dir "$target_dir/work" \
    --timeout-seconds "${PHPT_TIMEOUT_SECONDS:-10}" \
    --reuse-results "$reuse_results" \
    "${dev_reuse_args[@]}" \
    "${job_args[@]}"
  target_status=$?
else
  "$phpt_tool" run \
    --target "$target_php" \
    --target-mode "$target_mode" \
    --manifest "$manifest" \
    --out "$target_dir/results.jsonl" \
    --summary "$target_dir/summary.md" \
    --php-src "$php_src" \
    --work-dir "$target_dir/work" \
    --timeout-seconds "${PHPT_TIMEOUT_SECONDS:-10}" \
    "${dev_reuse_args[@]}" \
    "${job_args[@]}"
  target_status=$?
fi
set -e

if [[ "$target_status" -gt 1 ]]; then
  printf 'target module run failed before producing a report: status %s\n' "$target_status" >&2
  exit "$target_status"
fi

if [[ "$target_status" -eq 1 ]]; then
  printf 'target module run produced non-green outcomes; see %s\n' "$target_dir/summary.md" >&2
  exit 1
fi

printf '[ok] target-only module PHPT report for %s\n' "$module"
printf '[ok] target: %s\n' "$target_dir"
