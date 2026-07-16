#!/usr/bin/env bash
set -euo pipefail

mode="${1:-}"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
target_dir="${CARGO_TARGET_DIR:-target}"
engine="${repo_root}/${target_dir}/debug/php-vm"
out_dir="${repo_root}/${target_dir}/performance/profiles"

usage() {
    printf '%s\n' 'usage: scripts/performance/profile_smoke.sh dispatch|arrays|calls|composer'
}

case "${mode}" in
    dispatch)
        fixture="tests/fixtures/performance/perf_smoke/loops.php"
        label="vm-dispatch"
        ;;
    arrays)
        fixture="tests/fixtures/performance/perf_smoke/arrays_mixed.php"
        label="array-heavy"
        ;;
    calls)
        fixture="tests/fixtures/performance/perf_smoke/function_calls.php"
        label="call-heavy"
        ;;
    composer)
        fixture="tests/fixtures/stdlib/corpus/container_autoload.php"
        label="composer-smoke"
        ;;
    -h|--help|help|'')
        usage
        exit 0
        ;;
    *)
        usage >&2
        exit 2
        ;;
esac

fixture_path="${repo_root}/${fixture}"
if [ ! -f "${fixture_path}" ]; then
    printf '[skip] fixture missing for %s: %s\n' "${label}" "${fixture}"
    exit 0
fi

mkdir -p "${out_dir}"

if [ "${PHRUST_PERF_PROFILE_RUN:-0}" != "1" ]; then
    printf '[skip] dry-run profiling recipe for %s. Set PHRUST_PERF_PROFILE_RUN=1 to run a local profiler.\n' "${label}"
else
    if [ ! -x "${engine}" ]; then
        printf '[fail] debug VM binary missing: %s\n' "${engine}" >&2
        printf '%s\n' '       run `nix develop -c cargo build -p php_vm_cli --bin php-vm` first' >&2
        exit 1
    fi
fi

printf 'fixture: %s\n' "${fixture}"
printf 'vm: %s\n' "${target_dir}/debug/php-vm"
printf 'output directory: %s\n' "${target_dir}/performance/profiles"

if command -v cargo-flamegraph >/dev/null 2>&1; then
    flamegraph_bin="cargo-flamegraph"
elif cargo flamegraph --help >/dev/null 2>&1; then
    flamegraph_bin="cargo flamegraph"
else
    flamegraph_bin=""
fi

if [ -n "${flamegraph_bin}" ]; then
    printf 'cargo flamegraph command: %s -p php_vm_cli --bin php-vm -o %s/%s-flamegraph.svg -- run %s --engine-preset=default --counters=on\n' \
        "${flamegraph_bin}" "${target_dir}/performance/profiles" "${mode}" "${fixture}"
    if [ "${PHRUST_PERF_PROFILE_RUN:-0}" = "1" ]; then
        if [ "${flamegraph_bin}" = "cargo-flamegraph" ]; then
            cargo-flamegraph -p php_vm_cli --bin php-vm -o "${out_dir}/${mode}-flamegraph.svg" -- run "${fixture_path}" --engine-preset=default --counters=on
        else
            cargo flamegraph -p php_vm_cli --bin php-vm -o "${out_dir}/${mode}-flamegraph.svg" -- run "${fixture_path}" --engine-preset=default --counters=on
        fi
    fi
else
    printf '[skip] cargo flamegraph is not installed.\n'
fi

if command -v perf >/dev/null 2>&1; then
    printf 'linux perf command: perf record -g -o %s/%s.perf.data -- %s run %s --engine-preset=default --counters=on\n' \
        "${target_dir}/performance/profiles" "${mode}" "${target_dir}/debug/php-vm" "${fixture}"
    if [ "${PHRUST_PERF_PROFILE_RUN:-0}" = "1" ]; then
        perf record -g -o "${out_dir}/${mode}.perf.data" -- "${engine}" run "${fixture_path}" --engine-preset=default --counters=on
    fi
else
    printf '[skip] linux perf is not installed.\n'
fi

if command -v xctrace >/dev/null 2>&1; then
    printf 'macOS instruments command: xctrace record --template "Time Profiler" --output %s/%s.trace -- %s run %s --engine-preset=default --counters=on\n' \
        "${target_dir}/performance/profiles" "${mode}" "${target_dir}/debug/php-vm" "${fixture}"
    if [ "${PHRUST_PERF_PROFILE_RUN:-0}" = "1" ]; then
        xctrace record --template "Time Profiler" --output "${out_dir}/${mode}.trace" -- "${engine}" run "${fixture_path}" --engine-preset=default --counters=on
    fi
else
    printf '[skip] macOS xctrace is not installed.\n'
fi

if command -v dtrace >/dev/null 2>&1; then
    printf 'macOS dtrace availability: installed; use with local privileges and write outputs under %s.\n' "${target_dir}/performance/profiles"
else
    printf '[skip] dtrace is not installed or not visible in PATH.\n'
fi
