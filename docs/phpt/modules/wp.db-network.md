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

## Prompt 3.1 Harness

This module owns selected DB/network fixtures for WordPress. The mysqli surface
has a capability-gated MVP backed by a real Rust MySQL client. cURL exposes a
local HTTP MVP behind an explicit network-test gate. OpenSSL exposes selected
digest, random-byte, method-listing, and verification-gap helpers.

Selected fixtures:

- `tests/phpt/generated/wp.db-network/mysqli-platform-mvp.phpt`
- `tests/phpt/generated/wp.db-network/mysqli-default-off.phpt`
- `tests/phpt/generated/wp.db-network/mysqli-object-wpdb-mvp.phpt`
- `tests/phpt/generated/wp.db-network/mysqli-live-query-dsn.phpt`
- `tests/phpt/generated/wp.db-network/mysqli-object-live-wpdb-dsn.phpt`
- `tests/phpt/generated/wp.db-network/curl-platform-mvp.phpt`
- `tests/phpt/generated/wp.db-network/curl-default-off.phpt`
- `tests/phpt/generated/wp.db-network/curl-local-http.phpt`
- `tests/phpt/generated/wp.db-network/openssl-platform-mvp.phpt`
- `tests/phpt/generated/wp.db-network/openssl-helpers-mvp.phpt`

The MySQL live fixture uses `PHRUST_MYSQL_TEST_DSN` as the branch-wide external
service switch. Without that variable it must skip cleanly. Non-live mysqli
fixtures prove the module is visible and that connection attempts do not fake
success when the DSN gate is closed. References without native mysqli skip the
mysqli fixtures cleanly.

The cURL live fixture uses `PHRUST_NET_TESTS=1` and `PHRUST_CURL_TEST_URL` as
the explicit network switch. The URL must point at a local test server; the
runtime MVP rejects non-loopback hosts and does not use public internet tests.

Prepared statements are intentionally not implemented in this slice. The
selected WordPress-style fixtures exercise `mysqli_query` plus escaping and do
not require `mysqli_stmt_*`; `mysqli_prepare` remains an explicit unsupported
diagnostic until a selected fixture needs a real DB-backed statement path.

## Target Gates

- `nix develop -c cargo test -p php_phpt_tools`
- `nix develop -c cargo test -p php_runtime`
- `PHRUST_NET_TESTS=1 nix develop -c cargo test -p php_runtime curl`
- `nix develop -c just phpt-dev-module MODULE=wp.db-network`
- `nix develop -c just quality-fast`

## Next Step

Run closeout gates and keep live MySQL/cURL PHPTs gated by explicit local
environment variables.
