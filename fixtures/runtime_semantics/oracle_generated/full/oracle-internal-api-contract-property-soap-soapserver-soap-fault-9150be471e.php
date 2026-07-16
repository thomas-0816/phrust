<?php
// oracle-probe: id=oracle-internal-api-contract-property-soap-soapserver-soap-fault-9150be471e area=internal_api_contract kind=property symbol=Soap\SoapServer::__soap_fault source=ext/soap/soap.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-property-soap-soapserver-soap-fault-9150be471e failure_category=internal_api_contract requires_ref_extension=soap
$class = "Soap\\SoapServer";
$member = "__soap_fault";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && (new ReflectionClass($class))->hasProperty($member);
echo $available ? "available\n" : "missing\n";
