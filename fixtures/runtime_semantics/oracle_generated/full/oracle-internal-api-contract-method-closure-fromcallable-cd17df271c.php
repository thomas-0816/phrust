<?php
// oracle-probe: id=oracle-internal-api-contract-method-closure-fromcallable-cd17df271c area=internal_api_contract kind=method symbol=Closure::fromCallable source=Zend/zend_closures.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-method-closure-fromcallable-cd17df271c failure_category=internal_api_contract
$class = "Closure";
$member = "fromCallable";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
