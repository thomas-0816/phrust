# pdo

- Strategy: platform-negative classification
- Classification: optional
- Selected manifest: `tests/phpt/manifests/modules/pdo.selected.jsonl`
- Current corpus snapshot: 137 `pdo` candidates plus 159 `pdo_mysql` driver
  candidates in the committed baseline, for 296 candidates covered by this
  manifest. The broader triage row that includes `pdo_sqlite` is 376
  candidates, 0 PASS, 124 SKIP, 249 FAIL, 3 BORK, and 376 known non-green
  outcomes.

## Decision

Keep PDO out of the active runtime surface for this branch.

PDO core is not required for core language progress, and this branch explicitly
does not build network database clients or a Zend extension ABI. The selected
contract therefore keeps platform probes negative rather than exposing a fake
`PDO` class or fake query success. `pdo_sqlite` and `sqlite3` remain separate
decisions because an eventual SQLite-only MVP would still need real database
semantics rather than PDO-shaped placeholders.

## Unsupported Area

- Stable ID: `PHPT-DATA-PDO-CORE`
- Reference behavior: PHP with PDO enabled exposes `extension_loaded("pdo")`,
  `PDO`, `PDOException`, `PDOStatement`, drivers, attributes, exceptions, and
  real driver-backed connection/query behavior.
- Current phrust behavior: the PDO extension is unavailable;
  `extension_loaded("pdo")`, `class_exists("PDO")`,
  `class_exists("PDOException")`, and `class_exists("PDOStatement")` are false.
- Fixture: `tests/phpt/generated/pdo/platform-checks.phpt`
- Next owner layer: future `php_std`/database extension layer with a real PDO
  abstraction and driver capability model.

## Source References

- `ext/pdo/pdo.stub.php`
- `ext/pdo/pdo_dbh.stub.php`
- `ext/pdo/pdo_stmt.stub.php`
- `ext/pdo/tests/`
- `ext/pdo_mysql/tests/` is counted in this PDO manifest but remains network DB
  out-of-scope.

## Target Gates

- `nix develop -c just phpt-dev-module MODULE=pdo`
- `nix develop -c just verify-phpt`
