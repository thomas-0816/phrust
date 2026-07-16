<?php
// oracle-probe: id=oracle-internal-api-contract-class-stdclass-7323570caf area=internal_api_contract kind=class symbol=stdClass source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-class-stdclass-7323570caf failure_category=internal_api_contract
$class = "stdClass";
$available = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
echo $available ? "available\n" : "missing\n";
