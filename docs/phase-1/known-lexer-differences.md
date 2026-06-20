# Known Lexer Differences

Phase 1 has strict Rust-vs-PHP fixture comparison for the curated lexer
fixtures.

## Allowlisted Rules

No curated fixture differences are currently allowlisted.

Future temporary exceptions must explain the affected fixture, the exact
mismatch, and the Phase 1 or Phase 2 issue that will remove the exception. The
default Phase 1 gate must remain strict.

## Burn-Down Command

Use strict diffing to expose any mismatch:

```bash
nix develop -c just lexer-diff
```

Use the report command to collect a machine-readable success or failure report:

```bash
nix develop -c just lexer-diff-report
```

The JSON report is written to `target/lexer-diff-report.json` and must not be
committed.

## Remaining Non-Fixture Gaps

- Add byte-exact invalid-input handling if Phase 2 needs non-UTF-8 source
  preservation.
- Decide whether `TOKEN_PARSE` contextual keyword relaxation belongs in lexer
  configuration or parser/CST construction.
