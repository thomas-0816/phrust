# Runtime semantics Foreach Semantics

Runtime semantics keeps foreach execution bounded to arrays plus a small object iteration
MVP while making the array, Copy-on-Write, and reference behavior explicit.

## Executable Semantics

- `foreach ($array as $value)` snapshots insertion-ordered key/value entries at
  loop entry. Later appends, unsets, and overwrites are visible after the loop
  but do not change the active by-value iteration sequence.
- `foreach ($array as $key => $value)` uses the same by-value snapshot and writes
  the current key into the key local for each iteration.
- Snapshot values are dereferenced when copied into the foreach value register,
  so by-value iteration over reference elements does not alias the element.
- `foreach ($array as &$value)` is supported when the source is a simple local
  array variable. The value local is rebound to the current array element's
  reference cell for each iteration.
- `foreach ($array as $key => &$value)` also writes the current key local before
  executing the body.
- By-reference foreach keeps the final value local bound to the final visited
  element until user code unsets or overwrites that local, matching the
  observable lingering-reference behavior covered by fixtures.
- By-reference foreach rereads local-array keys at each step, so bounded appends
  during iteration are visible to the active loop.
- `foreach ($object as $key => $value)` over a plain object iterates public
  instance properties without converting the object to an array.
- Public-property object foreach rereads property keys and values at each step,
  so covered mutations to not-yet-read properties are visible.
- Objects implementing the internal `Iterator` metadata dispatch `rewind()`,
  then repeat `valid()`, `current()`, optional `key()`, and `next()` in the
  fixture-covered PHP order.
- Objects implementing `IteratorAggregate` dispatch `getIterator()` and then
  iterate the returned array, generator MVP object, public-property object, or
  `Iterator` object.

## Known Gaps

- `E_PHP_IR_UNSUPPORTED_BY_REF_FOREACH` still covers by-reference foreach over
  temporary or nonlocal source expressions.
- Invalid non-array and non-object sources now emit fixture-covered warning and
  empty-iteration behavior when lowering reaches runtime. Unsupported
  Traversable-like edge cases are tracked under the mutation and standard
  library gaps below rather than a broad foreach-source diagnostic.
- `E_PHP_RUNTIME_ARRAYACCESS_STDLIB_GAP` covers ArrayAccess offset indexing and
  the wider SPL surface. `ArrayAccess` alone is not treated as Traversable.
- `E_PHP_RUNTIME_FOREACH_MUTATION_COMPAT` covers the complete PHP mutation
  matrix beyond the committed fixtures, including unset/reindex combinations
  during by-reference iteration and object/Iterator side effects.

## Fixture Matrix

- `fixtures/runtime_semantics/foreach/by-value-append-snapshot.php`
- `fixtures/runtime_semantics/foreach/by-value-unset-snapshot.php`
- `fixtures/runtime_semantics/foreach/by-value-modify-snapshot.php`
- `fixtures/runtime_semantics/foreach/by-value-reference-element-snapshot.php`
- `fixtures/runtime_semantics/foreach/by-ref-simple.php`
- `fixtures/runtime_semantics/foreach/by-ref-lingering.php`
- `fixtures/runtime_semantics/foreach/by-ref-key-value.php`
- `fixtures/runtime_semantics/foreach/by-ref-append-live.php`
- `fixtures/runtime_semantics/foreach/nested-foreach.php`
- `fixtures/runtime_semantics/foreach/by-ref-temporary-source-known-gap.php`
- `fixtures/runtime_semantics/foreach/object-source-known-gap.php`
- `fixtures/runtime_semantics/foreach/object-property-mutation.php`
- `fixtures/runtime_semantics/foreach/iterator-class.php`
- `fixtures/runtime_semantics/foreach/iteratoraggregate-class.php`
- `fixtures/runtime_semantics/foreach/arrayaccess-known-gap.php`
