# WordPress DB/Network Current Report

## Prompt 3.1-3.7 Status

The branch currently owns a focused `wp.db-network` PHPT module for WordPress
database and network extension work. The mysqli/mysqlnd policy has moved from
negative probes to a capability-gated mysqli MVP backed by a real Rust MySQL
client layer. cURL and OpenSSL now expose narrow MVP helpers instead of
negative platform probes.

## Selected Fixtures

| Fixture | Purpose | Expected default outcome |
| --- | --- | --- |
| `mysqli-platform-mvp.phpt` | Prove `mysqli` functions/classes/constants are visible once the real MVP is registered. | PASS on target; SKIP on references without native mysqli |
| `mysqli-default-off.phpt` | Prove live `mysqli_connect()` does not fake success without `PHRUST_MYSQL_TEST_DSN`. | PASS on target; SKIP on references without native mysqli |
| `mysqli-object-wpdb-mvp.phpt` | Cover WordPress-style `mysqli_init()`/`real_connect()` object flow under the default-off gate. | PASS on target; SKIP on references without native mysqli |
| `mysqli-live-query-dsn.phpt` | Establish `PHRUST_MYSQL_TEST_DSN` as the live query switch. | SKIP without DSN |
| `mysqli-object-live-wpdb-dsn.phpt` | Cover DSN-gated WordPress-style object connect, `utf8mb4`, escaped insert, query, `$result->num_rows`, and fetch flow. | SKIP without DSN |
| `curl-platform-mvp.phpt` | Prove cURL MVP functions, class, and constants are visible. | PASS on target; SKIP on references without native curl |
| `curl-default-off.phpt` | Prove cURL execution does not fake network success without `PHRUST_NET_TESTS=1`. | PASS on target; SKIP on references without native curl |
| `curl-local-http.phpt` | Establish `PHRUST_NET_TESTS=1` plus `PHRUST_CURL_TEST_URL` as the live local HTTP gate. | SKIP without net URL |
| `openssl-platform-mvp.phpt` | Prove selected OpenSSL helper symbols are visible. | PASS on target; SKIP on references without native openssl |
| `openssl-helpers-mvp.phpt` | Cover digest, random bytes, method listing, and explicit verify-gap behavior. | PASS on target; SKIP on references without native openssl |

## Current Failures

No default run opens a MySQL socket. If `PHRUST_MYSQL_TEST_DSN` is set, live
fixtures run both `SELECT 1 AS one` and a WordPress-style object flow through
the Rust `mysql` client, including `utf8mb4`, escaped insert, buffered
`mysqli_result`, `$result->num_rows`, and associative fetch behavior. References
without native mysqli skip cleanly.

No default run opens a cURL socket. `curl_exec()` returns `false` with a
non-empty error unless `PHRUST_NET_TESTS=1` is set. Unit coverage uses a local
in-process HTTP server; PHPT live HTTP coverage additionally requires
`PHRUST_CURL_TEST_URL` to point at a local endpoint.

## Closeout Report

Summary report: `docs/phpt/reports/wp-db-network-summary.md`.

## DSN and Network Test Strategy

- `PHRUST_MYSQL_TEST_DSN=mysql://user:pass@127.0.0.1:3306/db` enables live
  MySQL/MariaDB tests in later prompts.
- The DSN must never be committed.
- Default non-DSN runs skip live database tests.
- HTTP/cURL tests use local in-process servers in Rust unit tests and
  `PHRUST_NET_TESTS=1`; public internet tests are not part of the branch.
- `PHRUST_CURL_TEST_URL` is required for the optional live cURL PHPT and must
  point at loopback.

## Prepared Statement Decision

No selected WordPress-style fixture in this branch requires prepared
statements. This slice keeps `mysqli_prepare` and `mysqli_stmt_init` as explicit
unsupported diagnostics rather than adding a partial statement implementation
without fixture pressure. A future statement MVP should cover
`mysqli_stmt_bind_param`, `mysqli_stmt_execute`, `mysqli_stmt_get_result`,
`mysqli_stmt_fetch`, and statement close/error basics through DSN-gated live
fixtures.

## cURL MVP

Implemented functions: `curl_version`, `curl_init`, `curl_setopt`,
`curl_exec`, `curl_error`, `curl_errno`, `curl_getinfo`, and `curl_close`.

Implemented options and info values cover WordPress remote request smoke:
`CURLOPT_URL`, `CURLOPT_RETURNTRANSFER`, `CURLOPT_TIMEOUT`,
`CURLOPT_TIMEOUT_MS`, `CURLOPT_FOLLOWLOCATION`, `CURLOPT_HTTPHEADER`,
`CURLOPT_POST`, `CURLOPT_POSTFIELDS`, `CURLOPT_CUSTOMREQUEST`,
`CURLOPT_SSL_VERIFYPEER`, `CURLOPT_SSL_VERIFYHOST`,
`CURLINFO_RESPONSE_CODE`, `CURLINFO_HTTP_CODE`, `CURLINFO_EFFECTIVE_URL`, and
`CURLINFO_TOTAL_TIME`.

Network capability policy: `curl_exec` requires `PHRUST_NET_TESTS=1` and the
runtime only permits loopback `http://` hosts. HTTPS, proxy/auth, multi,
streaming callbacks, file uploads, and HTTP/2 remain gaps.

## OpenSSL MVP

Implemented functions: `openssl_random_pseudo_bytes`, `openssl_digest`,
`openssl_get_md_methods`, and `openssl_verify`.

Digest methods: `md5`, `sha1`, `sha224`, `sha256`, `sha384`, and `sha512`.
`openssl_verify` is present but returns `-1` as an explicit verification gap
until a real key parser and signature verifier are introduced. Certificate
parsing, key generation, PKCS#12, encrypt/decrypt APIs, and full OpenSSL parity
remain gaps.

## Remaining Work

- Prompt 3.2: complete. Capability-gated MySQL/MariaDB connection layer. The selected
  dependency is `mysql = 28` with default features disabled and `minimal-rust`
  enabled. It is MIT/Apache-2.0 licensed, implemented in Rust, and used only
  through `crates/php_runtime/src/db/mysql.rs`.
- Prompt 3.3: complete for procedural connection/query/fetch/error/escape/close
  MVP.
- Prompt 3.4: complete for DSN-gated object `mysqli`/`mysqli_result` query and
  fetch shape used by WordPress `wpdb`, including escaped insert and
  `$result->num_rows`.
- Prompt 3.5: complete as an explicit gap decision; no selected fixture in this
  slice requires prepared statements yet.
- Prompt 3.6: complete for local loopback HTTP GET/POST MVP behind
  `PHRUST_NET_TESTS=1`.
- Prompt 3.7: complete for selected digest/random/method helpers plus explicit
  verify gap.
- Prompt 3.8: closeout report and non-network baseline gates.
