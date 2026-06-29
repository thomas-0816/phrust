# dom

- Strategy: bounded XML-backed DOM MVP
- Classification: optional, enabled for `DOMDocument`, `DOMElement`,
  `DOMNode`, and `DOMNodeList`
- Selected manifest: `tests/phpt/manifests/modules/dom.selected.jsonl`
- Selected gate: 3 generated PHPTs covering platform visibility,
  `DOMDocument::loadXML`/`saveXML`, node-list lookup, attributes, and bounded
  mutation

## Runtime Contract

- `extension_loaded("dom")` returns `true`.
- `DOMDocument`, `DOMElement`, `DOMNode`, and `DOMNodeList` exist.
- `DOMDocument::loadXML()` parses the shared strict XML tree.
- `DOMDocument::saveXML()` serializes that tree.
- `DOMDocument::createElement()` creates XML-backed element objects.
- `DOMDocument::appendChild()` sets the document root for constructed
  documents.
- `DOMDocument::getElementsByTagName()` returns a countable, iterable
  `DOMNodeList` with `length` and `item()`.
- `$document->documentElement->tagName` and `textContent` are available for the
  root element.
- `DOMElement` exposes `nodeName`, `nodeValue`, `tagName`, and `textContent`.
- `DOMElement::getAttribute()`, `setAttribute()`, and bounded `appendChild()`
  operate on that element object.

## Required PHPTs

- `tests/phpt/generated/dom/platform-checks.phpt`
- `tests/phpt/generated/dom/domdocument-basic.phpt`
- `tests/phpt/generated/dom/domdocument-node-apis.phpt`

## Unsupported Area

| Stable ID | Reference behavior summary | Current phrust behavior | Fixture path | Next owner layer |
| --- | --- | --- | --- | --- |
| `XML-DOM-INTL-DOM-NODE-MODEL` | PHP DOM exposes a large live node hierarchy with mutation, ownership, namespaces, XPath, and schema validation. | The MVP supports constructed document roots, element-local attributes/append, and countable/iterable node lists; it does not model full live node ownership or the full DOM hierarchy. | `tests/phpt/generated/dom/domdocument-node-apis.phpt` | future DOM object model |
| `XML-DOM-INTL-DOM-LIBXML-HTML` | DOM XML/HTML parsing uses libxml options, errors, files, streams, and HTML mode behavior. | Only strict in-memory XML strings are accepted. | `tests/phpt/generated/dom/domdocument-basic.phpt` | future libxml/stream integration |

## Target Gates

- `nix develop -c just phpt-module-target MODULE=dom`
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=dom`
- `nix develop -c just verify-phpt`
