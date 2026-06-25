# phpt.cli

- Priority: 3
- Selected manifest: `tests/phpt/manifests/modules/phpt.cli.selected.jsonl`
- Current counts: 3 PASS, 0 SKIP, 273 FAIL, 0 BORK from 350 corpus candidates

## Scope

- target binary discovery
- PHP CLI compatible invocation
- argv/stdin/ini plumbing

## Non-Scope

- full SAPI emulation
- CGI/FPM behavior

## Relevant PHPT Paths

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

## Known Gaps

- `runtime-unsupported-feature`: 217
- `runtime-error-or-diagnostic`: 42
- `runtime-output-mismatch`: 16

## Next Step

Keep target invocation deterministic for upstream PHPT execution.
