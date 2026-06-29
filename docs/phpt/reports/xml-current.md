# xml Current Focus Report

Focused bounded parser harness:

| Outcome | Count |
| --- | ---: |
| PASS | 2 |
| FAIL | 0 |
| SKIP | 0 |
| BORK | 0 |

## Selected Fixtures

- `tests/phpt/generated/xml/platform-checks.phpt`
- `tests/phpt/generated/xml/parser-basic.phpt`

## Current Policy

The `xml` extension is enabled for `XMLParser`, `xml_parser_create`, and strict
in-memory `xml_parse`. It accepts single-root XML, decodes built-in entities,
and rejects unresolved entities or malformed input. SAX parser callbacks remain
unsupported under
`XML-DOM-INTL-XML-SAX-CALLBACKS`.
