<?php
// oracle-probe: id=oracle-internal-api-contract-method-weakreference-get-fd8c4be1b6 area=internal_api_contract kind=method symbol=WeakReference::get source=Zend/zend_weakrefs.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-method-weakreference-get-fd8c4be1b6 failure_category=internal_api_contract
$class = "WeakReference";
$member = "get";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
