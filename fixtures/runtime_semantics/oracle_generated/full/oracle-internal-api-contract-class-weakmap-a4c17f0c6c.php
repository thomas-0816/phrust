<?php
// oracle-probe: id=oracle-internal-api-contract-class-weakmap-a4c17f0c6c area=internal_api_contract kind=class symbol=WeakMap source=Zend/zend_weakrefs.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-class-weakmap-a4c17f0c6c failure_category=internal_api_contract
$class = "WeakMap";
$available = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
echo $available ? "available\n" : "missing\n";
