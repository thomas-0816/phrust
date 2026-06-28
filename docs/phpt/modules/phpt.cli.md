# phpt.cli

- Priority: 3
- Selected manifest: `tests/phpt/manifests/modules/phpt.cli.selected.jsonl`
- Last focused run: 2026-06-28
- Current selected-gate counts: 3 PASS, 200 SKIP, 0 FAIL, 0 BORK from 203 selected cases

## Scope

- target binary discovery
- PHP CLI compatible invocation
- argv/stdin/ini plumbing
- `phrust-php -v`
- `phrust-php -r 'code'`
- `phrust-php -n`
- repeated `-d key=value`
- script execution with argv
- STDIN
- `$argc`, `$argv`
- `$_SERVER['argc']`, `$_SERVER['argv']`
- exit-code mapping

## Non-Scope

- full SAPI emulation
- CGI/FPM behavior
- phpdbg
- Apache module behavior
- CLI built-in HTTP server
- process-control helpers used to spawn nested PHP commands
- process-title and stdio descriptor rebinding APIs
- CLI `--ini` introspection and `-R` line-processing modes
- unrelated runtime/frontend gaps required by some upstream `sapi/cli` tests

## Selected PHPT Coverage

The selected manifest starts with generated Prompt 1B contract fixtures:

- `tests/phpt/generated/phpt.cli/argv-argc-superglobals.phpt`
- `tests/phpt/generated/phpt.cli/ini-overrides.phpt`
- `tests/phpt/generated/phpt.cli/stdin.phpt`

The remaining selected upstream PHPTs are retained as explicit non-scope
coverage and skip with concrete reasons under `PHPT_TARGET_MODE=php-cli`.

## Relevant Upstream PHPT Paths

- `tests/basic/011_register_argc_argv_disabled.phpt`
- `sapi/phpdbg/tests/gh12962.phpt`
- `sapi/phpdbg/tests/bug73615.phpt`
- `sapi/fpm/tests/status-ping.phpt`
- `sapi/fpm/tests/status-listen.phpt`
- `sapi/fpm/tests/status-listen-expose-php-on.phpt`
- `sapi/fpm/tests/status-listen-expose-php-off.phpt`
- `sapi/fpm/tests/status-basic.phpt`
- `sapi/fpm/tests/socket-uds-too-long-filename-test.phpt`
- `sapi/fpm/tests/socket-uds-too-long-filename-start.phpt`
- `sapi/fpm/tests/socket-uds-numeric-ugid.phpt`
- `sapi/fpm/tests/socket-uds-numeric-ugid-nonroot.phpt`
- `sapi/fpm/tests/socket-uds-basic.phpt`
- `sapi/fpm/tests/socket-uds-acl.phpt`
- `sapi/fpm/tests/socket-ipv6-basic.phpt`
- `sapi/fpm/tests/socket-ipv6-any.phpt`
- `sapi/fpm/tests/socket-ipv4-fallback.phpt`
- `sapi/fpm/tests/socket-ipv4-basic.phpt`
- `sapi/fpm/tests/socket-ipv4-allowed-clients.phpt`
- `sapi/fpm/tests/socket-invalid-allowed-clients.phpt`
- `sapi/fpm/tests/socket-close-on-exec.phpt`
- `sapi/fpm/tests/setsofib.phpt`
- `sapi/fpm/tests/request_parse_body_urlencoded.phpt`
- `sapi/fpm/tests/request_parse_body_multipart.phpt`
- `sapi/fpm/tests/reload-uses-sigkill-as-last-measure.phpt`
- `sapi/fpm/tests/proc-user-not-set-when-root.phpt`
- `sapi/fpm/tests/proc-user-ignored.phpt`
- `sapi/fpm/tests/proc-no-start-server.phpt`
- `sapi/fpm/tests/proc-idle-timeout.phpt`
- `sapi/fpm/tests/pool-prefix.phpt`
- `sapi/fpm/tests/pool-apparmor-basic.phpt`
- `sapi/fpm/tests/pm-max-spawn-rate-run.phpt`
- `sapi/fpm/tests/pm-max-spawn-rate-config.phpt`
- `sapi/fpm/tests/php_admin_value-failure.phpt`
- `sapi/fpm/tests/php-admin-doc-root.phpt`
- `sapi/fpm/tests/opcache_enable_admin_value.phpt`
- `sapi/fpm/tests/main-version.phpt`
- `sapi/fpm/tests/main-global-prefix.phpt`
- `sapi/fpm/tests/log-suppress-output.phpt`
- `sapi/fpm/tests/log-suppress-output-request-body.phpt`

## Relevant php-src Source Areas

- `crates/php_vm_cli/`
- `scripts/phpt/`

## Target Gates

- `nix develop -c just phpt-target-smoke`
- `TARGET_PHP=target/debug/phrust-php PHPT_TARGET_MODE=php-cli nix develop -c just phpt-dev-module MODULE=phpt.cli`

## Known Gaps

- `FPM not available in php-cli target mode`: 141 selected skips
- `CLI built-in web server not available in php-cli target mode`: 39 selected skips
- `CLI process-control APIs not available in php-cli target mode`: 6 selected skips
- `process-control functions are outside the Prompt 1B CLI contract`: 5 selected skips
- `CLI stdio descriptor rebinding not available in php-cli target mode`: 3 selected skips
- `phpdbg not available in php-cli target mode`: 2 selected skips
- `CLI --ini introspection not available in php-cli target mode`: 1 selected skip
- `CLI -R line-processing mode not available in php-cli target mode`: 1 selected skip
- `include-path expression runtime gap outside the Prompt 1B CLI contract`: 1 selected skip
- `STDOUT default-parameter lowering is outside the Prompt 1B CLI contract`: 1 selected skip

## Next Step

Keep target invocation deterministic for upstream PHPT execution and route
non-scope SAPI, HTTP server, process-control, and unrelated runtime/frontend
gaps to their owning modules instead of measuring them as Prompt 1B CLI
contract failures.
