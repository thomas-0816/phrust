# Project Guidelines

## Repository Workflow

- Inspect the repository before changing files.
- Use `nix develop -c ...` for validation commands.
- Complete every change with relevant checks and report skipped checks clearly.
- If a check cannot run because of missing network, missing reference binaries,
  or platform support, report the skipped check and exact reason.
- Do not silently skip checks.
- Keep scripts deterministic and provide clear error messages.
- Use `bash` scripts with `set -euo pipefail`.
- Make script files executable when they are added.
- Update documentation together with tooling changes.
- In a dirty worktree, stage only files intentionally changed for the current
  task and never revert unrelated user changes.

## Reference Target

- PHP series: `8.5`
- PHP version: `8.5.7`
- Git tag: `php-8.5.7`
- Repository: `https://github.com/php/php-src.git`

Do not automatically update the target PHP version without a new ADR.

## Scope Boundaries

- Do not implement VM, runtime values, JIT, extensions, or Zend ABI emulation
  unless the user explicitly asks for that layer.
- Parser and CST work must reuse the existing lexer. Do not introduce a second
  lexer.
- Do not hardcode numeric PHP token values.
- Compare reference behavior by token names, token text, diagnostics, and
  source positions rather than raw numeric token IDs.
- Preserve byte-based spans as the source of truth. Treat line and column as
  derived display information.
- Public lexer and parser APIs must not panic on invalid input.
- Reference-dependent checks must skip clearly when no PHP reference binary is
  available and must be strict when `REFERENCE_PHP` is explicitly set.
- Do not commit generated reports under `target/`.
- Do not commit extracted `php-src` corpus files or a vendored `php-src` copy.
- Keep local reference checkouts under `third_party/`.

## Validation Commands

- Use the narrowest relevant check while iterating.
- Use `nix develop -c just help` to discover the current canonical gates.
- Before finishing foundation, reference-tooling, lexer, parser, or CST work,
  run the strongest relevant verification target available in `just help`.
- Parser fixture, diff, and roundtrip gates should be run when available.

## Codex Operating Profile

- Preferred launch command:

```bash
codex -p phrust-engine --cd /Volumes/CrucialMusic/src/phrust
```

- The matching profile is `~/.codex/phrust-engine.config.toml`.
- Keep work vertical and auditable: requirement mapping, implementation,
  focused tests, then the relevant `nix develop -c just ...` gate.

## Commit Message Rules

- Use conventional commits: `type(scope): description`.
- Keep the first line under 72 characters.
- Use imperative mood.
- Never mention Codex, Anthropic, assistants, or assisted development in commit
  messages.
