# WordPress Web, Database, and Diagnostics Bring-Up

This slice covers integrated HTTP request mapping, response state behavior,
MySQLi database access, and deterministic first-cause reporting for WordPress
bring-up smoke runs. It does not special-case WordPress paths, tables, or SQL.

## Report Schema

`just wordpress-smoke-report` writes
`target/wordpress-bringup/report.json` and
`target/wordpress-bringup/report.md`.

Required JSON fields:

```text
schema_version
status
wordpress_error_class
owner_layer
diagnostic_id
source_path
source_span
first_php_frame
runtime_stack
include_stack
autoload_stack
request_context_summary
database_context_summary
last_vm_instruction
counters
stdout_digest
stderr_digest
secondary_errors
suggested_reduced_fixture_path
```

The committed sample failure is
`fixtures/runtime_semantics/wp_web_db_diagnostics/diagnostics/classified_failure.json`.
It classifies a statement execution failure as `database_mysqli` and preserves
the first PHP frame, owner layer, SQLSTATE-like status, and reduced fixture path.

## Web Fixtures

- `web_request/superglobals.php`: validates seeded request method, URI,
  `PATH_INFO`, GET, POST, COOKIE, and normalized `HTTP_*` header values.
- `web_request/path_info.php`: validates script name, `PHP_SELF`,
  script filename, document root, and separated path info.
- `web_request/response_headers.php`: validates response status, redirect
  header, cookie emission, and output-buffer content.

Existing server tests in `crates/php_server/tests/health.rs` cover traversal
rejection, request body parsing, uploads, `php://input`, header replacement,
response-splitting rejection, sessions, and timeout responses.

## MySQLi Surface

Connection and status:
`mysqli_init`, `mysqli_real_connect`, `mysqli_connect`, `mysqli_close`,
`mysqli_ping`, `mysqli_options`, `mysqli_set_charset`,
`mysqli_character_set_name`, `mysqli_select_db`, `mysqli_get_server_info`,
`mysqli_get_client_info`, `mysqli_get_client_version`,
`mysqli_connect_errno`, `mysqli_connect_error`, `mysqli_errno`,
`mysqli_error`, `mysqli_sqlstate`, and `mysqli_report`.

Query and result:
`mysqli_query`, `mysqli_real_query`, `mysqli_multi_query`,
`mysqli_more_results`, `mysqli_next_result`, `mysqli_store_result`,
`mysqli_use_result`, `mysqli_fetch_assoc`, `mysqli_fetch_array`,
`mysqli_fetch_row`, `mysqli_fetch_object`, `mysqli_num_rows`,
`mysqli_num_fields`, `mysqli_fetch_field`, `mysqli_free_result`,
`mysqli_data_seek`, `mysqli_affected_rows`, `mysqli_insert_id`, and
`mysqli_real_escape_string`.

Prepared statements:
`mysqli_prepare`, `mysqli_stmt_bind_param`, `mysqli_stmt_execute`,
`mysqli_stmt_get_result`, `mysqli_stmt_bind_result`, `mysqli_stmt_fetch`,
`mysqli_stmt_num_rows`, `mysqli_stmt_affected_rows`,
`mysqli_stmt_insert_id`, `mysqli_stmt_error`, `mysqli_stmt_errno`,
`mysqli_stmt_sqlstate`, and `mysqli_stmt_close`.

The object API for `mysqli`, `mysqli_result`, and `mysqli_stmt` shares the same
runtime handles as the procedural API.

## Integration Setup

`just mysqli-integration` is explicit and does not require Docker or network for
normal unit tests. If `PHRUST_MYSQL_TEST_DSN` is unset, it writes
`target/mysqli-integration/report.json` with `environment_blocker` and exits
successfully. If the DSN is set, it runs the live `php_runtime` MySQL smoke tests
for connection, query, result, temporary table cleanup, and prepared statements.

For deterministic local fixtures without MariaDB, set
`PHRUST_MYSQLI_SQLITE_COMPAT=1`; this routes selected tests through the in-memory
SQLite compatibility adapter.

## Known Gaps

- `E_PHP_MYSQLI_MULTI_RESULT_GAP`: multi-result traversal is represented by
  stable `false` from `mysqli_more_results` and `mysqli_next_result`.
- `E_PHP_MYSQLI_NATIVE_PROTOCOL_TYPE_GAP`: prepared statements are rendered
  through bounded SQL literals before dispatch; native binary protocol type
  fidelity remains an integration-test expansion area.
- `E_PHP_MYSQLI_PERSISTENT_CONNECTION_GAP`: persistent connection semantics are
  not implemented.

## Merge Notes

Branches adding frontend/lowering or autoload semantics should preserve these
diagnostic fields unchanged. Runtime and request-context consumers should keep
`RuntimeHttpRequestContext`, `RuntimeHttpResponseState`, and `MysqlState` as the
shared API boundaries so web, runtime, and database failures classify their first
cause without hiding later cascades.
