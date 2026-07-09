# filter

- Strategy: validation and sanitization MVP
- Selected manifest: `tests/phpt/manifests/modules/filter.selected.jsonl`
- Selected target close gate: 116 PASS, 1 XFAIL, 0 SKIP, 0 FAIL, 0 BORK from
  117 selected fixtures
- Reference close gate: 117 SKIP because the sibling oracle binary does not load
  ext/filter
- Latest full module gate with reference and target reuse disabled: reference
  117 SKIP; target 116 PASS, 1 upstream XFAIL; 0 unexpected non-green outcomes
- Selected fixtures:
  - `tests/phpt/generated/filter/basic.phpt`
  - `tests/phpt/generated/filter/arrays.phpt`
  - `tests/phpt/generated/filter/options-callback.phpt`
  - 114 selected upstream rows from `ext/filter/tests`

## Implemented Surface

`filter_var` covers `FILTER_DEFAULT`/`FILTER_UNSAFE_RAW`, `FILTER_VALIDATE_EMAIL`,
`FILTER_VALIDATE_URL`, `FILTER_VALIDATE_INT`, `FILTER_VALIDATE_FLOAT`,
`FILTER_VALIDATE_BOOLEAN`, `FILTER_SANITIZE_EMAIL`, and
`FILTER_SANITIZE_URL`. It also handles `FILTER_SANITIZE_SPECIAL_CHARS` with
PHP-style decimal entities, `FILTER_REQUIRE_ARRAY`, `FILTER_FORCE_ARRAY`,
`FILTER_REQUIRE_SCALAR`, and `FILTER_SANITIZE_NUMBER_FLOAT` with the common
fraction, thousand, and scientific flags. Required-array filtering recurses into
nested arrays while applying scalar filters to scalar leaves.

The selected slice also includes `filter_has_var`, `filter_input_array`,
`filter_var_array`, `filter_list`, and `filter_id` for common request and
metadata paths. Array filter specs support integer filter IDs plus nested
`filter`, `flags`, and `options` entries. `FILTER_VALIDATE_INT` and
`FILTER_VALIDATE_FLOAT` honor `min_range` and `max_range` option arrays.
`FILTER_VALIDATE_INT` also follows PHP's accepted hexadecimal and octal
forms, including unsigned-prefixed overflow wrapping for the
`FILTER_FLAG_ALLOW_HEX` and `FILTER_FLAG_ALLOW_OCTAL` paths, and returns the
promoted `options["default"]` value for failed validation and missing
`filter_input` values. The standard registry exposes the php-src filter flag
constant surface used by upstream PHPT option arrays.

`FILTER_VALIDATE_EMAIL` enforces PHP's local-part dot-atom, quoted local-part,
Unicode flag, address literal, numeric-TLD, total length, label, and
domain-label hyphen-edge behavior for the promoted fixtures. Validation filters
return the normal failure/default value for non-stringable objects instead of
fataling during scalar conversion, matching the promoted object-input
regression rows.
`FILTER_VALIDATE_BOOLEAN` returns the normal failure value when object string
conversion is invalid.

`FILTER_VALIDATE_REGEXP` compiles php-src style PCRE patterns from the
`options["regexp"]` array entry, returns the original string on match, returns
the filter failure value on mismatch or invalid pattern, and raises the
PHP-compatible `ValueError` message when the regexp option is missing.

`FILTER_VALIDATE_MAC` accepts the promoted hyphen, colon, and dotted EUI-64
forms, rejects mixed separators and non-hex tokens, honors
`options["separator"]`, and raises the PHP-compatible `ValueError` message for
empty or multi-character separators.

`FILTER_VALIDATE_DOMAIN` accepts the promoted hostname and non-hostname domain
forms, ignores a final root-label dot for length validation, enforces the
253-byte total domain limit and 63-byte label limit, applies
`FILTER_FLAG_HOSTNAME` alphanumeric edge rules, and preserves PHP's non-hostname
underscore acceptance.

`FILTER_VALIDATE_IP` honors the promoted IPv4/IPv6 version flags plus PHP's
distinct `FILTER_FLAG_NO_PRIV_RANGE`, `FILTER_FLAG_NO_RES_RANGE`, and
`FILTER_FLAG_GLOBAL_RANGE` semantics, including IPv4 RFC 6890 edge ranges,
IPv6 unique-local/link-local/mapped ranges, and global-only filtering.

`FILTER_VALIDATE_FLOAT` supports the promoted custom `decimal` option,
including comma decimal parsing, rejection when the input still uses `.`, and
the PHP-compatible `ValueError` for multi-character decimal separators. It also
supports the promoted custom `thousand` option when
`FILTER_FLAG_ALLOW_THOUSAND` is set and raises PHP's empty-thousand
`ValueError`. The selected rows reject malformed thousand groups and float
underflow to zero, and cover array range validation plus
`options["default"]` returns.

