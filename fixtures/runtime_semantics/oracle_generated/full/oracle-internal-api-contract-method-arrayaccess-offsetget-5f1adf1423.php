<?php
// oracle-probe: id=oracle-internal-api-contract-method-arrayaccess-offsetget-5f1adf1423 area=internal_api_contract kind=method symbol=ArrayAccess::offsetGet source=Zend/zend_interfaces.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-method-arrayaccess-offsetget-5f1adf1423 failure_category=internal_api_contract
$class = "ArrayAccess";
$member = "offsetGet";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
