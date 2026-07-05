# Reference Aliasing Deoptimization Policy

Date: 2026-06-28.

FPE-22 adds a conservative alias-state model for PHP references. It does not
optimize through references yet; it classifies aliasing and poisons the affected
fast paths so reference-heavy behavior stays on the generic interpreter path.

## Alias Classes

The VM exports `php_vm::AliasState` with stable report labels:

- `no_references_observed`: frame activation before any reference-producing
  operation is seen.
- `local_only_reference`: local `=&` aliases that have not been proven to escape.
- `escaped_reference`: by-reference parameters, returns, closure captures,
  static-local cells, or value-level references.
- `global_or_superglobal_reference`: `global` bindings and top-level/global
  symbol-table backed locals.
- `property_or_array_dim_reference`: array element, foreach-by-reference, or
  property-slot reference state.
- `unknown_aliasing`: heap or bytecode states where the current model cannot
  prove a narrower class.

Unknown aliasing is reference-sensitive and fails closed.

## Counters

Counter JSON now reports:

- `frame_alias_state`
- `alias_state_transitions`
- `fast_path_disabled_by_reference`
- `dequickened_by_reference`
- `IC_invalidated_by_reference`
- `dense_bytecode_fallback_by_reference`

`scripts/performance/perf_report.py` includes these counters in the frame and
framework summaries and flattens `frame_alias_state` plus
`alias_state_transitions` for aggregate reports.

## Integration Points

- Frame activation records `no_references_observed`.
- Local `=&`, `global`, static-local, by-reference parameter, by-reference
  return, closure-capture, array-dimension reference, and foreach-by-reference
  paths record alias-state transitions.
- Packed-array fast paths and the existing COW/reference fallback counter record
  `fast_path_disabled_by_reference`.
- Quickened packed-array reads over reference-bearing arrays record
  `dequickened_by_reference` before falling back.
- Property assignment IC reference-slot and reference-metadata exits record
  `IC_invalidated_by_reference`.
- Dense bytecode strict/auto fallback reasons containing reference, by-ref, or
  COW state record `dense_bytecode_fallback_by_reference`.
- Native eligibility reports annotate by-reference params, returns, captures,
  and reference-producing opcodes with the alias class that blocks native code.

## Fixtures

VM tests cover:

- simple no-reference hot path;
- local reference creation;
- by-reference parameter;
- by-reference return;
- array element reference;
- globals;
- unset and rebinding;
- foreach by reference;
- reference escape through closure capture.

Object property references remain a known frontend/runtime gap with a stable
diagnostic fixture (`E_PHP_IR_UNSUPPORTED_PROPERTY_REFERENCE`).

## Disabled Fast Paths

The following remain disabled whenever aliasing is observed or uncertain:

- packed-array read/append/foreach shortcuts over reference-bearing or COW
  storage;
- property assignment IC writes into reference slots;
- call-frame reuse for by-reference parameters;
- dense bytecode lowering/execution for reference/COW/by-ref states;
- native/JIT eligibility for by-reference params, returns, captures, and
  reference-producing opcodes.

## Gate Classification

Per `docs/performance-optimization-gates.md`:

- Optimizing through `escaped_reference`, `global_or_superglobal_reference`,
  `property_or_array_dim_reference`, and `unknown_aliasing` stays blocked
  (`HARD_BLOCK` for native/JIT consumption, generic-interpreter-only
  otherwise).
- `no_references_observed` paths and proven local/location-based interpreter
  paths are `SUBSET_ALLOWED`.
- By-ref argument location encoding — binding a callee by-ref parameter to
  the caller's slot instead of materializing the argument as a value
  register — is explicitly `SUBSET_ALLOWED` when the binding produces the
  same reference cells as the generic binder and unsupported shapes fall
  back with recorded reasons.

Future work may specialize `local_only_reference`, but only after reference
identity and write-through behavior have focused differential fixtures.
