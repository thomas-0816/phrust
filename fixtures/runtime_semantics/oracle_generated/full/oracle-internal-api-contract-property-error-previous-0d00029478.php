<?php
// oracle-probe: id=oracle-internal-api-contract-property-error-previous-0d00029478 area=internal_api_contract kind=property symbol=Error::previous source=Zend/zend_exceptions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-property-error-previous-0d00029478 failure_category=internal_api_contract
$class = "Error";
$member = "previous";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && (new ReflectionClass($class))->hasProperty($member);
echo $available ? "available\n" : "missing\n";
