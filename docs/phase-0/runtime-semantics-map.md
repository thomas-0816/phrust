# Runtime Semantics Map

Phase 0 records the runtime compatibility surface for PHP `8.5.7`. It does not
implement runtime values, object layouts, execution, extensions, or VM behavior.

## 1. Value Model

Later phases must account for:

- `null`
- `bool`
- `int`
- `float`
- `string`
- `array`
- `object`
- `resource`
- references

Key compatibility topics include Copy-on-Write, refcounting, zvals, identity,
truthiness, and cycle collection.

## 2. PHP Arrays

PHP arrays are ordered maps with int and string keys. Later implementation work
must distinguish:

- Packed/list fast paths.
- Mixed maps.
- Key normalization.
- Insertion order.
- Mutation during `foreach`.
- Interaction with references and Copy-on-Write.

## 3. References

PHP references are observable alias cells, not just pointer aliases. Later work
must model:

- `&$x`
- Alias cells.
- References inside arrays.
- References inside object properties.
- Copy-on-Write separation and reference promotion.

## 4. Type Conversions

Compatibility depends on PHP's conversion rules:

- Weak typing.
- `strict_types`.
- Numeric strings.
- Arithmetic conversions.
- Loose and strict comparison semantics.
- Warning, notice, and error behavior around conversions.

## 5. Object Model

Later object-model work must map:

- Class entries.
- Traits.
- Interfaces.
- Enums.
- Attributes.
- `readonly`.
- Property hooks.
- Asymmetric visibility.
- Magic methods.
- Late static binding.
- Visibility and inheritance checks.

## 6. Execution Model

Execution compatibility includes:

- Functions.
- Methods.
- Closures.
- Generators.
- Fibers.
- Exceptions.
- `include`, `require`, and `eval`.
- Shutdown order.
- Destructor order.
- Error handling.

## 7. Standard Library and Extensions

Core engine compatibility eventually depends on selected standard extensions:

- `ext/standard`
- `tokenizer`
- SPL
- Reflection
- DateTime
- JSON
- PCRE
- streams
- resources

Phase 0 only records this surface. Extension implementation belongs to later
phases.

## 8. Test Oracles

Reference behavior should be measured with:

- `.phpt`
- Reference CLI
- `token_get_all`
- `php -l`
- Composer smoke tests

Documentation, manuals, and RFCs are supporting material. The pinned PHP
reference behavior is authoritative when there is a conflict.

## 9. Risk Matrix

| Area | Compatibility risk | Implementation risk | Testability | Later phase |
| --- | --- | --- | --- | --- |
| Copy-on-Write and references | High | High | Medium via `.phpt` | Runtime |
| Array order and key behavior | High | High | High via fixtures and `.phpt` | Runtime |
| `foreach` mutation | High | High | Medium via targeted `.phpt` | Runtime |
| Numeric strings | High | Medium | High via value tests | Runtime |
| Object model | High | High | Medium via `.phpt` | Runtime |
| Destructor and shutdown order | High | High | Medium via CLI tests | Runtime |
| Reflection exactness | High | High | Medium via reflection `.phpt` | Runtime |
| Streams and resources | High | High | Lower without extension coverage | Later |
| Extensions | High | High | Varies by extension | Later |

## Phase 0 Boundary

This map identifies compatibility work. It does not choose Rust data
structures, implement value representation, or implement execution.
