# mysqlnd

- Strategy: out-of-scope network driver classification
- Classification: out-of-scope
- Selected manifest: `tests/phpt/manifests/modules/mysqlnd.selected.jsonl`
- Current corpus snapshot: mysqlnd-specific PHPTs are represented through the
  `mysqli` and `pdo_mysql` module rows; there is no separate raw `mysqlnd`
  module row in the committed baseline.

## Decision

Keep mysqlnd out of scope for this branch.

mysqlnd is a native MySQL driver implementation detail for `mysqli` and
`pdo_mysql`, not an isolated PHP userland API to stub in phrust. It requires
network protocol behavior, authentication, charset handling, result buffering,
packet parsing, and driver hooks. This branch classifies those failures under
the database extension policy and does not add mysqlnd runtime code.

## Unsupported Area

- Stable ID: `PHPT-DATA-MYSQLND`
- Reference behavior: PHP builds using mysqlnd provide native MySQL protocol
  support beneath `mysqli` and PDO MySQL, including driver stats/options and
  mysqlnd-specific behavior in those test suites.
- Current phrust behavior: `extension_loaded("mysqlnd")` is false, and no
  mysqlnd driver layer exists.
- Fixture: `tests/phpt/generated/mysqlnd/platform-checks.phpt`
- Next owner layer: future optional MySQL driver layer, if network database
  support is ever accepted.

## Source References

- `ext/mysqlnd/`
- `ext/mysqli/tests/*mysqlnd*.phpt`
- `ext/pdo_mysql/tests/`

## Target Gates

- `nix develop -c just phpt-dev-module MODULE=mysqlnd`
- `nix develop -c just verify-phpt`
