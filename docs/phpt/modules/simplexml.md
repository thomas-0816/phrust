# simplexml

- Strategy: bounded XML-backed SimpleXML MVP
- Classification: optional, enabled for `simplexml_load_string` and
  `SimpleXMLElement`
- Selected manifest: `tests/phpt/manifests/modules/simplexml.selected.jsonl`
- Selected gate: 3 generated PHPTs covering platform visibility, object
  access, and WordPress-style RSS/plugin/config snippets

## Runtime Contract

- `extension_loaded("simplexml")` returns `true`.
- `simplexml_load_string()` parses the shared strict XML tree.
- `SimpleXMLElement` supports text conversion, child property access,
  `attributes()`, iteration over child elements, and `asXML()`.
- The selected WordPress-style slice covers RSS title/item reads, plugin
  metadata attributes, and simple config option iteration.

## Required PHPTs

- `tests/phpt/generated/simplexml/platform-checks.phpt`
- `tests/phpt/generated/simplexml/simplexml-basic.phpt`
- `tests/phpt/generated/simplexml/wordpress-snippets.phpt`

## Unsupported Area

| Stable ID | Reference behavior summary | Current phrust behavior | Fixture path | Next owner layer |
| --- | --- | --- | --- | --- |
| `XML-DOM-INTL-SIMPLEXML-NAMESPACES-XPATH` | PHP SimpleXML supports namespace-aware access, XPath, file loading, DOM import, and iterator variants. | Only in-memory XML strings, direct child access, attributes, text, iteration, and serialization are implemented. | `tests/phpt/generated/simplexml/simplexml-basic.phpt` | future SimpleXML/DOM integration |
| `XML-DOM-INTL-SIMPLEXML-LIBXML-ERRORS` | SimpleXML participates in libxml error handling and parse option behavior. | The shared strict parser rejects unsupported constructs without libxml error state. | `tests/phpt/generated/simplexml/simplexml-basic.phpt` | future libxml compatibility layer |

## Target Gates

- `nix develop -c just phpt-module-target MODULE=simplexml`
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=simplexml`
- `nix develop -c just verify-phpt`
