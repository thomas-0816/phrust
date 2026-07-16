<?php
// oracle-probe: id=oracle-internal-api-contract-property-exception-message-e64b7493ba area=internal_api_contract kind=property symbol=Exception::message source=Zend/zend_exceptions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-property-exception-message-e64b7493ba failure_category=internal_api_contract
$class = "Exception";
$member = "message";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && (new ReflectionClass($class))->hasProperty($member);
echo $available ? "available\n" : "missing\n";
