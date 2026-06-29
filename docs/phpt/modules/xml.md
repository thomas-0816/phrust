# xml

- Strategy: bounded parser MVP
- Classification: optional, enabled for the local WordPress XML slice
- Selected manifest: `tests/phpt/manifests/modules/xml.selected.jsonl`
- Selected gate: 3 generated PHPTs covering platform visibility, strict
  parse/reject behavior, and parser error helpers

## Runtime Contract

- `extension_loaded("xml")` returns `true`.
- `xml_parser_create()` returns a bounded `XMLParser` object.
- `xml_parse(XMLParser $parser, string $data, bool $is_final = false)` returns
  `1` for a strict single-root XML document and `0` for malformed XML.
- `xml_get_error_code()` and `xml_error_string()` expose deterministic parser
  error state for the selected malformed-input slice.
- Built-in XML entities are decoded. Unresolved entities, DTDs, processing
  instructions beyond the XML declaration, and trailing content are rejected.
- The PHP SAX parser API remains unsupported.

## Required PHPTs

- `tests/phpt/generated/xml/platform-checks.phpt`
- `tests/phpt/generated/xml/parser-basic.phpt`
- `tests/phpt/generated/xml/parser-error-state.phpt`

## Unsupported Area

| Stable ID | Reference behavior summary | Current phrust behavior | Fixture path | Next owner layer |
| --- | --- | --- | --- | --- |
| `XML-DOM-INTL-XML-SAX-CALLBACKS` | PHP `ext/xml` exposes parser callbacks, parser options, and position constants. | `XMLParser`, `xml_parser_create`, strict `xml_parse`, and selected error helpers are implemented; SAX callbacks are absent. | `tests/phpt/generated/xml/platform-checks.phpt` | future XML parser resource layer |
| `XML-DOM-INTL-LIBXML-ERROR-STATE` | libxml reports structured parse diagnostics and global error state. | Parse failures expose a deterministic selected error code/string, but no full libxml error buffer is modeled. | `tests/phpt/generated/xml/parser-error-state.phpt` | future libxml compatibility layer |

## Target Gates

- `nix develop -c just phpt-module-target MODULE=xml`
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=xml`
- `nix develop -c just verify-phpt`
