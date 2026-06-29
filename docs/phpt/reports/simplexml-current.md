# simplexml Current Focus Report

Focused bounded SimpleXML harness:

| Outcome | Count |
| --- | ---: |
| PASS | 3 |
| FAIL | 0 |
| SKIP | 0 |
| BORK | 0 |

## Selected Fixtures

- `tests/phpt/generated/simplexml/platform-checks.phpt`
- `tests/phpt/generated/simplexml/simplexml-basic.phpt`
- `tests/phpt/generated/simplexml/wordpress-snippets.phpt`

## Current Policy

The `simplexml` extension is enabled for `simplexml_load_string`,
`SimpleXMLElement`, text conversion, child access, attributes, iteration, and
`asXML()` over the shared strict XML tree. The selected slice includes
WordPress-style RSS title/item reads, plugin metadata attributes, and simple
config option iteration. Namespaces, XPath, DOM import, file loading, and
libxml error state remain documented gaps.
