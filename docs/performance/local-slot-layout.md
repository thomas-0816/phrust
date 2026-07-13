# Performance Local Slot Layout

The performance layer is implemented on top of the existing Runtime VM frame model.
Compiled variables are already addressed by `LocalId` and stored in
`Frame.locals: LocalFile`, where `LocalFile` is a fixed `Vec<Slot>`.

## Fast Path

Simple local-variable bytecode uses the compiled slot index directly:

- `LoadLocal` and `LoadLocalQuiet` read `LocalFile::get(LocalId)`.
- `StoreLocal` writes `LocalFile::set(LocalId, Value)`.
- `BindReference`, `BindReferenceDim`, `IssetLocal`, `EmptyLocal`,
  `UnsetLocal`, and local dim operations use the same compiled slot bounds.

These operations do not perform a name hash lookup. Performance adds explicit
counter instrumentation around those accesses:

- `local_slot_fast_path_hits`
- `local_slot_fast_path_misses`

A hit means the current frame exists and the compiled `LocalId` is in bounds.
A miss means the instruction used a dynamic-symbol-table path or the slot was
not available, after which the existing runtime behavior still decides whether
to return a value, emit diagnostics, or raise an invalid-local error.

## Fallback Boundaries

The VM does not optimize across PHP-visible symbol-table exposure:

- `$GLOBALS` local reads route through the globals array and count as misses.
- `global $x` binds a local to the global symbol table and counts as a miss.
- Superglobal/global access remains observable through the existing globals
  storage.
- Reference binding, closure `use`, and by-reference params continue to use the
  current runtime paths. Fixed-slot reads and writes around those features are
  counted only when the compiled local slot remains directly addressable.
- Variable variables do not currently reach the VM as a lowered HIR/IR form.
  They remain a frontend boundary; once lowered, they must not use the fixed
  `LocalId` path unless the resolved name is proven stable.
- `extract` and `compact` are not implemented as VM builtins in this work.
  Any future implementation must treat them as symbol-table exposure and count
  the dynamic path as a local-slot miss.

`isset`, `unset`, and references are unchanged. The counters are observational:
they do not change slot contents, reference cells, diagnostics, or control flow.
