# pdo

- Strategy: SQLite-backed core MVP
- Classification: real implementation for SQLite infrastructure
- Selected manifest: `tests/phpt/manifests/modules/pdo.selected.jsonl`
- Fixture: `tests/phpt/generated/pdo/platform-checks.phpt`

## Implemented Scope

enables PDO core because PDO_SQLite now has a real SQLite engine
under it.

Implemented behavior:

- `extension_loaded("pdo")`, `pdo_drivers`, and PDO class visibility.
- `PDO`, `PDOException`, `PDOStatement`, and `PDORow` platform probes.
- Core PDO constants used by the MVP fetch/error-mode surface.
- SQLite DSN construction through `new PDO("sqlite:...")`.
- `PDO::exec`, `query`, `prepare`, `errorCode`, `errorInfo`,
  `getAttribute`, `setAttribute`, and `quote`.
- Selected `PDO::ERRMODE_EXCEPTION` handling for SQLite query and statement
  execution failures.
- `PDOStatement::execute`, `fetch`, `fetchAll`, `fetchColumn`,
  `columnCount`, `rowCount`, `closeCursor`, `errorCode`, `errorInfo`, and
  `setFetchMode`.

## Remaining Gaps

- Stable ID: `PHPT-DATA-PDO-MVP-GAPS`
- Only the SQLite driver is available; MySQL, PostgreSQL, ODBC, Firebird, and
  other drivers are explicitly out of scope.
- Persistent connections, transactions, cursor orientation, full attribute
  handling, bound parameters beyond the selected SQLite slice, bound columns,
  lazy rows, object/class fetch modes, and exact warning text are not complete.
- PDO class constants are available for the MVP set only.

## Source References

- `ext/pdo/pdo.stub.php`
- `ext/pdo/pdo_dbh.stub.php`
- `ext/pdo/pdo_stmt.stub.php`
- `ext/pdo/tests/`

## Target Gates

- `nix develop -c cargo test -p php_runtime`
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just phpt-dev-module MODULE=pdo`
