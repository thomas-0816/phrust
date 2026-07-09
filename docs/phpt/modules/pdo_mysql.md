# pdo_mysql

- Strategy: bounded PDO MySQL execution surface over the existing MySQL runtime
- Classification: partial live driver implementation
- Selected manifest: `tests/phpt/manifests/modules/pdo_mysql.selected.jsonl`
- Fixtures:
  - `tests/phpt/generated/pdo_mysql/platform-checks.phpt`
  - `tests/phpt/generated/pdo_mysql/live-basic-dsn.phpt`

## Implemented Scope

This slice enables the PDO MySQL extension surface and the first live
connection/query path without claiming full mysqlnd parity.

Implemented behavior:

- `extension_loaded("pdo_mysql")`.
- PDO driver discovery includes `mysql` through `pdo_drivers()` and
  `PDO::getAvailableDrivers()`.
- Generated `Pdo\Mysql` class metadata is visible through `class_exists`.
- `new PDO("mysql:host=...;port=...;dbname=...;charset=...", $user, $pass)`
  opens a live MySQL/MariaDB connection through the existing `mysql` crate
  runtime backend.
- `PDO::query`, `PDO::exec`, `PDO::prepare`, `PDOStatement::execute`,
  `fetch`, `fetchAll`, `fetchColumn`, `rowCount`, `columnCount`,
  `closeCursor`, `errorCode`, and `errorInfo` route to MySQL state for
  MySQL-backed PDO handles.
- Basic transaction SQL, `lastInsertId`, `PDO::ATTR_DRIVER_NAME`, and MySQL
  quoting are implemented for MySQL-backed PDO handles.

## Remaining Gaps

- Stable ID: `PHPT-DATA-PDO-MYSQL-MVP-GAPS`
- Unix-socket DSNs, SSL attributes, persistent connections, timeout attributes,
  mysqlnd-specific metadata, warning counts, multi-statement behavior, native
  versus emulated prepare mode, and full result type fidelity remain outside
  this selected slice.
- Live PHPT promotion remains opt-in and must stay disabled unless
  `PHRUST_MYSQL_TEST_DSN` is set.

## Source References

- `ext/pdo_mysql/pdo_mysql.stub.php`
- `ext/pdo_mysql/tests/`

## Target Gates

- `nix develop -c cargo test -p php_std pdo --no-fail-fast`
- `nix develop -c cargo test -p php_runtime pdo --no-fail-fast`
- `nix develop -c cargo test -p php_vm pdo_mysql --no-fail-fast`
- `nix develop -c just phpt-dev-module MODULE=pdo_mysql`
