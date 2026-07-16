<?php
// oracle-probe: id=oracle-internal-api-contract-interface-iterator-35965050b8 area=internal_api_contract kind=interface symbol=Iterator source=Zend/zend_interfaces.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-interface-iterator-35965050b8 failure_category=internal_api_contract
$class = "Iterator";
$available = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
echo $available ? "available\n" : "missing\n";
