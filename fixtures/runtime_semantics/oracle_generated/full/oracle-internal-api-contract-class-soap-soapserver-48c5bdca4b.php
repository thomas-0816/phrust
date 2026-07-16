<?php
// oracle-probe: id=oracle-internal-api-contract-class-soap-soapserver-48c5bdca4b area=internal_api_contract kind=class symbol=Soap\SoapServer source=ext/soap/soap.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-class-soap-soapserver-48c5bdca4b failure_category=internal_api_contract requires_ref_extension=soap
$class = "Soap\\SoapServer";
$available = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
echo $available ? "available\n" : "missing\n";
