# Rust Quality Tooling

`just check` remains the narrow Rust edit-loop baseline: formatting, Clippy
with `-D warnings`, and workspace tests. `just quality-fast` is the required
cheap integrity/dependency/docs gate used by local CI parity and GitHub
Actions. The broader `just quality` aggregate adds opt-in and discovery checks
without changing the default edit loop.

## Quality Gates

| Area | Command | Behavior |
| --- | --- | --- |
| Required fast gate | `nix develop -c just quality-fast` | Runs source integrity, known-gap manifest validation, dependency policy, unused dependency detection, all-features compile coverage, rustdoc warnings as errors, and doctests. |
| Supply chain | `nix develop -c just quality-deps` | Runs `cargo-deny` for advisories, license policy, banned crates, wildcard dependencies, and unknown sources. |
| Unused dependencies | `nix develop -c just quality-unused-deps` | Runs `cargo machete` against the workspace. |
| Coverage | `PHRUST_RUN_COVERAGE=1 nix develop -c just quality-coverage` | Runs `cargo-llvm-cov`, using `cargo nextest` when available. Skips clearly unless explicitly enabled. |
| Mutation testing | `PHRUST_RUN_MUTANTS=1 nix develop -c just quality-mutants` | Runs `cargo-mutants`. This is intentionally opt-in because it is much slower than normal tests. |
| Fuzz and property smokes | `nix develop -c just quality-fuzz` | Runs deterministic lexer/parser/runtime/VM fuzz and property smokes, then reports whether `cargo-fuzz` is available for coverage-guided expansion. |
| Rustdoc | `nix develop -c just quality-docs` | Builds workspace library docs with `RUSTDOCFLAGS="-D warnings"` and runs doctests. |
| Public API compatibility | `nix develop -c just quality-api` | Runs `cargo-semver-checks` against `PHRUST_SEMVER_BASELINE`, defaulting to `HEAD`. Use `PHRUST_SEMVER_BASELINE=origin/main` in PR workflows with that ref fetched. |
| Stricter lint discovery | `nix develop -c just quality-lints` | Runs Clippy `pedantic` and `nursery` as warning-only discovery, with noisy documentation and size lints allowed for now. |

`nix develop -c just quality` runs `quality-fast` plus the slower or
discovery-oriented quality targets. Expensive coverage and mutation gates still
skip unless their environment variables are set.

## Policy Notes

- `deny.toml` is the source of truth for dependency advisory, license, source,
  and ban policy.
- Wildcard dependencies are denied. Local workspace path dependencies carry
  explicit versions so the deny gate can enforce that policy. Multiple versions
  remain warning-only while current `rusqlite`, `serde_json`, Cranelift, and
  platform-support transitive graphs require duplicate support crates.
- The all-features compile check currently records existing JIT helper unused
  warnings as warning-only coverage; normal `just lint` remains the warning
  denial gate for default features.
- `cargo machete` findings should be fixed when they are real. Use
  `[package.metadata.cargo-machete] ignored = [...]` only for known false
  positives such as dependencies whose crate import name differs from the Cargo
  package name.
- Coverage and mutation reports are local artifacts under `target/` and must
  not be committed.
- `quality-lints` is a discovery target, not a style rewrite mandate. Promote
  individual Clippy lints into `just lint` only after they are low-noise for the
  current workspace.
