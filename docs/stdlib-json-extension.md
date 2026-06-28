# Standard library JSON Extension MVP

Reference target: PHP 8.5.7 (`php-8.5.7`).

Work item adds the JSON extension surface required by Composer-style
configuration files and common framework metadata.

## Implemented Functions

- `json_encode`
- `json_decode`
- `json_last_error`
- `json_last_error_msg`
- `json_validate`

## Implemented Symbols

- `JsonException`
- `JsonSerializable`
- JSON error constants for `NONE`, `DEPTH`, `SYNTAX`, and `UTF8`
- Common flags including `JSON_OBJECT_AS_ARRAY`, `JSON_BIGINT_AS_STRING`,
  `JSON_PRETTY_PRINT`, `JSON_UNESCAPED_SLASHES`, `JSON_UNESCAPED_UNICODE`,
  `JSON_PRESERVE_ZERO_FRACTION`, and `JSON_THROW_ON_ERROR`

## Mapping

`json_decode` maps JSON arrays to packed PHP arrays. JSON objects map to
associative PHP arrays when the associative argument is true or
`JSON_OBJECT_AS_ARRAY` is set; otherwise they map to `stdClass` runtime
objects.

`json_encode` maps packed PHP arrays to JSON arrays, mixed PHP arrays to JSON
objects, and runtime objects to JSON objects using public runtime properties.

## Known Gaps

The MVP uses `serde_json` for strict JSON parsing and serialization. Full
byte-perfect PHP flag behavior, bigint string preservation, UTF-8 substitution
flags, and userland `JsonSerializable::jsonSerialize()` calls are tracked as
known gaps. `json_last_error` state is request-local and persists across VM
builtin dispatches.
