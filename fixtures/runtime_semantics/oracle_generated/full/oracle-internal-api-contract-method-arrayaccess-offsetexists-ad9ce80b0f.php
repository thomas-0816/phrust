<?php
// oracle-probe: id=oracle-internal-api-contract-method-arrayaccess-offsetexists-ad9ce80b0f area=internal_api_contract kind=method symbol=ArrayAccess::offsetExists source=Zend/zend_interfaces.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-method-arrayaccess-offsetexists-ad9ce80b0f failure_category=internal_api_contract
$class = "ArrayAccess";
$member = "offsetExists";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
