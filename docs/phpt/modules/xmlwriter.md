# xmlwriter

- Strategy: bounded in-memory XMLWriter MVP
- Selected manifest: `tests/phpt/manifests/modules/xmlwriter.selected.jsonl`
- Selected gate: 1 generated PHPT covering memory output, element, attribute,
  text, and document close behavior

## Runtime Contract

- `extension_loaded("xmlwriter")` returns `true`.
- `XMLWriter::openMemory()`, `startDocument()`, `startElement()`,
  `writeAttribute()`, `text()`, `endElement()`, `endDocument()`, and
  `outputMemory()` are implemented for deterministic XML output.

## Unsupported Area

| Stable ID | Reference behavior summary | Current phrust behavior | Fixture path | Next owner layer |
| --- | --- | --- | --- | --- |
| `XML-DOM-INTL-XMLWRITER-FULL-SURFACE` | PHP XMLWriter supports file/URI output, namespaces, indentation, comments, DTDs, PIs, and libxml-backed error behavior. | Only in-memory elements, attributes, text, and document output are implemented. | `tests/phpt/generated/xmlwriter/basic.phpt` | future XMLWriter state/libxml layer |

## Target Gates

- `nix develop -c just phpt-module-target MODULE=xmlwriter`
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=xmlwriter`
- `nix develop -c just verify-phpt`
