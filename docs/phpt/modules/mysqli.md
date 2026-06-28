# mysqli

- Strategy: out-of-scope network database classification
- Classification: out-of-scope
- Selected manifest: `tests/phpt/manifests/modules/mysqli.selected.jsonl`
- Current corpus snapshot: 442 `mysqli` candidates, 2 PASS, 4 SKIP, 429 FAIL,
  4 BORK, and 442 known non-green outcomes.

## Decision

Keep `mysqli` out of scope for this branch.

`mysqli` is a network database client surface and depends on MySQL protocol,
connection handling, result metadata, prepared statements, and mysqlnd/libmysql
integration. The prompt explicitly forbids network DB support, so this branch
does not add runtime stubs or partial query behavior.

## Unsupported Area

- Stable ID: `PHPT-DATA-MYSQLI`
- Reference behavior: PHP with `mysqli` enabled exposes procedural and object
  APIs, `mysqli` classes, connection/query/result/statement behavior, errors,
  options, and mysqlnd integration.
- Current phrust behavior: `extension_loaded("mysqli")` and
  `class_exists("mysqli")` are false.
- Fixture: `tests/phpt/generated/mysqli/platform-checks.phpt`
- Next owner layer: future optional database extension package, not the core
  runtime or current PHPT branch.

## Source References

- `ext/mysqli/mysqli.stub.php`
- `ext/mysqli/tests/`

## Target Gates

- `nix develop -c just phpt-dev-module MODULE=mysqli`
- `nix develop -c just verify-phpt`
