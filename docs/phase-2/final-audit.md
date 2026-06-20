# Parser and CST Final Audit

Audit date: 2026-06-20

## Status

Complete for the parser/CST milestone. The repository has a Rust syntax crate,
parser CLI, lossless CST, strict fixture comparison against the pinned PHP
8.5.7 reference, and documented handoff boundaries for semantic layers.

No reference-dependent checks were skipped in this audit. The local reference
binary was available at `third_party/php-src/sapi/cli/php` and reported PHP
8.5.7.

## Gate Results

| Command | Result | Notes |
| --- | --- | --- |
| `nix develop -c just verify-phase0` | pass | Foundation files, Rust workspace checks, and PHP reference lock verification passed. |
| `nix develop -c just verify-phase1` | pass | Lexer verification passed, including strict lexer fixture comparison with the PHP 8.5.7 reference. |
| `nix develop -c just verify-phase2` | pass | Central parser gate passed; includes formatting, clippy, workspace tests, parser diff, and CST roundtrip. |
| `nix develop -c cargo fmt --all --check` | pass | No formatting drift. |
| `nix develop -c cargo clippy --workspace --all-targets -- -D warnings` | pass | No clippy warnings. |
| `nix develop -c cargo test --workspace` | pass | Workspace tests passed, including parser snapshots, diagnostics, property smoke, and fixture roundtrip. |
| `nix develop -c just parser-fixtures` | pass | PHP lint oracle checked 65 parser fixtures with the pinned local reference. Invalid fixtures were rejected by PHP as expected. |
| `nix develop -c just parser-diff` | pass | Compared 65 parser fixtures; `allowed gaps=0`. |
| `nix develop -c just cst-roundtrip` | pass | All committed parser fixtures reconstructed exactly from CST tokens. |

## Scope Audit

- No VM, runtime value model, JIT, extensions, or Zend ABI emulation were added.
- No AST/HIR or semantic lowering layer was added.
- `crates/php_syntax` depends on `php_lexer` and consumes lexer tokens through
  `TokenSource`.
- No second lexer exists in `crates/php_syntax`; lexer construction remains in
  `php_lexer`.
- Invalid-input robustness is covered by parser unit tests and deterministic
  property smoke tests.
- Roundtrip checks exist at unit, snapshot, parser-diff, and all-fixture gate
  levels.
- The known-gap allowlist is empty, and `docs/phase-2/parser-known-gaps.md`
  records no accepted curated fixture gaps.
- `docs/phase-2/grammar-coverage.md` is current for the implemented fixture
  surface.

## Residual Gaps

P1: Complex interpolation internals remain shallow CST structure. The parser
preserves and groups encapsed string and heredoc tokens losslessly, but the
lexer-mode token stream does not yet expose every interpolation body as normal
expression-mode syntax.

P2: Optional php-src corpus smoke still reports exploratory deviations outside
the curated fixture contract. These are not accepted gaps. Any real issue should
be reduced into a committed fixture before adding an allowlist entry.

P2: Incremental reparsing is prepared through byte ranges and optional source
identity, but no stable node identity or subtree reuse implementation exists.

## Release Note

Suggested local tag name if the project later wants one:
`parser-cst-complete`.

No git tag was created during this audit.

## Recommendation

Start the next workstream from `docs/phase-2/handoff-to-phase-3.md`. The first
vertical slice should build read-only declaration tables from the existing CST,
then add separate semantic diagnostics without changing parser acceptance or
roundtrip behavior.
