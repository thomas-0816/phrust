<?php
// oracle-probe: id=oracle-internal-api-contract-method-fiber-isstarted-389641bc1b area=internal_api_contract kind=method symbol=Fiber::isStarted source=Zend/zend_fibers.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-method-fiber-isstarted-389641bc1b failure_category=internal_api_contract
$class = "Fiber";
$member = "isStarted";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
