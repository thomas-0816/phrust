# soap PHPT coverage

Current focused coverage:

- `extension_loaded("soap")`, helper function visibility, and SOAP class visibility.
- `use_soap_error_handler()` request-local toggle with PHP's disabled default.
- SOAP constant registration for protocol, encoding, XSD, feature, cache, and SSL constants.
- Constructor-backed `SoapParam`, `SoapHeader`, `SoapVar`, and `SoapFault` value objects.
- `is_soap_fault()` and `SoapFault::__toString()` facade behavior.
- Local WSDL metadata parsing for target namespace, service location, and operation names.
- Non-WSDL `SoapClient::__soapCall()`/`__doRequest()` build SOAP 1.1 envelopes, post loopback
  HTTP through libcurl, store `__getLast*()` request/response state, and parse simple return
  values or `SoapFault` bodies through the libxml-backed XML stack.
- `SoapServer::fault()` and `SoapServer::handle()` produce XML-backed SOAP fault responses for
  malformed requests or currently unsupported PHP callback dispatch.

Known gaps:

- Full upstream `ext/soap` PHPT corpus is not green.
- WSDL/schema/type binding, SOAP encoding rules, typemaps, headers, and persistence behavior remain
  incomplete.
- `SoapClient` remote non-loopback HTTP is disabled unless `PHRUST_NET_TESTS=1`.
- `SoapServer` cannot invoke registered PHP callbacks yet because the bounded class helper does not
  receive a VM callback-dispatch context; it fails closed with a SOAP fault response instead.
- `SoapFault` stores the SOAP-visible fields but does not yet model the full internal `Exception`
  property layout.

Focused gate:

```bash
PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=soap
```
