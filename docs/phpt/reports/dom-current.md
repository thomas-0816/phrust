# dom Current Focus Report

Focused bounded DOM harness:

| Outcome | Count |
| --- | ---: |
| PASS | 3 |
| FAIL | 0 |
| SKIP | 0 |
| BORK | 0 |

## Selected Fixtures

- `tests/phpt/generated/dom/platform-checks.phpt`
- `tests/phpt/generated/dom/domdocument-basic.phpt`
- `tests/phpt/generated/dom/domdocument-node-apis.phpt`

## Current Policy

The `dom` extension is enabled for `DOMDocument`, `DOMElement`, `DOMNode`, and
`DOMNodeList` backed by the shared strict XML tree. The selected slice covers
XML load/save, constructed document roots, element-local attributes and append,
node properties, and countable/iterable `getElementsByTagName()` results. XPath,
full live DOM node ownership, namespaces, HTML parsing, schema validation, and
libxml error state remain documented gaps.
