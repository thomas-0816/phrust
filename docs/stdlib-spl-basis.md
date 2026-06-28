# Standard library SPL Basis

Reference target: PHP 8.5.7 (`php-8.5.7`).

Work item enables the SPL extension by default for the Standard library runtime
registry and exposes the Composer-facing SPL basis:

- `spl_autoload_register`
- `spl_autoload_unregister`
- `spl_autoload_functions`
- `spl_autoload_call`
- `iterator_count`
- `iterator_to_array`
- `spl_object_id`
- `spl_object_hash`
- `Traversable`, `Iterator`, `IteratorAggregate`, `ArrayAccess`, `Countable`,
  `SeekableIterator`, `RecursiveIterator`, and `Serializable`
- SPL `LogicException` and `RuntimeException` hierarchy classes

Autoload stack state remains owned by the VM execution state. Runtime builtin
registry entries exist for symbol discovery, but direct non-VM calls to the
autoload functions return a deterministic VM-context-required diagnostic.

`spl_object_id` and `spl_object_hash` are standalone runtime builtins backed by
the stable runtime object identity. The hash is a deterministic 32-character
lowercase hexadecimal rendering of that identity.

The VM accepts SPL exception classes as internal throwable catch types and maps
their parent hierarchy for catch matching and `instanceof`.

Work item adds an internal-object MVP for core SPL iterator classes:

- `ArrayIterator`
- `RecursiveArrayIterator`
- `IteratorIterator`
- `LimitIterator`
- `EmptyIterator`
- `AppendIterator`

The MVP snapshots array/object sources into runtime iterator objects and supports
the methods required by Standard library foreach interop: `rewind()`, `valid()`,
`current()`, `key()`, `next()`, `count()`, `getArrayCopy()`, and
`AppendIterator::append()`/`addIterator()`. The VM recognizes these objects for
foreach, `instanceof` checks against the relevant SPL interfaces/classes
including `SeekableIterator`/`RecursiveIterator` where selected fixtures require
them, `count()` over Countable iterator MVP objects, and MVP
`iterator_count()`/`iterator_to_array()` support for arrays and the same
Traversable object path.

Work item adds Composer/framework-facing SPL container MVPs:

- `ArrayObject`
- `SplFixedArray`
- `SplObjectStorage`
- `SplDoublyLinkedList`
- `SplStack`
- `SplQueue`

`ArrayObject` and `SplFixedArray` support one-dimensional ArrayAccess reads and
writes, Countable behavior, foreach iteration, and their common storage methods.
`SplObjectStorage` stores attached objects by runtime object identity and
supports attach/detach/contains/count/foreach plus info access through the
method API. The list/stack/queue classes use simple internal vector storage for
push/pop/shift/unshift/top/bottom/count and foreach.

Work item adds SPL file class MVPs:

- `SplFileInfo`
- `SplFileObject`
- `SplTempFileObject`

`SplFileInfo` exposes path, basename, realpath, size, mtime, and simple file/dir
predicates through the VM's existing root-constrained filesystem capability
policy. `SplFileObject` loads allowed local files into deterministic line
storage and supports `fgets()`, `fgetcsv()` with a simple delimiter MVP,
`rewind()`, `eof()`, and foreach over lines. `SplTempFileObject` exposes an empty
`php://temp`-style in-memory MVP for path and size checks.

## Known Gaps

The following gaps are tracked in `docs/stdlib-known-gaps.md`:

- `STDLIB-GAP-SPL-INTERFACE-METHOD-SURFACES`
- `STDLIB-GAP-SPL-AUTOLOAD-ADVANCED`
- `STDLIB-GAP-SPL-OBJECT-HASH-PARITY`
- `STDLIB-GAP-SPL-ITERATOR-MUTATION-EDGES`
- `STDLIB-GAP-SPL-ITERATOR-FULL-API`
- `STDLIB-GAP-SPL-CONTAINER-FULL-API`
- `STDLIB-GAP-SPL-CONTAINER-NESTED-ARRAYACCESS`
- `STDLIB-GAP-SPL-FILE-FULL-API`
- `STDLIB-GAP-SPL-FILE-CSV-FLAGS`
