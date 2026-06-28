# WordPress DB/Network Summary

## Implementation Status

- `mysqli`: implemented a WordPress-oriented MVP backed by the Rust `mysql`
  client. The exposed surface covers connection initialization, DSN-gated live
  connect/query, result fetch modes, row/field counts, errors, escaping,
  DB-backed charset/select-db updates, close/free, and object `mysqli` /
  `mysqli_result` flows used by `wpdb`, including `$result->num_rows`.
- `mysqli` prepared statements: explicit gap. No selected WordPress-style
  fixture requires `mysqli_stmt_*` yet, so `mysqli_prepare` and
  `mysqli_stmt_init` report unsupported diagnostics instead of fake handles.
- `curl`: implemented an HTTP MVP with `curl_version`, `curl_init`,
  `curl_setopt`, `curl_exec`, `curl_error`, `curl_errno`, `curl_getinfo`, and
  `curl_close`. Network execution requires `PHRUST_NET_TESTS=1` and permits
  loopback `http://` hosts only.
- `openssl`: implemented selected helpers:
  `openssl_random_pseudo_bytes`, `openssl_digest`,
  `openssl_get_md_methods`, and `openssl_verify`. Digest/random/method helpers
  are real; `openssl_verify` returns `-1` as the explicit key-verification gap.

## Fixtures

The `wp.db-network` module selects ten fixtures:

- `mysqli-platform-mvp.phpt`
- `mysqli-default-off.phpt`
- `mysqli-object-wpdb-mvp.phpt`
- `mysqli-live-query-dsn.phpt`
- `mysqli-object-live-wpdb-dsn.phpt`
- `curl-platform-mvp.phpt`
- `curl-default-off.phpt`
- `curl-local-http.phpt`
- `openssl-platform-mvp.phpt`
- `openssl-helpers-mvp.phpt`

Default module runs open no MySQL or cURL sockets. Live MySQL coverage requires
`PHRUST_MYSQL_TEST_DSN`. Live cURL PHPT coverage requires
`PHRUST_NET_TESTS=1` and `PHRUST_CURL_TEST_URL` pointing at a local endpoint.
Rust cURL unit coverage uses an in-process loopback server.

## Merge Risks

- Full extension parity is not claimed. The broad `mysqli`, `curl`, and
  `openssl` PHPT corpora still contain many non-green cases tracked by existing
  extension policy and known-gap reports.
- cURL HTTPS transport is not implemented; SSL verification options are
  accepted but do not imply TLS support.
- OpenSSL signature verification, certificate parsing, key management, and
  encrypt/decrypt APIs remain gaps.
- MySQL live behavior depends on an explicitly configured local
  `PHRUST_MYSQL_TEST_DSN`; default runs prove capability gating, not database
  availability.

## Closeout Gates

Required non-network closeout gates run on this branch:

- `nix develop -c just fmt`: PASS
- `nix develop -c cargo test -p php_runtime`: PASS, 229 tests
- `nix develop -c cargo test -p php_vm`: PASS, 413 tests
- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src nix develop -c just verify-stdlib`: PASS
- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src nix develop -c just verify-phpt`: PASS
- `nix develop -c just quality-fast`: PASS
- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=wp.db-network`: PASS, 10 tests on reference and target
- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=mysqli`: PASS, 1 test on reference and target
- `PHRUST_NET_TESTS=1 nix develop -c cargo test -p php_runtime curl`: PASS, 2 filtered cURL tests

External-service gates skipped:

- `PHRUST_MYSQL_TEST_DSN=... nix develop -c just phpt-dev-module MODULE=mysqli`:
  skipped because `PHRUST_MYSQL_TEST_DSN` is not configured locally.
- `PHRUST_MYSQL_TEST_DSN=... nix develop -c just phpt-dev-module MODULE=wp.db-network`:
  skipped because `PHRUST_MYSQL_TEST_DSN` is not configured locally.
- `PHRUST_NET_TESTS=1 PHRUST_CURL_TEST_URL=... nix develop -c just phpt-dev-module MODULE=wp.db-network`:
  skipped because `PHRUST_CURL_TEST_URL` is not configured locally.
