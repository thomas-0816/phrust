<?php
// oracle-probe: id=oracle-internal-api-contract-property-attribute-flags-35b68d1f4b area=internal_api_contract kind=property symbol=Attribute::flags source=Zend/zend_attributes.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-property-attribute-flags-35b68d1f4b failure_category=internal_api_contract
$class = "Attribute";
$member = "flags";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && (new ReflectionClass($class))->hasProperty($member);
echo $available ? "available\n" : "missing\n";
