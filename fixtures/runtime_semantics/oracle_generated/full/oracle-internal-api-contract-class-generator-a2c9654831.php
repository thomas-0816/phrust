<?php
// oracle-probe: id=oracle-internal-api-contract-class-generator-a2c9654831 area=internal_api_contract kind=class symbol=Generator source=Zend/zend_generators.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-class-generator-a2c9654831 failure_category=internal_api_contract
$class = "Generator";
$available = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
echo $available ? "available\n" : "missing\n";
