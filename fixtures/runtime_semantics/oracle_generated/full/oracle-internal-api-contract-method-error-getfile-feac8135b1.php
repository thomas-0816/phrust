<?php
// oracle-probe: id=oracle-internal-api-contract-method-error-getfile-feac8135b1 area=internal_api_contract kind=method symbol=Error::getFile source=Zend/zend_exceptions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-method-error-getfile-feac8135b1 failure_category=internal_api_contract
$class = "Error";
$member = "getFile";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
