# Standard library Serialization MVP

Reference target: PHP 8.5.7 (`php-8.5.7`).

The standard library implements bounded `serialize` and `unserialize` support for:

- `null`, `bool`, `int`, `float`, and binary strings
- arrays with integer and string keys
- simple objects as class-name plus public runtime properties
- runtime `ReferenceCell` values by serializing the effective value
- malformed-input handling without panics

Reference identity records are intentionally out of scope for this scope:
`serialize` does not emit PHP `R`/`r` records, and `unserialize` rejects them
as `STDLIB-GAP-SERIALIZE-REFERENCES` rather than fabricating aliases.

`unserialize` uses deterministic security limits:

- maximum recursive depth: 64
- maximum parsed array/object entries: 16,384
- maximum input bytes: 1,048,576

Invalid serialized input returns `false` from the PHP-visible builtin and emits
the PHP-style malformed-offset warning. The internal parser returns a structured
`SerializationError` for tests and future diagnostics.

Known gaps are tracked in `docs/stdlib/known-gaps.md`: `allowed_classes`,
serialized `R`/`r` reference records, resource payloads, magic methods, and
full object hook behavior are not implemented in this scope.
