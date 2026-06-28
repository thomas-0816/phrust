# phpt.runner

- Priority: 2
- Selected manifest: `tests/phpt/manifests/modules/phpt.runner.selected.jsonl`
- Last focused run: 2026-06-28
- Current selected-gate counts: 3 PASS, 197 SKIP, 0 FAIL, 0 BORK from 200 selected cases

## Scope

- PHPT section handling
- expectation matching
- runner BORK reduction
- safe local `FILE_EXTERNAL` and `EXPECT*_EXTERNAL` materialization
- lossy non-UTF8 PHPT source classification
- explicit target capability skips before execution

## Non-Scope

- VM feature implementation
- SAPI implementation
- phpdbg behavior
- CGI request emulation
- terminal-dependent stdio capture behavior

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
- `TARGET_PHP=target/debug/phrust-php PHPT_TARGET_MODE=php-cli nix develop -c just phpt-dev-module MODULE=phpt.runner`

## Known Gaps

- malformed or non-UTF8 PHPT sources remain tracked as runner gaps in triage
- unsupported sections and unsupported expectation forms remain tracked by BORK subclass
- SAPI/phpdbg/CGI and terminal-dependent stdio tests skip with concrete target-mode reasons
- `ext/standard/tests/hrtime/hrtime.phpt` skips under `php-cli` target mode because the flaky busy loop exceeds the current VM step limit

## Next Step

Keep reducing runner-owned BORK subclasses while routing SAPI, target VM, and
terminal-dependent behavior to their owning modules.
