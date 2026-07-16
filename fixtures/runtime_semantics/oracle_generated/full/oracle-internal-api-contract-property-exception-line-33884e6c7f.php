<?php
// oracle-probe: id=oracle-internal-api-contract-property-exception-line-33884e6c7f area=internal_api_contract kind=property symbol=Exception::line source=Zend/zend_exceptions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-property-exception-line-33884e6c7f failure_category=internal_api_contract
$class = "Exception";
$member = "line";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && (new ReflectionClass($class))->hasProperty($member);
echo $available ? "available\n" : "missing\n";
