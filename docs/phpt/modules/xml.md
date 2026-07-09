# xml PHPT Coverage

## Strategy

The xml slice is a bounded parser MVP. It covers platform visibility,
`XMLParser` object creation, strict parser success/failure state, current
position helpers, selected parser option retention, selected SAX callback
dispatch, and selected `xml_parse_into_struct()` flattening. Full libxml2
object/resource parity remains outside the selected gate.

## Selected Rows

- `tests/phpt/generated/xml/platform-checks.phpt`
- `tests/phpt/generated/xml/parser-basic.phpt`
- `tests/phpt/generated/xml/parser-error-state.phpt`
- `tests/phpt/generated/xml/parser-current-position.phpt`
- `tests/phpt/generated/xml/constants-options.phpt`
- `tests/phpt/generated/xml/sax-handlers.phpt`
- `tests/phpt/generated/xml/parse-into-struct.phpt`
- `ext/xml/tests/xml_parser_get_option_variation3.phpt`
- `ext/xml/tests/xml_parser_set_option_basic.phpt`
- `ext/xml/tests/xml_parser_free_deprecated.phpt`
- `ext/xml/tests/bug78563_serialize.phpt`

## Implemented Surface

- `extension_loaded('xml')`, `class_exists('XMLParser')`, and selected
  `function_exists` visibility.
- `xml_parser_create()` and `xml_parse()` over the strict parser MVP.
- `xml_get_error_code()`, `xml_error_string()`,
  `xml_get_current_byte_index()`, `xml_get_current_line_number()`, and
  `xml_get_current_column_number()` for deterministic selected parser state.
- Built-in XML entity decoding for selected valid documents.
- Rejection and stable error reporting for selected invalid XML and unresolved
  entity inputs.
- Selected `xml_parser_get_option()` and `xml_parser_set_option()` rows for
  option retention and return-value behavior, including
  `XML_OPTION_PARSE_HUGE` value retention.
- Selected `XML_OPTION_PARSE_HUGE` and `XML_SAX_IMPL` constant visibility.
- Selected `xml_parser_free()` no-op return behavior and PHP 8.5
  deprecation output.
- Selected `XMLParser` `serialize()` rejection with PHP's not-serializable
  exception message.
- Selected `xml_set_element_handler()`,
  `xml_set_character_data_handler()`, and `xml_set_default_handler()`
  registration and dispatch over the strict parser MVP, including default case
  folding for element callbacks and source-case preservation for default
  callbacks.
- Selected `xml_parse_into_struct()` values/index array flattening for open,
  complete, cdata, and close records, including selected case-folding behavior.

## Current Gate

The selected xml module gate is policy-green with 11 selected rows. In the
current local php-src oracle build, reference rows skip because the xml
extension is not loaded; the target runtime reports 11 PASS.

```text
REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php \
PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src \
PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 PHPT_DISABLE_REFERENCE_REUSE=1 \
PHPT_TIMEOUT_SECONDS=20 \
PHPT_WORK_DIR=/private/tmp/phrust-phpt-xml-selected-current \
nix develop -c just phpt-dev-module MODULE=xml
```

A temporary target-only upstream sweep was also run from a generated manifest
outside the repository:

```text
target/debug/php-phpt-tools run \
  --manifest /private/tmp/phrust-xml-manifest-current/xml-originals.jsonl \
  --out /private/tmp/phrust-phpt-xml-originals-current/results.jsonl
```

That sweep reported 67 upstream ext/xml rows: PASS 2 / SKIP 5 / FAIL 60. No
additional target-green rows were available beyond the two selected upstream
parser option rows.

## Remaining Gaps

- Full SAX callback edge-case parity, full `xml_parse_into_struct` edge cases,
  namespace-aware SAX behavior, parser recursion diagnostics, and
  `xml_set_object` deprecation behavior remain unpromoted.
- `XMLParser` final/uncloneable object semantics beyond the selected
  `xml_parser_free()` no-op path and `serialize()` rejection are not yet
  matched.
- `XML_OPTION_PARSE_HUGE` long-name parser behavior, broad encoding parity,
  depth-limit parity, and full libxml2 parser option/security behavior remain
  outside the selected gate.
