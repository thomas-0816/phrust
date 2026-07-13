# ADR Index

This directory contains accepted architecture decision records. ADRs are reserved
for durable project contracts: target compatibility, source-of-truth boundaries,
layer ownership, default behavior, and hard safety or compatibility constraints.

Detailed subsystem designs, implementation inventories, and work-item notes live
under the owning topic directories such as `docs/frontend/`, `docs/runtime/`,
`docs/stdlib/`, and `docs/performance/`.

## Decisions

| ADR | Decision |
| --- | --- |
| [0001](0001-target-php-version.md) | Target PHP Version |
| [0002](0002-nix-dev-environment.md) | Nix Development Environment |
| [0003](0003-reference-oracle.md) | Reference Oracle |
| [0004](0004-no-vendored-php-src.md) | No Vendored php-src |
| [0005](0005-layer-boundaries.md) | Layer Boundaries |
| [0006](0006-lossless-cst-parser.md) | Lossless CST Parser |
| [0007](0007-lexer-parser-boundary.md) | Lexer/Parser Boundary |
| [0008](0008-syntax-semantics-boundary.md) | Syntax and Semantics Boundary |
| [0009](0009-frontend-no-runtime-boundary.md) | Semantic Frontend Runtime Boundary |
| [0010](0010-runtime-known-gap-policy.md) | Runtime Known-Gap Policy |
| [0011](0011-stdlib-standard-library-scope.md) | Standard Library Scope |
| [0012](0012-stdlib-composer-source-mode.md) | Composer Source Mode |
| [0013](0013-phar-strategy.md) | PHAR Strategy |
| [0014](0014-performance-scope.md) | Performance Scope |
| [0015](0015-bytecode-cache-format.md) | Bytecode Cache Format |
| [0016](0016-cache-invalidation-model.md) | Inline Cache Invalidation Model |
| [0017](0017-cranelift-jit-experiment.md) | Cranelift JIT Experiment |
| [0018](0018-cranelift-memory-safety.md) | Cranelift Memory Safety Boundary |
| [0019](0019-fast-baseline-native-tier-prerequisites.md) | Fast Baseline Native Tier Prerequisites |

## Adding Records

Add a new ADR only when the decision changes a durable contract. If a topic
needs more implementation detail, add or update the topic document instead and
link to the relevant ADR.

## Additional References

- [0020: Audited unsafe exception for the runtime-memory module](0020-runtime-memory-unsafe-exception.md)
- [ADR 0021: Audited unchecked frame-slot access in `php_vm`](0021-vm-frame-unchecked-access-exception.md)
