# xml

- Strategy: bounded parser MVP
- Classification: optional, enabled for the local WordPress XML slice
- Selected manifest: `tests/phpt/manifests/modules/xml.selected.jsonl`
- Selected gate: 2 generated PHPTs covering platform visibility and strict
  parse/reject behavior

## Runtime Contract

- `extension_loaded("xml")` returns `true`.
- `xml_parser_create()` returns a bounded `XMLParser` object.
- `xml_parse(XMLParser $parser, string $data, bool $is_final = false)` returns
  `1` for a strict single-root XML document and `0` for malformed XML.
- Built-in XML entities are decoded. Unresolved entities, DTDs, processing
  instructions beyond the XML declaration, and trailing content are rejected.
- The PHP SAX parser API remains unsupported.

## Required PHPTs

- `tests/phpt/generated/xml/platform-checks.phpt`
- `tests/phpt/generated/xml/parser-basic.phpt`

## Unsupported Area

| Stable ID | Reference behavior summary | Current phrust behavior | Fixture path | Next owner layer |
| --- | --- | --- | --- | --- |
| `XML-DOM-INTL-XML-SAX-CALLBACKS` | PHP `ext/xml` exposes parser callbacks, parser options, and position/error constants. | Only `XMLParser`, `xml_parser_create`, and strict `xml_parse` are implemented; SAX callbacks are absent. | `tests/phpt/generated/xml/platform-checks.phpt` | future XML parser resource layer |
| `XML-DOM-INTL-LIBXML-ERROR-STATE` | libxml reports structured parse diagnostics and global error state. | Parse failures are deterministic boolean false or runtime errors; no libxml error buffer is modeled. | `tests/phpt/generated/xml/parser-basic.phpt` | future libxml compatibility layer |

## Target Gates

- `nix develop -c just phpt-module-target MODULE=xml`
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=xml`
- `nix develop -c just verify-phpt`
