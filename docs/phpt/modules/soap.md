# soap PHPT coverage

Current focused coverage:

- `extension_loaded("soap")`, helper function visibility, and SOAP class visibility.
- `use_soap_error_handler()` request-local toggle with PHP's disabled default.
- SOAP constant registration for protocol, encoding, XSD, feature, cache, and SSL constants.
- Constructor-backed `SoapParam`, `SoapHeader`, `SoapVar`, and `SoapFault` value objects.
- `is_soap_fault()` and `SoapFault::__toString()` facade behavior.

Known gaps:

- Full upstream `ext/soap` PHPT corpus is not green.
- WSDL parsing, XML serialization/deserialization, SOAP encoding rules, HTTP/curl transport,
  typemaps, headers, and local server dispatch remain incomplete.
- `SoapClient` and `SoapServer` only expose bounded state helpers; transport and dispatch methods
  fail explicitly until the dependent XML/curl/streams layers are complete.
- `SoapFault` stores the SOAP-visible fields but does not yet model the full internal `Exception`
  property layout.

Focused gate:

```bash
PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=soap
```
