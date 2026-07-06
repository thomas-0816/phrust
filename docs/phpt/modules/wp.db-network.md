# wp.db-network

- Strategy: WordPress database and network extension harness
- Classification: required-framework
- Selected manifest: `tests/phpt/manifests/modules/wp.db-network.selected.jsonl`

## Scope

- `mysqli`/mysqlnd policy reversal through real implementation work
- DSN-gated MySQL/MariaDB fixtures for future live database tests
- cURL transport probes for WordPress remote request support
- OpenSSL helper probes for HTTPS, update, and security flows

## Non-Scope

- SAPI, FPM, CGI, Apache, phpdbg, or webserver process changes
- Fake successful database or network behavior
- Shelling out to `mysql`, `curl`, or `openssl` binaries
- Public internet dependencies in tests
- Committing local DSNs or secrets

## Harness

This module owns selected DB/network fixtures for WordPress. The mysqli surface
has a capability-gated MVP backed by a real Rust MySQL client. cURL exposes a
local HTTP MVP behind an explicit network-test gate. OpenSSL exposes selected
digest, random-byte, method-listing, and verification-gap helpers.

Selected fixtures:

- `tests/phpt/generated/wp.db-network/mysqli-platform-mvp.phpt`
- `tests/phpt/generated/wp.db-network/feature-detection-env-gates.phpt`
- `tests/phpt/generated/wp.db-network/mysqli-default-off.phpt`
- `tests/phpt/generated/wp.db-network/mysqli-object-wpdb-mvp.phpt`
- `tests/phpt/generated/wp.db-network/mysqli-live-query-dsn.phpt`
- `tests/phpt/generated/wp.db-network/mysqli-object-live-wpdb-dsn.phpt`
- `tests/phpt/generated/wp.db-network/mysqli-prepared-basic-dsn.phpt`
- `tests/phpt/generated/wp.db-network/mysqli-prepared-reexecute-dsn.phpt`
- `tests/phpt/generated/wp.db-network/mysqli-prepared-bind-result-dsn.phpt`
- `tests/phpt/generated/wp.db-network/mysqli-prepared-error-dsn.phpt`
- `tests/phpt/generated/wp.db-network/curl-platform-mvp.phpt`
- `tests/phpt/generated/wp.db-network/curl-default-off.phpt`
- `tests/phpt/generated/wp.db-network/curl-local-http.phpt`
- `tests/phpt/generated/wp.db-network/curl-wordpress-http-options.phpt`
- `tests/phpt/generated/wp.db-network/curl-header-and-status.phpt`
- `tests/phpt/generated/wp.db-network/openssl-platform-mvp.phpt`
- `tests/phpt/generated/wp.db-network/openssl-helpers-mvp.phpt`

The MySQL live fixture uses `PHRUST_MYSQL_TEST_DSN` as the module-wide external
service switch. Without that variable it must skip cleanly. Non-live mysqli
fixtures prove the module is visible and that connection attempts do not fake
success when the DSN gate is closed. References without native mysqli skip the
mysqli fixtures cleanly.

Feature-detection coverage verifies that WordPress-style extension, function,
and class probes for `mysqli`, `curl`, and `openssl` remain visible before and
after the DB/network capability environment variables are set. That coverage
does not open sockets.

The cURL live fixture uses `PHRUST_NET_TESTS=1` and `PHRUST_CURL_TEST_URL` as
the explicit network switch. The URL must point at a local test server; the
runtime MVP rejects non-loopback hosts and does not use public internet tests.

Prepared statements are part of the selected DSN-gated scope. The module covers
`mysqli_prepare`, `mysqli_stmt_init`, `mysqli_stmt_prepare`,
`mysqli_stmt_bind_param`, `mysqli_stmt_execute`, `mysqli_stmt_get_result`,
`mysqli_stmt_bind_result`, `mysqli_stmt_fetch`, statement status accessors, and
statement close/free-result behavior without claiming broad mysqlnd parity.

Failure paths are expected to be first-cause friendly. MySQLi and cURL selected
false-return paths record structured `db_network` runtime diagnostics with
stable IDs, function/operation names, capability state, DSN presence, safe
host/port/database metadata, MySQL-style error code/state/message fields, and no
committed DSNs or passwords. These diagnostics are auxiliary to PHP-visible
behavior; selected userland calls still return `false` where PHP expects a
non-throwing failure path.

## Target Gates

- `nix develop -c cargo test -p php_phpt_tools`
- `nix develop -c cargo test -p php_runtime`
- `PHRUST_NET_TESTS=1 nix develop -c cargo test -p php_runtime curl`
- `nix develop -c just phpt-dev-module MODULE=wp.db-network`
- `nix develop -c just quality-fast`

## Next Step

Run validation gates and keep live MySQL/cURL PHPTs gated by explicit local
environment variables.
