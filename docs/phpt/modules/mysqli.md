# mysqli

- Strategy: WordPress database MVP
- Classification: required-framework
- Selected manifest: `tests/phpt/manifests/modules/mysqli.selected.jsonl`
- Current corpus snapshot: 442 `mysqli` candidates, 2 PASS, 4 SKIP, 429 FAIL,
  4 BORK, and 442 known non-green outcomes.

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
4. Prepared statements are documented as an explicit gap because the selected
   WordPress-style fixtures do not require `mysqli_stmt_*`.
5. mysqlnd-specific behavior remains represented as driver metadata/gaps unless
   a selected fixture requires a real user-visible surface.
6. SQLite compatibility coverage is selected only for phrust-owned generated
   fixtures and remains opt-in through `PHRUST_MYSQLI_SQLITE_COMPAT=1`.

## Unsupported Area

- Stable ID: `PHPT-DATA-MYSQLI`
- Reference behavior: PHP with `mysqli` enabled exposes procedural and object
  APIs, `mysqli` classes, connection/query/result/statement behavior, errors,
  options, and mysqlnd integration.
- Current phrust behavior: the WordPress DB/network runtime exposes a narrow
  `mysqli` MVP for connection/query/result/error/escape/close/status behavior
  behind `PHRUST_MYSQL_TEST_DSN`; selected phrust-owned fixtures can use
  `PHRUST_MYSQLI_SQLITE_COMPAT=1` for deterministic in-memory query/fetch/status
  coverage; prepared statements remain an explicit unsupported diagnostic
  because the selected fixtures do not require them yet.
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
