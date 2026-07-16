<?php
// oracle-probe: id=oracle-internal-api-contract-interface-serializable-f454dbf7be area=internal_api_contract kind=interface symbol=Serializable source=Zend/zend_interfaces.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-interface-serializable-f454dbf7be failure_category=internal_api_contract
$class = "Serializable";
$available = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
echo $available ? "available\n" : "missing\n";
