<?php
// oracle-probe: id=oracle-internal-api-contract-class-delayedtargetvalidation-cf9152674c area=internal_api_contract kind=class symbol=DelayedTargetValidation source=Zend/zend_attributes.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-class-delayedtargetvalidation-cf9152674c failure_category=internal_api_contract
$class = "DelayedTargetValidation";
$available = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
echo $available ? "available\n" : "missing\n";
