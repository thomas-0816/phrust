<?php
// oracle-probe: id=oracle-internal-api-contract-class-constant-attribute-target-function-8c130880bb area=internal_api_contract kind=class_constant symbol=Attribute::TARGET_FUNCTION source=Zend/zend_attributes.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-class-constant-attribute-target-function-8c130880bb failure_category=internal_api_contract
$class = "Attribute";
$member = "TARGET_FUNCTION";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && defined($class . '::' . $member);
echo $available ? "available\n" : "missing\n";
