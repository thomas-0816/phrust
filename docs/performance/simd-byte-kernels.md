# SIMD and Byte-Kernel Facade

`php_source::byte_kernel` provides the shared byte-kernel facade for source,
runtime string, HTML, and JSON scanning. The public API is safe, byte-oriented,
and intentionally narrow:

- find one byte;
- find one byte in reverse;
- find any of two or three bytes;
- find a byte subslice from an offset;
- find a byte subslice in reverse;
- find ASCII case-insensitive byte subslices forward and backward;
- count one byte;
- count PHP source line breaks (`\n`, `\r\n`, and standalone `\r`);
- detect all-ASCII byte slices;
- classify ASCII identifier-continuation chunks;
- classify ASCII digit runs and all-digit byte slices;
- find ASCII whitespace/non-whitespace forward and whitespace in reverse;
- ASCII-only lowercase and uppercase copy/in-place helpers;
- find the first JSON string escape byte;
- find the first default HTML escape byte;
- find default `trim()` bounds.

The general byte search helpers use `memchr` behind the facade. Hot
classification, transform, escape-scan, and default-trim helpers additionally
have architecture backends selected inside the facade:

- `x86_64`: AVX2 when available at runtime, otherwise SSE2.
- `aarch64`: NEON.
- other targets: scalar reference implementation.

Every optimized helper has a scalar reference function and parity tests that
cover empty inputs, short inputs, vector-width-adjacent lengths, large inputs,
all-byte sweeps where relevant, delimiter positions, and invalid UTF-8 byte
sequences.

## Policy

The public API exposes no unsafe functions. Architecture-specific code stays in
private modules and must keep scalar reference parity tests. Runtime dispatch is
an implementation detail; callers depend only on PHP-visible byte semantics.

The facade does not change PHP-visible behavior by itself. Lexer, source-map,
runtime string, and output call sites opt in with token/span/diagnostic or
runtime parity evidence.

SIMD and byte kernels accelerate byte-heavy loops. They do not replace VM
semantic optimization, interpreter feedback, inline caches, superinstructions,
or PHP runtime helpers.

## FPE-03 Integration

The first call-site integration uses the facade in source and lexer code only:

- `LineIndex` jumps between LF/CR bytes with `find_any2` while preserving the
  existing CRLF-as-one-line rule.
- The lexer cursor now supports safe byte-count advancement after line
  accounting, avoiding per-byte cursor bumps for bulk spans.
- Inline HTML skips to the next `<` byte before rechecking PHP open-tag shapes.
- Line comments jump to the next newline or `?` byte, then preserve the existing
  `?>` close-tag stop condition.
- Block comments jump to the next `*` byte before checking for `*/`.
- Identifier consumers use `ascii_identifier_continue_chunk_len` for ASCII runs
  and keep the previous byte-by-byte handling for PHP non-ASCII identifier
  bytes.
- Constant single- and double-quoted string scanning uses byte-kernel delimiter
  search while preserving escape and interpolation handling.

Skipped loops remain intentionally conservative where stop conditions depend on
interpolation state, heredoc indentation/labels, numeric literal separators, or
PHP whitespace/cast grammar. Those loops should move only with focused parity
tests and benchmark evidence.

Current benchmark support for this layer is the local advisory
`just bench-lexer` Criterion-style throughput smoke. It is not a compatibility
gate and should not be used for standalone speed claims.

## Runtime Integration

The second integration pass extends byte kernels from the frontend into runtime
string and serialization hot paths:

- `strpos`/`strrpos`/`strstr`/`stristr`/related runtime searches use the
  shared byte, subslice, reverse-subslice, and ASCII-folded search helpers.
- Default `trim()` uses SIMD-capable trim-bound discovery while custom masks
  remain on the generic mask table.
- `strtolower`, `strtoupper`, and `substr_compare(..., case_insensitive: true)`
  share the ASCII case kernels.
- Numeric-string classification, `parse_url` port parsing, `version_compare`
  numeric parts, and `ctype_digit`/`ctype_space` use byte-kernel digit and
  whitespace classifiers.
- `wordwrap` whitespace search and filter/email/URL whitespace rejection use
  the shared whitespace search helpers.
- PHAR halt-marker scans, cURL HTTP delimiter scans, stream line reads, form
  split helpers, and HTML entity semicolon/digit checks use byte/subslice
  kernels instead of ad hoc scalar `position`/`windows` loops.
- Default `htmlspecialchars` pre-scans with the HTML escape classifier and
  shares unchanged input storage when no escape byte exists.
- Single-byte `explode` pre-counts separators and scans with byte kernels.
- JSON string encoding uses the JSON escape classifier to copy all-ASCII,
  no-escape strings through the exact fast path before falling back to the
  existing Unicode/error-aware encoder.

`crates/php_bench/benches/perf_hotpaths.rs` now includes advisory Criterion
coverage for forward/reverse byte search, ASCII-folded substring search, byte
count, digit/whitespace classification, JSON/HTML escape scans, ASCII case
copy, default trim bounds, and the runtime string intrinsic consumers. These
benchmarks are local performance evidence only; correctness remains owned by
the focused unit, runtime, VM, stdlib, and source-integrity gates.
