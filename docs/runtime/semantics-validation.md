# Runtime semantics validation

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

`verify-runtime` runs bytecode snapshots, bytecode execution smoke, VM smoke,
VM trace smoke, runtime fixtures, runtime known-gap validation, Runtime
semantics fixture gates, the Runtime semantics diff harness, the VM semantics
oracle, and runtime hardening lints. Formatting, general linting, and workspace
tests are covered by `ci-rust`, `ci-local`, and the dedicated CI jobs.

## Evidence Map

| Area | Evidence |
| --- | --- |
| Runtime contract | `docs/runtime/semantics-contract.md` |
| Coverage matrix | `docs/runtime/semantics-coverage-matrix.md` |
| Known gaps | `docs/runtime/semantics-known-gaps.md` |
| References and COW | `docs/runtime/semantics-reference-cow.md`, `docs/adr/0027-runtime-semantics-slot-reference-cow.md` |
| Arrays and foreach | `docs/runtime/semantics-array-semantics.md`, `docs/runtime/semantics-foreach-semantics.md`, `docs/adr/0028-runtime-semantics-array-element-reference-foreach.md` |
| Objects, traits, enums, hooks | `docs/runtime/semantics-object-semantics.md`, `docs/adr/0029-runtime-semantics-object-model-traits-enums-hooks.md` |
| Generators and fibers | `docs/runtime/semantics-generators-fibers.md`, `docs/adr/0030-runtime-semantics-generator-fiber-control-flow.md` |
| Reflection and attributes | `docs/runtime/semantics-reflection-attributes.md` |
| Destructors and GC | `docs/adr/0025-runtime-semantics-destructor-queue.md`, `docs/adr/0026-runtime-semantics-gc-skeleton.md` |
| Unsafe and hardening audit | `docs/runtime/semantics-hardening.md` |
| Standard library roadmap | `docs/stdlib/roadmap.md` |

## Docs and CI Consistency

- `README.md` points to the runtime semantics contract, known-gap catalog,
  coverage matrix, unsafe audit, and standard-library roadmap.
- `AGENTS.md` keeps runtime semantics boundaries and requires `verify-runtime`
  before handing off runtime semantics changes.
- `.github/workflows/ci.yml` runs `verify-runtime` in the domain-gates matrix;
  the runtime job bootstraps the pinned reference PHP binary before running the
  gate.
- `scripts/verify/runtime-semantics.sh` remains the local validation script for
  full Runtime semantics verification, including final docs, PHPT allowlist
  categories, regression metadata, minimization tooling, and `runtime-phpt-smoke`.

## Closure Criteria

Any red gate must be classified as an existing baseline issue, a new
regression, or an allowed known gap before a runtime semantics change is
accepted. New regressions are not accepted.
