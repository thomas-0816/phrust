# runtime-semantics Final Audit

Runtime semantics owns executable PHP behavior over the existing frontend,
runtime, and VM pipeline:

```text
php_lexer -> php_syntax -> php_ast -> php_semantics/HIR -> php_ir -> php_runtime -> php_vm -> php_vm_cli
```

No second lexer, parser, semantic frontend, or source-string execution path is
part of this layer.

## Required Gates

Run these before handing off runtime semantics changes:

```bash
nix develop -c just verify-runtime
nix develop -c cargo test --workspace
```

`verify-runtime` includes formatting, linting, workspace tests, Runtime
verification, Runtime semantics fixture gates, the Runtime semantics diff
harness, PHPT smoke allowlist checks, hardening lints, the devshell toolchain
audit, and documentation checks.

## Evidence Map

| Area | Evidence |
| --- | --- |
| Runtime contract | `docs/runtime-semantics-contract.md` |
| Coverage matrix | `docs/runtime-semantics-coverage-matrix.md` |
| Known gaps | `docs/runtime-semantics-known-gaps.md` |
| References and COW | `docs/runtime-semantics-reference-cow.md`, `docs/adr/0027-runtime-semantics-slot-reference-cow.md` |
| Arrays and foreach | `docs/runtime-semantics-array-semantics.md`, `docs/runtime-semantics-foreach-semantics.md`, `docs/adr/0028-runtime-semantics-array-element-reference-foreach.md` |
| Objects, traits, enums, hooks | `docs/runtime-semantics-object-semantics.md`, `docs/adr/0029-runtime-semantics-object-model-traits-enums-hooks.md` |
| Generators and fibers | `docs/runtime-semantics-generators-fibers.md`, `docs/adr/0030-runtime-semantics-generator-fiber-control-flow.md` |
| Reflection and attributes | `docs/runtime-semantics-reflection-attributes.md` |
| Destructors and GC | `docs/adr/0025-runtime-semantics-destructor-queue.md`, `docs/adr/0026-runtime-semantics-gc-skeleton.md` |
| Unsafe and hardening audit | `docs/runtime-semantics-unsafe-audit.md` |
| Standard library roadmap | `docs/stdlib-roadmap.md` |

## Docs and CI Consistency

- `README.md` points to the runtime semantics contract, known-gap catalog,
  coverage matrix, unsafe audit, and standard-library roadmap.
- `AGENTS.md` keeps runtime semantics boundaries and requires `verify-runtime`
  before handing off runtime semantics changes.
- `.github/workflows/runtime-semantics.yml` runs
  `nix develop -c just verify-runtime` and uploads Runtime semantics/Runtime
  report artifacts when present.
- `scripts/verify/runtime-semantics.sh` asserts final docs, PHPT allowlist
  categories, regression metadata, and minimization tooling.

## Closure Criteria

Any red gate must be classified as an existing baseline issue, a new
regression, or an allowed known gap before a runtime semantics change is
accepted. New regressions are not accepted.
