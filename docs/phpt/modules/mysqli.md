# mysqli

- Strategy: WordPress database MVP
- Classification: required-framework
- Selected manifest: `tests/phpt/manifests/modules/mysqli.selected.jsonl`
- Current corpus snapshot: 442 `mysqli` candidates, 2 PASS, 4 SKIP, 429 FAIL,
  4 BORK, and 442 known non-green outcomes.

## Decision

`mysqli` is now a WordPress-oriented MVP instead of a policy-only out-of-scope
extension. It registers the selected functions/classes/constants only because
the branch added a real MySQL/MariaDB client layer and capability-gated query
path.

Host database access is capability-gated by `PHRUST_MYSQL_TEST_DSN` and disabled
by default. Without that DSN, connection attempts fail deterministically instead
of opening sockets or pretending success.

## Implemented Surface

1. Internal MySQL/MariaDB connection layer behind `PHRUST_MYSQL_TEST_DSN`.
2. Procedural WordPress bootstrap functions:
   `mysqli_connect`, `mysqli_real_connect`, `mysqli_connect_errno`,
   `mysqli_connect_error`, `mysqli_set_charset`, `mysqli_query`,
   `mysqli_fetch_assoc`, `mysqli_fetch_array`, `mysqli_num_rows`,
   `mysqli_free_result`, `mysqli_close`, `mysqli_error`, `mysqli_errno`, and
   `mysqli_real_escape_string`.
3. Object API coverage for `mysqli`, `mysqli_result`, and the wpdb-style
   connect/set-charset/query/escape/fetch flow.
4. Prepared statements are documented as an explicit gap because no selected
   WordPress-style fixture in this branch requires `mysqli_stmt_*`.
5. mysqlnd-specific behavior remains represented as driver metadata/gaps unless
   a selected fixture requires a real user-visible surface.

## Unsupported Area

- Stable ID: `PHPT-DATA-MYSQLI`
- Reference behavior: PHP with `mysqli` enabled exposes procedural and object
  APIs, `mysqli` classes, connection/query/result/statement behavior, errors,
  options, and mysqlnd integration.
- Current phrust behavior: the WordPress DB/network branch exposes a narrow
  `mysqli` MVP for connection/query/result/error/escape/close behavior behind
  `PHRUST_MYSQL_TEST_DSN`; prepared statements remain an explicit unsupported
  diagnostic because the selected fixtures do not require them yet.
- Fixture: `tests/phpt/generated/wp.db-network/mysqli-platform-mvp.phpt`
- Next owner layer: `crates/php_runtime/src/db/**` and
  `crates/php_runtime/src/builtins/modules/mysqli.rs`.

## Source References

- `ext/mysqli/mysqli.stub.php`
- `ext/mysqli/tests/`

## Target Gates

- `nix develop -c just phpt-dev-module MODULE=mysqli`
- `nix develop -c just phpt-dev-module MODULE=wp.db-network`
- `nix develop -c just verify-phpt`
