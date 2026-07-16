<?php
// oracle-probe: id=oracle-internal-api-contract-property-exception-string-a07415871e area=internal_api_contract kind=property symbol=Exception::string source=Zend/zend_exceptions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-property-exception-string-a07415871e failure_category=internal_api_contract
$class = "Exception";
$member = "string";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && (new ReflectionClass($class))->hasProperty($member);
echo $available ? "available\n" : "missing\n";
