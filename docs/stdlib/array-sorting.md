# Standard library Array Sorting Functions

The standard library implements deterministic MVP sorting helpers:

- `sort`, `rsort`
- `asort`, `arsort`
- `ksort`, `krsort`
- `usort`, `uasort`, `uksort`
- `natsort`, `natcasesort`

Execution is VM-routed because all functions mutate their first array argument
by reference and the `u*sort` variants invoke user callbacks. The runtime
registry exposes the names for function lookup and introspection, while the VM
binds the first argument to the caller local and writes the sorted array back.

Covered behavior:

- packed reindexing for `sort`, `rsort`, and `usort`
- key preservation for associative sorts and natural sorts
- regular PHP comparison for value and key sorts
- callback comparator dispatch with runtime-error propagation
- deterministic ASCII natural sorting for common filename-like strings

Known gap:

- `STDLIB-GAP-NATURAL-SORT-EDGE-CASES`: natural sorting is not byte-perfect for
  all Zend leading-zero, locale-sensitive, or `strnatcmp` edge cases.

Validation:

- `nix develop -c cargo test -p php_vm array_sort_builtins_mutate_arrays_and_call_comparators`
- `nix develop -c just diff-stdlib`
- `nix develop -c just performance-tests`
- `nix develop -c just verify-stdlib`
