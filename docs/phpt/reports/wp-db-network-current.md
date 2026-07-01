# WordPress DB/Network Current Report

## Implementation Status

The repository includes a focused `wp.db-network` PHPT module for WordPress
database and network extension work. The mysqli/mysqlnd policy has moved from
negative probes to a capability-gated mysqli MVP backed by a real Rust MySQL
client layer. cURL and OpenSSL now expose narrow MVP helpers instead of
negative platform probes.

## Selected Fixtures

| Fixture | Purpose | Expected default outcome |
| --- | --- | --- |
| `mysqli-platform-mvp.phpt` | Prove `mysqli` functions/classes/constants are visible once the real MVP is registered. | PASS on target; SKIP on references without native mysqli |
| `feature-detection-env-gates.phpt` | Prove `mysqli`, `curl`, and `openssl` feature-detection probes stay visible before and after capability env vars are set. | PASS on target; SKIP on references without required native extensions |
| `mysqli-default-off.phpt` | Prove live `mysqli_connect()` does not fake success without `PHRUST_MYSQL_TEST_DSN`. | PASS on target; SKIP on references without native mysqli |
| `mysqli-object-wpdb-mvp.phpt` | Cover WordPress-style `mysqli_init()`/`real_connect()` object flow under the default-off gate. | PASS on target; SKIP on references without native mysqli |
| `mysqli-live-query-dsn.phpt` | Establish `PHRUST_MYSQL_TEST_DSN` as the live query switch. | SKIP without DSN |
| `mysqli-object-live-wpdb-dsn.phpt` | Cover DSN-gated WordPress-style object connect, `utf8mb4`, escaped insert, query, `$result->num_rows`, and fetch flow. | SKIP without DSN |
| `mysqli-prepared-basic-dsn.phpt` | Cover DSN-gated prepared insert and result readback. | SKIP without DSN |
| `mysqli-prepared-reexecute-dsn.phpt` | Cover repeated execute reading current bound parameter values. | SKIP without DSN |
| `mysqli-prepared-bind-result-dsn.phpt` | Cover `mysqli_stmt_bind_result()` and `mysqli_stmt_fetch()` assignment. | SKIP without DSN |
| `mysqli-prepared-error-dsn.phpt` | Cover statement prepare failure errno/error/sqlstate. | SKIP without DSN |
| `curl-platform-mvp.phpt` | Prove cURL MVP functions, class, and constants are visible. | PASS on target; SKIP on references without native curl |
| `curl-default-off.phpt` | Prove cURL execution does not fake network success without `PHRUST_NET_TESTS=1`. | PASS on target; SKIP on references without native curl |
| `curl-local-http.phpt` | Establish `PHRUST_NET_TESTS=1` plus `PHRUST_CURL_TEST_URL` as the live local HTTP gate. | SKIP without net URL |
| `curl-wordpress-http-options.phpt` | Cover WordPress HTTP option combinations through loopback cURL. | SKIP without net URL |
| `curl-header-and-status.phpt` | Cover headers, status info, reset, and `CURLOPT_NOBODY`. | SKIP without net URL |
| `openssl-platform-mvp.phpt` | Prove selected OpenSSL helper symbols are visible. | PASS on target; SKIP on references without native openssl |
| `openssl-helpers-mvp.phpt` | Cover digest, random bytes, method listing, and explicit verify-gap behavior. | PASS on target; SKIP on references without native openssl |

## Current Failures

No default run opens a MySQL socket. If `PHRUST_MYSQL_TEST_DSN` is set, live
fixtures run both `SELECT 1 AS one` and a WordPress-style object flow through
the Rust `mysql` client, including `utf8mb4`, escaped insert, buffered
`mysqli_result`, `$result->num_rows`, field counts, field metadata, and
associative fetch behavior. References without native mysqli skip cleanly.

No default run opens a cURL socket. `curl_exec()` returns `false` with a
non-empty error unless `PHRUST_NET_TESTS=1` is set. Unit coverage uses a local
in-process HTTP server; PHPT live HTTP coverage additionally requires
`PHRUST_CURL_TEST_URL` to point at a local endpoint.

DB/network false-return paths also record first-cause structured runtime
diagnostics without printing secrets or changing PHP-visible output. MySQLi
connection/query/prepare/bind/execute failures use stable IDs including
`E_PHP_MYSQLI_CAPABILITY_DISABLED`, `E_PHP_MYSQLI_CONNECTION_FAILED`,
`E_PHP_MYSQLI_QUERY_FAILED`, `E_PHP_MYSQLI_PREPARE_FAILED`,
`E_PHP_MYSQLI_STMT_BIND_FAILED`, and
`E_PHP_MYSQLI_STMT_EXECUTE_FAILED`. cURL network and request failures use
`E_PHP_CURL_CAPABILITY_DISABLED`, `E_PHP_CURL_REQUEST_FAILED`, and
`E_PHP_CURL_OPTION_UNSUPPORTED`. The payload is mapped into the existing
`WordPressBringup` runtime diagnostic envelope with `db_network` context fields:
`diagnostic_id`, `function_name`, `operation`, `capability_state`,
`mysqli_report_flags`, `dsn_present_boolean`, `host`, `port`,
`database_name_if_nonsecret`, `mysql_error_code`, `mysql_sqlstate`, and
`mysql_error_message`. `mysqli_report()` flags are request-local and report
error/strict modes raise the structured diagnostic severity to recoverable while
preserving the selected false-return behavior.

