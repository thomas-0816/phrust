# phpt.runner

- Priority: 2
- Selected manifest: `tests/phpt/manifests/modules/phpt.runner.selected.jsonl`
- Current counts: 0 PASS, 0 SKIP, 0 FAIL, 437 BORK from 0 corpus candidates

## Scope

- PHPT section handling
- expectation matching
- runner BORK reduction

## Non-Scope

- VM feature implementation

## Relevant PHPT Paths

- `tests/output/stream_isatty_out.phpt`
- `tests/output/stream_isatty_out-err.phpt`
- `tests/output/stream_isatty_in-out.phpt`
- `tests/output/stream_isatty_in-out-err.phpt`
- `tests/output/stream_isatty_in-err.phpt`
- `tests/output/stream_isatty_err.phpt`
- `tests/output/sapi_windows_vt100_support_winok_out.phpt`
- `tests/output/sapi_windows_vt100_support_winok_out-err.phpt`
- `tests/output/sapi_windows_vt100_support_winok_in-out.phpt`
- `tests/output/sapi_windows_vt100_support_winok_in-out-err.phpt`
- `tests/output/sapi_windows_vt100_support_winok_in-err.phpt`
- `tests/output/sapi_windows_vt100_support_winok_err.phpt`
- `tests/output/sapi_windows_vt100_support_winko_out.phpt`
- `tests/output/sapi_windows_vt100_support_winko_out-err.phpt`
- `tests/output/sapi_windows_vt100_support_winko_in-out.phpt`
- `tests/output/sapi_windows_vt100_support_winko_in-out-err.phpt`
- `tests/output/sapi_windows_vt100_support_winko_in-err.phpt`
- `tests/output/sapi_windows_vt100_support_winko_err.phpt`
- `tests/output/ob_018.phpt`
- `tests/output/bug74725.phpt`
- `tests/basic/bug71273.phpt`
- `tests/basic/029.phpt`
- `tests/basic/022.phpt`
- `tests/basic/011_empty_query.phpt`
- `sapi/phpdbg/tests/watch_007.phpt`
- `sapi/phpdbg/tests/watch_006.phpt`
- `sapi/phpdbg/tests/watch_005.phpt`
- `sapi/phpdbg/tests/watch_004.phpt`
- `sapi/phpdbg/tests/watch_003.phpt`
- `sapi/phpdbg/tests/watch_002.phpt`
- `sapi/phpdbg/tests/watch_001.phpt`
- `sapi/phpdbg/tests/stepping_001.phpt`
- `sapi/phpdbg/tests/stdin_001.phpt`
- `sapi/phpdbg/tests/set_exception_handler.phpt`
- `sapi/phpdbg/tests/run_002.phpt`
- `sapi/phpdbg/tests/run_001.phpt`
- `sapi/phpdbg/tests/register_function_leak.phpt`
- `sapi/phpdbg/tests/register_function.phpt`
- `sapi/phpdbg/tests/print_002.phpt`
- `sapi/phpdbg/tests/print_001.phpt`

## Relevant php-src Source Areas

- `crates/php_phpt_tools/src/`
- `scripts/phpt/`

## Target Gates

- `nix develop -c just phpt-runner-smoke`

## Known Gaps

- `needs-triage`: 320
- `runtime-unsupported-feature`: 135

## Next Step

Reduce runner-owned BORKs before attributing failures to the engine.
