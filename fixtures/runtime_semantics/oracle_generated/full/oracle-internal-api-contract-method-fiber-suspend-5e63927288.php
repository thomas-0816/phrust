<?php
// oracle-probe: id=oracle-internal-api-contract-method-fiber-suspend-5e63927288 area=internal_api_contract kind=method symbol=Fiber::suspend source=Zend/zend_fibers.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-method-fiber-suspend-5e63927288 failure_category=internal_api_contract
$class = "Fiber";
$member = "suspend";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
