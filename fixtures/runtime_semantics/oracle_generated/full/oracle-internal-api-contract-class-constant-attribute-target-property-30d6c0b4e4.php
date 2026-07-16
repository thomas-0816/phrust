<?php
// oracle-probe: id=oracle-internal-api-contract-class-constant-attribute-target-property-30d6c0b4e4 area=internal_api_contract kind=class_constant symbol=Attribute::TARGET_PROPERTY source=Zend/zend_attributes.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-class-constant-attribute-target-property-30d6c0b4e4 failure_category=internal_api_contract
$class = "Attribute";
$member = "TARGET_PROPERTY";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && defined($class . '::' . $member);
echo $available ? "available\n" : "missing\n";
