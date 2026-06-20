# Parser Snapshot Fixtures

Parser snapshots are stored under `crates/php_syntax/tests/snapshots/`.

They are generated from curated fixtures in `fixtures/parser/` and intentionally
avoid absolute paths. Snapshot content includes fixture-relative path,
roundtrip status, diagnostics summary, and either the CST debug tree or
diagnostic details.

Update snapshots with:

```bash
nix develop -c just parser-snapshots
```

Review `.snap` changes before committing. Snapshot tests are deterministic and
run as part of `cargo test`.
