# xmlreader

- Strategy: bounded XMLReader MVP over the shared XML tree
- Selected manifest: `tests/phpt/manifests/modules/xmlreader.selected.jsonl`
- Selected gate: 1 generated PHPT covering `XML()`, `read()`, node fields,
  `getAttribute()`, and `close()`

## Runtime Contract

- `extension_loaded("xmlreader")` returns `true`.
- `XMLReader::XML()` parses an in-memory strict XML string.
- `XMLReader::read()` advances through element, text, and end-element events.
- `nodeType`, `name`, `value`, `getAttribute()`, and `close()` are available.

## Unsupported Area

| Stable ID | Reference behavior summary | Current phrust behavior | Fixture path | Next owner layer |
| --- | --- | --- | --- | --- |
| `XML-DOM-INTL-XMLREADER-FULL-STREAM` | PHP XMLReader supports files, streams, namespaces, validation, attributes, and libxml options/errors. | Only in-memory strict XML traversal is implemented. | `tests/phpt/generated/xmlreader/basic.phpt` | future XMLReader stream/libxml layer |

## Target Gates

- `nix develop -c just phpt-module-target MODULE=xmlreader`
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=xmlreader`
- `nix develop -c just verify-phpt`
