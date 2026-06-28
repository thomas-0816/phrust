# ext-xml-soap Merge Notes

Branch scope: XML-family PHPT policy and harnessing for `dom`, `xml`,
`simplexml`, `xsl`, and `soap`.

## Policy Table

| Extension | Corpus count | Selected count | Focus status | Policy | Next action |
| --- | ---: | ---: | --- | --- | --- |
| dom | 879 | 1 | platform checks only | optional | Defer real DOM/XML object model work. |
| xml | 65 | 1 | platform checks only | optional | Defer SAX parser work until dependency strategy is approved. |
| simplexml | 157 | 1 | platform checks only | optional | Defer until XML parser and DOM object views exist. |
| xsl | 72 | 1 | platform checks only | optional | Defer until DOM/XML plus libxslt strategy exists. |
| soap | 589 | 1 | platform checks only | out-of-scope | Keep out of scope until XML, streams, HTTP, and schema support exist. |

## Stub Decision

No runtime XML-family stubs were added.

The selected platform PHPTs prove the current contract: `extension_loaded()`,
representative `class_exists()`, selected `function_exists()`, and selected
`defined()` probes return false for XML-family surfaces. This is deliberate and
avoids fake successful XML parsing.

## Generated PHPTs

- `tests/phpt/generated/dom/platform-checks.phpt`
- `tests/phpt/generated/xml/platform-checks.phpt`
- `tests/phpt/generated/simplexml/platform-checks.phpt`
- `tests/phpt/generated/xsl/platform-checks.phpt`
- `tests/phpt/generated/soap/platform-checks.phpt`

## Gate Results

All module gates used:

- `PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src`
- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php`
- `PHPT_REUSE_LAST=0`
- `PHPT_DEV_REUSE_TARGET_PASS=0`

| Command | Reference status | Target status |
| --- | --- | --- |
| `nix develop -c just phpt-dev-module MODULE=dom` | PASS, 1 run, 0 non-green | PASS, 1 run, 0 non-green |
| `nix develop -c just phpt-dev-module MODULE=xml` | PASS, 1 run, 0 non-green | PASS, 1 run, 0 non-green |
| `nix develop -c just phpt-dev-module MODULE=simplexml` | PASS, 1 run, 0 non-green | PASS, 1 run, 0 non-green |
| `nix develop -c just phpt-dev-module MODULE=xsl` | PASS, 1 run, 0 non-green | PASS, 1 run, 0 non-green |
| `nix develop -c just phpt-dev-module MODULE=soap` | PASS, 1 run, 0 non-green | PASS, 1 run, 0 non-green |

Closeout gates:

- `nix develop -c cargo test -p php_runtime`: PASS, 185 tests.
- `nix develop -c just verify-phpt`: PASS.

## Unsupported Areas

| Stable ID | Reference behavior summary | Current phrust behavior | Fixture | Next owner layer |
| --- | --- | --- | --- | --- |
| `XML-FAMILY-DOM-REAL-IMPLEMENTATION` | `ext/dom` exposes a libxml-backed DOM class hierarchy and upstream DOM corpus behavior. | DOM extension/classes are not registered. | `tests/phpt/generated/dom/platform-checks.phpt` | Future `php_std`, `php_runtime`, and `php_vm` DOM/XML integration. |
| `XML-FAMILY-XML-SAX-PARSER` | `ext/xml` exposes parser constants, `XMLParser`, and parser/callback functions. | XML extension/classes/functions/constants are not registered. | `tests/phpt/generated/xml/platform-checks.phpt` | Future parser dependency and runtime parser object/resource support. |
| `XML-FAMILY-SIMPLEXML-REAL-IMPLEMENTATION` | `ext/simplexml` exposes XML object views and DOM import helpers. | SimpleXML extension/classes/functions are not registered. | `tests/phpt/generated/simplexml/platform-checks.phpt` | Future DOM/XML parser plus runtime object/iterator integration. |
| `XML-FAMILY-XSL-REAL-IMPLEMENTATION` | `ext/xsl` exposes `XSLTProcessor` and libxslt constants/transform behavior. | XSL extension/classes/constants are not registered. | `tests/phpt/generated/xsl/platform-checks.phpt` | Future DOM/XML plus libxslt strategy. |
| `XML-FAMILY-SOAP-OUT-OF-SCOPE` | `ext/soap` exposes SOAP client/server/fault/header behavior over WSDL/XML/HTTP. | SOAP extension/classes are not registered. | `tests/phpt/generated/soap/platform-checks.phpt` | No current owner; requires XML, streams, HTTP, and schema support first. |

## Merge Risks

- These artifacts intentionally do not reduce the upstream XML-family corpus
  failures; they make the current policy explicit and focused.
- Future DOM/XML implementation work must not treat these negative platform
  fixtures as a permanent contract. When a real extension is enabled, update the
  selected manifests and module docs in the same change.
