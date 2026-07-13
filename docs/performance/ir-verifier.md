# Performance IR Verifier Contract

Performance optimizer, cache, quickening, inline-cache, and JIT work must treat
`php_ir::verify_unit()` as the boundary check before and after every transform.
The performance layer does not add an optimizer pass; the post-optimizer check is the
identity boundary until the performance layer provides real optimization modes.

## Gate

```bash
nix develop -c just ir-verify
nix develop -c just optimizer-diff
```

`optimizer-diff` runs `ir-verify` before its placeholder, so
the layer gate already proves verifier coverage before optimizer work lands.

## Invariants

The verifier checks these structural invariants:

- supported `IR_VERSION`;
- valid entry function;
- table IDs matching file, class, block, and instruction positions;
- source spans pointing at known files with non-decreasing byte ranges;
- valid register, local-slot, constant, function, and branch target IDs;
- every basic block has a terminator;
- exception/finally edge targets from `EnterTry` and `EndFinally` point at
  blocks in the same function;
- call argument operands and by-reference locals are valid;
- unpacked call arguments cannot carry by-reference local metadata;
- by-reference return metadata must point at the same local returned as the
  terminator value;
- register operands must be defined on every reachable incoming control-flow
  path before use.

PHP local slots are not required to be assigned before `LoadLocal`. Reading an
unset PHP variable is observable runtime behavior, so the verifier only checks
that local slots are declared in the function local table. Register
definition-before-use ignores unreachable predecessors but still leaves
structural ID, span, terminator, and edge checks enabled for all blocks.

## Optimizer-Sensitive Operations

The following instruction families are treated as not reorderable unless a
future pass proves an equivalent PHP-visible result and has dedicated
differential tests:

- type checks: `InstanceOf`, typed call/return/property enforcement consumers;
- references: `BindReference`, `BindReferenceDim`, `BindReferenceFromDim`,
  `BindReferenceFromCall`, by-reference calls, and by-reference returns;
- copy-on-write-sensitive array operations: `ArrayInsert`, `AssignDim`,
  `AppendDim`, `FetchDim`, `ForeachInit`, `ForeachNext`, `ForeachInitRef`, and
  `ForeachNextRef`;
- side-effecting runtime operations: calls, includes, eval, property access,
  autoload-sensitive object/class operations, throws, try/finally edges, output,
  and runtime diagnostics.

Verifier checks are structural. They do not prove semantic equivalence for
optimizer rewrites; future optimizer work items must pair verifier success with
snapshot and runtime differential tests.

## Diagnostics

Verifier errors expose stable `VerificationErrorCode` values and
`E_PHP_IR_VERIFY_*` diagnostic IDs via `VerificationError::diagnostic_id()`.
The performance layer provides:

- `E_PHP_IR_VERIFY_UNDEFINED_REGISTER_USE`;
- `E_PHP_IR_VERIFY_INVALID_CALL_ARG_METADATA`.
