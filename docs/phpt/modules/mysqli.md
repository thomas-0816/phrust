# mysqli

- Strategy: WordPress database MVP
- Classification: required-framework
- Selected manifest: `tests/phpt/manifests/modules/mysqli.selected.jsonl`
- Selected gate: selected generated mysqli fixtures with live MySQL access
  disabled by default.

## Decision

`mysqli` is now a WordPress-oriented MVP instead of a policy-only out-of-scope
extension. It registers the selected functions/classes/constants because the
runtime includes a real MySQL/MariaDB client layer, a deterministic
SQLite-backed compatibility adapter for selected application fixtures, and
capability-gated query paths.

Host database access is capability-gated by `PHRUST_MYSQL_TEST_DSN` and disabled
by default. Without that DSN, connection attempts fail deterministically instead
of opening sockets or pretending success.

Selected deterministic fixtures may instead set `PHRUST_MYSQLI_SQLITE_COMPAT=1`.
That path executes common application SQL through an in-memory SQLite database so
query, fetch, error, insert-id, and affected-row behavior can be verified without
opening network sockets. It is compatibility behavior and does not claim MySQL
protocol or SQL-dialect parity.

## Implemented Surface

1. Internal MySQL/MariaDB connection layer behind `PHRUST_MYSQL_TEST_DSN`.
2. Procedural WordPress bootstrap functions:
   `mysqli_connect`, `mysqli_real_connect`, `mysqli_connect_errno`,
   `mysqli_connect_error`, `mysqli_set_charset`, `mysqli_query`,
   `mysqli_fetch_assoc`, `mysqli_fetch_array`, `mysqli_num_rows`,
   `mysqli_num_fields`, `mysqli_insert_id`, `mysqli_affected_rows`,
   `mysqli_free_result`, `mysqli_close`, `mysqli_error`, `mysqli_errno`, and
   `mysqli_real_escape_string`.
3. Object API coverage for `mysqli`, `mysqli_result`, and the wpdb-style
   connect/set-charset/query/escape/fetch/status flow.
4. mysqlnd client metadata probes:
   `mysqli_get_client_info`, `mysqli_get_client_version`,
   `MYSQLND_CLIENT_INFO`, and `MYSQLND_CLIENT_VERSION`.
5. Prepared statement MVP coverage:
   `mysqli_prepare`, `mysqli_stmt_init`, `mysqli_stmt_prepare`,
   `mysqli_stmt_bind_param`, `mysqli_stmt_execute`,
   `mysqli_stmt_get_result`, `mysqli_stmt_bind_result`,
   `mysqli_stmt_fetch`, statement status accessors, close, and free-result.
6. SQLite compatibility coverage is selected only for phrust-owned generated
   fixtures and remains opt-in through `PHRUST_MYSQLI_SQLITE_COMPAT=1`.
7. Structured first-cause diagnostics for selected connection, query, prepare,
   bind, execute, select-db, and charset false-return paths. Payload fields use
   the shared `db_network` WordPress diagnostic context and include
   `diagnostic_id`, `function_name`, `operation`, `capability_state`,
   `dsn_present_boolean`, `host`, `port`, `database_name_if_nonsecret`,
   `mysql_error_code`, `mysql_sqlstate`, and `mysql_error_message`.

## Unsupported Area

- Stable ID: `PHPT-DATA-MYSQLI`
- Reference behavior: PHP with `mysqli` enabled exposes procedural and object
  APIs, `mysqli` classes, connection/query/result/statement behavior, errors,
  options, and mysqlnd integration.
- Current phrust behavior: the WordPress DB/network runtime exposes a narrow
  `mysqli` MVP for connection/query/result/error/escape/close/status and
  selected prepared-statement behavior behind `PHRUST_MYSQL_TEST_DSN`; selected
  phrust-owned fixtures can use `PHRUST_MYSQLI_SQLITE_COMPAT=1` for
  deterministic in-memory query/fetch/status coverage; broad mysqlnd internals
  and full corpus parity remain gaps. `mysqli_report()` stores request-local
  flags and report-enabled failures are classified as recoverable structured
  diagnostics; throwing `mysqli_sql_exception` remains limited by the current
  builtin/runtime exception surface.
- Fixture: `tests/phpt/generated/wp.db-network/mysqli-platform-mvp.phpt` and
  `tests/phpt/generated/mysqli/sqlite-compat-query.phpt`
- Next owner layer: `crates/php_runtime/src/db/**` and
  `crates/php_runtime/src/builtins/modules/mysqli.rs`.

## Source References

- `ext/mysqli/mysqli.stub.php`
- `ext/mysqli/tests/`

## Target Gates

- `nix develop -c just phpt-dev-module MODULE=mysqli`
- `nix develop -c just phpt-dev-module MODULE=wp.db-network`
- `nix develop -c just verify-phpt`