`FILTER_UNSAFE_RAW` preserves scalar input while honoring
`FILTER_FLAG_ENCODE_AMP`, `FILTER_FLAG_ENCODE_LOW`, and
`FILTER_FLAG_ENCODE_HIGH` with PHP-style decimal entities. High-byte handling
uses PHP's `0x7f` threshold, so `FILTER_FLAG_STRIP_HIGH` removes ASCII DEL in
the promoted raw, encoded, string, and special-character sanitizer paths.
`FILTER_FLAG_STRIP_BACKTICK` removes backticks from `FILTER_UNSAFE_RAW`
independently of the low/high strip flags.
`FILTER_FLAG_EMPTY_STRING_NULL` turns empty `FILTER_DEFAULT`/`FILTER_UNSAFE_RAW`
results into `NULL`, including empty results produced by stripping.
`FILTER_SANITIZE_ENCODED` percent-encodes non-safe bytes using PHP's promoted
safe byte set, preserving ASCII alphanumerics plus `-`, `_`, and `.`.
`FILTER_SANITIZE_SPECIAL_CHARS` emits PHP-style decimal entities for quotes,
angle brackets, and ampersands. `FILTER_SANITIZE_FULL_SPECIAL_CHARS` emits the
promoted named entity form with `&#039;` for apostrophes and honors
`FILTER_FLAG_NO_ENCODE_QUOTES`.
`FILTER_SANITIZE_ADD_SLASHES` covers quote, backslash, and NUL escaping.
`filter_var_array` covers the promoted encoded sanitizer and scalar/array
shape interactions. It also matches the promoted php-src rows for unknown
single-filter options returning `false`, unknown per-key specs warning while
preserving the original value, invalid options type errors, and empty spec-key
`ValueError` handling. In single-filter mode, it recurses into nested array
values and writes filtered results back through referenced elements while
preserving the reference shape returned to userland.

`filter_input` preserves the PHP missing-input distinction for the promoted
fixtures, including returning `false` for absent values when
`FILTER_NULL_ON_FAILURE` is set. `filter_has_var` raises the PHP-compatible
`ValueError` when the input source is not one of the public `INPUT_*`
constants. `filter_input_array` returns `NULL` for an empty request input source
before expanding add-empty spec entries. The request input map exposes
`INPUT_ENV` from the runtime environment for `filter_input`.

`filter_list` follows php-src's public filter-name ordering, including the
`validate_domain` position before URL, email, IP, and MAC validators.
`FILTER_VALIDATE_URL` validates the promoted scheme matrix, authority host
labels, numeric port range, path-required flag, and query-required flag,
including hyphenated hostnames while rejecting underscores in hostname labels.
The promoted URL security rows also reject backslashes and bracket characters
in userinfo/authority positions that would confuse host parsing while still
accepting valid userinfo with bracketed IPv6 hosts.

`FILTER_CALLBACK` is exposed in `filter_list`, `filter_id`, and the standard
constant registry. The runtime executes callback filters for registered builtin
string callables such as `strtoupper`. The VM-dispatched filter path executes
user function, closure, and static method callbacks for `filter_var`,
`filter_input`, and `filter_input_array`, preserves `FILTER_REQUIRE_SCALAR`
failure behavior, recurses arrays by default for callback filters, emits
PHP-compatible by-reference callback warnings, raises catchable `TypeError` for
invalid callback options, and propagates thrown callback exceptions through PHP
`try/catch`.

Unsupported filter identifiers emit the PHP-style unknown-filter warning and
return the normal filter failure value (`false` or `null` with
`FILTER_NULL_ON_FAILURE`) instead of accepting unknown behavior.

The CLI startup path emits the upstream `filter.default` deprecation diagnostic
when PHPT-style startup error display is enabled. The diagnostic is suppressed
when `display_errors=0`, and `ini_get()` exposes the promoted
`filter.default` and `filter.default_flags` values from PHPT-style INI
overrides.

Deprecated filter constants carry registry metadata and the VM emits the
PHP-compatible `E_DEPRECATED` diagnostic when `FILTER_SANITIZE_STRING` and
`FILTER_SANITIZE_STRIPPED` are accessed at runtime. The promoted rows cover the
legacy string sanitizer warning surface plus strip-low, strip-high, and
nested tag-stripping combinations that are now green.

The selected upstream set also covers `FILTER_VALIDATE_INT` request input and
array/scalar flag combinations plus additional promoted URL, email, and scalar
filter regression rows that the existing implementation already satisfies.

PHPT CLI request fixtures populate `$_GET`, `$_POST`, `$_COOKIE`, and
`$_REQUEST` from the harness environment, including `filter.default` request
input handling for promoted `special_chars` and stripped cases. The startup
deprecation for `filter.default` is suppressed when `error_reporting` masks
`E_DEPRECATED` or `display_errors=0`, matching the promoted php-src rows.
Duplicate cookie names preserve the first cookie value within the header while
the first cookie still overrides earlier GET/POST values in `$_REQUEST`.
`filter_input()` and `filter_input_array()` read raw request snapshots instead
of the filtered superglobal values.

## Gaps

The full PHP filter option matrix, remaining exact filter flag behavior,
remaining request input edge cases, throw-on-failure mode, and locale-specific
numeric parsing remain out of scope.

The latest full module gate measured 117 SKIP on the local sibling oracle,
which does not load ext/filter, and 116 PASS plus 1 upstream `--XFAIL--` row
on the target from 117 selected rows. All upstream filter rows are represented
in the selected manifest; `bug49184.phpt` and `bug67167.02.phpt` are locally
promoted upstream-XFAIL rows because the target now matches their EXPECT
sections, while `bug42718.phpt` remains an upstream expected-failure row.

## Target Gates

- `nix develop -c cargo test -p php_runtime filter`
- `nix develop -c cargo test -p php_std filter`
- `nix develop -c cargo test -p php_server server_filter_input_array_reads_query_snapshot`
- `PHPT_TIMEOUT_SECONDS=60 nix develop -c just phpt-dev-module MODULE=filter`
