# Performance Numeric-String Classification Cache

The performance layer provides a conservative runtime cache under the existing
numeric-string classifier. The raw classifier remains the source of truth; the
cache stores only classification results, never diagnostics or converted
operation results.

## Scope

The cached wrapper is used by scalar conversions, comparisons, array-key
classification, and selected builtin guards:

- explicit int and float casts,
- arithmetic conversion through `to_number`,
- guarded int-compatible numeric-string arithmetic against integer operands,
- PHP 8 style loose comparisons between strings and numbers,
- string/string numeric comparisons,
- array-key normalization through the shared classifier,
- `is_numeric`, range, base-conversion, and weak arginfo builtin paths that
  already model numeric strings.

Array key normalization remains a different rule set, but it now consumes the
same classification model: canonical decimal integer strings without a leading
plus, without leading zeroes, and without surrounding whitespace become integer
keys. Whitespace, leading plus, float-looking strings, overflow-sized integers,
and `-0` remain string keys.

## Classification Model

The runtime model records:

- non numeric;
- int-compatible full numeric strings;
- float-compatible full numeric strings;
- leading numeric strings that require warning-sensitive handling;
- canonical integer strings;
- canonical float strings;
- overflow/precision-sensitive classifications, including integer overflow
  that falls back to float payloads and float strings with large significant
  digit counts or non-finite parsed payloads.

## Cache Key

The request-local key is:

- string storage identity,
- byte length,
- stable byte fingerprint.

Including the fingerprint keeps the cache legal for immutable runtime strings,
shared copy-on-write strings, and request-local strings that later become
mutable. Changed bytes cannot reuse stale classification even if the allocation
identity and length are unchanged. The cache is request-local and never persists
string bytes, diagnostics, converted operation results, or userland values.

The cache is intentionally bounded and clears itself when it reaches the small
Performance limit. That avoids unbounded retention while keeping hot-loop repeated
strings visible as hits.

## Counters

When VM counters are enabled, each execution resets the numeric-string cache and
exports:

- `numeric_string_classify_calls`
- `numeric_string_cache_hits`
- `numeric_string_cache_misses`
- `numeric_string_specialization_hits`
- `numeric_string_warning_sensitive_fallbacks`
- `numeric_string_overflow_precision_fallbacks`

The counters are harvested after execution, including runtime-error exits, so a
non-numeric arithmetic error still reports the classification miss that produced
the error without caching or delaying the diagnostic.

## Semantics

The cache stores `NumericStringKind` and parsed numeric payloads for the existing
Runtime semantics/6 classifier:

- non numeric,
- integer numeric,
- float numeric,
- leading numeric,
- whitespace-trimmed full numeric,
- integer overflow that falls back to float classification.

Locale is not consulted. The classifier remains byte-oriented and deterministic.

## Specialization Policy

The current specialization is deliberately narrow: when one arithmetic operand
is an integer and the other is a canonical, int-compatible numeric string, the
VM can execute checked integer add/subtract/multiply directly and record
`numeric_string_specialization_hits`. Division, modulo, leading numeric strings,
float-compatible strings, precision-sensitive strings, and overflow fall back to
the shared scalar helper path so warning order, diagnostics, and float fallback
behavior stay centralized.

Persistent quickening opcodes for numeric-string compare and array-key lookup
remain future work. The current array-key fast paths consume the shared
classifier for ambiguity checks and keep falling back whenever key coercion is
not proven safe.
