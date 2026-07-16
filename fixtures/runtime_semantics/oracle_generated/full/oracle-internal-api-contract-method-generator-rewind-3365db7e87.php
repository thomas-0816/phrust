<?php
// oracle-probe: id=oracle-internal-api-contract-method-generator-rewind-3365db7e87 area=internal_api_contract kind=method symbol=Generator::rewind source=Zend/zend_generators.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-method-generator-rewind-3365db7e87 failure_category=internal_api_contract
$class = "Generator";
$member = "rewind";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
