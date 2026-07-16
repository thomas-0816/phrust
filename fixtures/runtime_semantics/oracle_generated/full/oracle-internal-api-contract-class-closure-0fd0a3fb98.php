<?php
// oracle-probe: id=oracle-internal-api-contract-class-closure-0fd0a3fb98 area=internal_api_contract kind=class symbol=Closure source=Zend/zend_closures.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-class-closure-0fd0a3fb98 failure_category=internal_api_contract
$class = "Closure";
$available = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
echo $available ? "available\n" : "missing\n";
