<?php
// oracle-probe: id=oracle-internal-api-contract-class-attribute-93faa7f4fb area=internal_api_contract kind=class symbol=Attribute source=Zend/zend_attributes.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-class-attribute-93faa7f4fb failure_category=internal_api_contract
$class = "Attribute";
$available = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
echo $available ? "available\n" : "missing\n";
