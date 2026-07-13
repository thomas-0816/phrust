# Standard library Array Basics

Reference target: PHP 8.5.7 (`php-8.5.7`).

The standard library implements read-oriented array helpers:

- Count and aliases: `count`, `sizeof`.
- Key and value helpers: `array_key_exists`, `array_keys`, `array_values`,
  `array_is_list`, `array_key_first`, `array_key_last`.
- Search helpers: `in_array`, `array_search` with strict and loose matching.
- `array_column` MVP for array rows with integer/string/null column and index
  keys.

The implementation builds new `PhpArray` values and does not mutate the input
array, preserving Runtime semantics COW/reference invariants for these read helpers.