## Closeout Report

Summary report: `docs/phpt/reports/wp-db-network-summary.md`.

## DSN and Network Test Strategy

- `PHRUST_MYSQL_TEST_DSN=mysql://user:pass@127.0.0.1:3306/db` enables live
  MySQL/MariaDB tests.
- The DSN must never be committed.
- Default non-DSN runs skip live database tests.
- HTTP/cURL tests use local in-process servers in Rust unit tests and
  `PHRUST_NET_TESTS=1`; public internet tests are outside this scope.
- `PHRUST_CURL_TEST_URL` is required for the optional live cURL PHPT and must
  point at loopback.

## Prepared Statement MVP

The selected module now includes DSN-gated coverage for procedural and object
statement handles. Implemented statement APIs include `mysqli_prepare`,
`mysqli_stmt_init`, `mysqli_stmt_prepare`, `mysqli_stmt_bind_param`,
`mysqli_stmt_execute`, `mysqli_stmt_get_result`, `mysqli_stmt_bind_result`,
`mysqli_stmt_fetch`, status accessors, `mysqli_stmt_free_result`, and
`mysqli_stmt_close`. Bound parameter references are read at execute time so
repeated executes observe updated variable values.

## cURL MVP

Implemented functions: `curl_version`, `curl_init`, `curl_setopt`,
`curl_setopt_array`, `curl_exec`, `curl_error`, `curl_errno`, `curl_getinfo`,
`curl_reset`, `curl_copy_handle`, `curl_multi_init`, `curl_multi_add_handle`,
`curl_multi_exec`, `curl_multi_close`, and `curl_close`.

Implemented options and info values cover WordPress remote request smoke:
`CURLOPT_URL`, `CURLOPT_RETURNTRANSFER`, `CURLOPT_TIMEOUT`,
`CURLOPT_TIMEOUT_MS`, `CURLOPT_FOLLOWLOCATION`, `CURLOPT_HTTPHEADER`,
`CURLOPT_HEADER`, `CURLOPT_NOBODY`, `CURLOPT_USERAGENT`, `CURLOPT_REFERER`,
`CURLOPT_ENCODING`, `CURLOPT_HTTP_VERSION`, `CURLOPT_CONNECTTIMEOUT`,
`CURLOPT_CONNECTTIMEOUT_MS`, `CURLOPT_MAXREDIRS`, `CURLOPT_FAILONERROR`,
`CURLOPT_POST`, `CURLOPT_POSTFIELDS`, `CURLOPT_CUSTOMREQUEST`,
`CURLOPT_SSL_VERIFYPEER`, `CURLOPT_SSL_VERIFYHOST`,
`CURLINFO_RESPONSE_CODE`, `CURLINFO_HTTP_CODE`, `CURLINFO_EFFECTIVE_URL`, and
`CURLINFO_TOTAL_TIME`.

Network capability policy: `curl_exec` requires `PHRUST_NET_TESTS=1` and the
runtime only permits loopback `http://` hosts. HTTPS, proxy/auth, streaming
callbacks, file uploads, and HTTP/2 remain gaps. The multi interface is a
deterministic single-request compatibility shell for WordPress feature probes.

## OpenSSL MVP

Implemented functions: `openssl_random_pseudo_bytes`, `openssl_digest`,
`openssl_get_md_methods`, `openssl_pkey_get_public`,
`openssl_get_publickey`, `openssl_error_string`, and `openssl_verify`.

Digest methods: `md5`, `sha1`, `sha224`, `sha256`, `sha384`, and `sha512`.
`openssl_verify` is present but returns `-1` as an explicit verification gap
until a real key parser and signature verifier are introduced. Certificate
parsing, key generation, PKCS#12, encrypt/decrypt APIs, and full OpenSSL parity
remain gaps.

## Feature Status

- Capability-gated MySQL/MariaDB connection layer is implemented. The selected
  dependency is `mysql = 28` with default features disabled and `minimal-rust`
  enabled. It is MIT/Apache-2.0 licensed, implemented in Rust, and used only
  through `crates/php_runtime/src/db/mysql.rs`.
- Procedural connection/query/fetch/error/escape/close MVP is implemented.
- DSN-gated object `mysqli`/`mysqli_result` query and fetch shape used by
  WordPress `wpdb` is implemented, including escaped insert and
  `$result->num_rows`.
- Prepared statements are implemented for the selected DSN-gated WordPress
  statement MVP; broad mysqlnd internals and full mysqli corpus parity remain
  gaps.
- Local loopback HTTP GET/POST MVP is available behind `PHRUST_NET_TESTS=1`.
- First-cause DB/network diagnostics are implemented for the selected MySQLi
  and cURL false-return paths and deliberately redact DSNs/passwords.
- Selected digest/random/method helpers are implemented with an explicit
  verification gap.
- Closeout reporting and non-network baseline gates are recorded.
