# ADR Index

This directory contains accepted architecture decision records. ADR filenames
use a stable numeric prefix; do not renumber accepted ADRs just to close gaps.
When an older unnumbered or duplicate-numbered record is cleaned up, assign the
next appropriate free number and update direct links.

## Number Ranges

| Range | Area |
| --- | --- |
| 0001-0006 | Foundation, reference target, and lexer prerequisites |
| 0007-0016 | Parser and semantic frontend boundaries |
| 0017-0032 | Runtime, VM, and runtime-semantics decisions |
| 0060-0067 | Standard library and extension strategy |
| 0070-0076 | Performance interpreter, bytecode cache, and JIT experiments |
| 0780-0787 | Cranelift and native-tier addenda |

## Ordered Decisions

### Foundation

| ADR | Decision |
| --- | --- |
| [0001](0001-target-php-version.md) | Target PHP Version |
| [0002](0002-nix-dev-environment.md) | Nix Development Environment |
| [0003](0003-reference-oracle.md) | Reference Oracle |
| [0004](0004-no-vendored-php-src.md) | No Vendored php-src |
| [0005](0005-layer-boundaries.md) | Layer Boundaries |
| [0006](0006-byte-oriented-lossless-lexer.md) | Byte-Oriented Lossless Lexer |

### Parser And Frontend

| ADR | Decision |
| --- | --- |
| [0007](0007-lossless-cst-parser.md) | Lossless CST Parser |
| [0008](0008-lexer-parser-boundary.md) | Lexer Parser Boundary |
| [0009](0009-pratt-expression-parser.md) | Pratt Expression Parser |
| [0010](0010-syntax-semantics-boundary.md) | Syntax and Semantics Boundary |
| [0011](0011-typed-ast-views.md) | Typed AST Views over CST |
| [0012](0012-hir-symbol-ids-and-interning.md) | HIR, Symbol IDs, and Interning |
| [0013](0013-php-name-resolution.md) | PHP Name Resolution |
| [0014](0014-compile-time-diagnostics.md) | Compile-Time Diagnostics |
| [0015](0015-constant-expression-lowering.md) | Constant Expression Lowering |
| [0016](0016-frontend-no-runtime-boundary.md) | Semantic frontend Runtime Boundary |
| [0031](0031-token-oracle-normalization.md) | Token Oracle Normalization |
| [0032](0032-parser-error-recovery.md) | Parser Error Recovery |

### Runtime And VM

| ADR | Decision |
| --- | --- |
| [0017](0017-runtime-register-ir.md) | Runtime Register IR |
| [0018](0018-runtime-vm-dispatch.md) | Runtime VM Dispatch |
| [0019](0019-runtime-value-representation.md) | Runtime Value Representation |
| [0020](0020-runtime-array-mvp.md) | Runtime Array MVP |
| [0021](0021-runtime-object-mvp.md) | Runtime Object MVP |
| [0022](0022-runtime-exception-model.md) | Runtime Exception Model |
| [0023](0023-runtime-include-model.md) | Runtime Include Model |
| [0024](0024-runtime-known-gap-policy.md) | Runtime Known-Gap Policy |

### Runtime Semantics

| ADR | Decision |
| --- | --- |
| [0025](0025-runtime-semantics-destructor-queue.md) | Runtime semantics Destructor Queue MVP |
| [0026](0026-runtime-semantics-gc-skeleton.md) | Runtime semantics GC Skeleton and Root Tracking |
| [0027](0027-runtime-semantics-slot-reference-cow.md) | Runtime semantics Slot, Reference, and Copy-on-Write Model |
| [0028](0028-runtime-semantics-array-element-reference-foreach.md) | Runtime semantics Array Element References and Foreach |
| [0029](0029-runtime-semantics-object-model-traits-enums-hooks.md) | Runtime semantics Object Model, Traits, Enums, and Hooks |
| [0030](0030-runtime-semantics-generator-fiber-control-flow.md) | Runtime semantics Generator and Fiber Control Flow |

### Standard Library

| ADR | Decision |
| --- | --- |
| [0060](0060-stdlib-standard-library-scope.md) | Standard library Scope |
| [0061](0061-stdlib-extension-registry.md) | Standard library Extension Registry |
| [0062](0062-stdlib-builtin-abi.md) | Standard library Builtin Function ABI |
| [0063](0063-stdlib-streams-and-capabilities.md) | Standard library Streams and Capabilities |
| [0064](0064-stdlib-composer-source-mode.md) | Standard library Composer Source Mode |
| [0065](0065-stdlib-pcre-date-strategy.md) | Standard library PCRE and Date Strategy |
| [0066](0066-phar-strategy.md) | Standard library PHAR Strategy |
| [0067](0067-dom-xml-extension-strategy.md) | DOM/XML Extension Strategy |

### Performance

| ADR | Decision |
| --- | --- |
| [0070](0070-performance-scope.md) | Performance Scope |
| [0072](0072-bytecode-cache-format.md) | Bytecode Cache Format |
| [0074](0074-quickening-inline-cache-model.md) | Quickening And Inline Cache Model |
| [0075](0075-cache-invalidation-model.md) | Inline Cache Invalidation Model |
| [0076](0076-cranelift-jit-experiment.md) | Cranelift JIT Experiment |

### Native Tier Addenda

| ADR | Decision |
| --- | --- |
| [0780](0780-cranelift-addendum-scope.md) | Cranelift Big-Wins Addendum Scope |
| [0781](0781-jit-backend-api.md) | Backend-Neutral JIT API |
| [0782](0782-cranelift-runtime-abi.md) | Cranelift Runtime ABI |
| [0783](0783-cranelift-side-exit-model.md) | Cranelift Side-Exit Model |
| [0785](0785-cranelift-memory-safety.md) | Cranelift Memory Safety Boundary |
| [0786](0786-cranelift-tiering-policy.md) | Cranelift Tiering Policy |
| [0787](0787-fast-baseline-native-tier-prerequisites.md) | Fast Baseline Native Tier Prerequisites |
