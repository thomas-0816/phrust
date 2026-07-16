<?php
// oracle-probe: id=oracle-internal-api-contract-method-error-gettraceasstring-2a3fd35c0d area=internal_api_contract kind=method symbol=Error::getTraceAsString source=Zend/zend_exceptions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-method-error-gettraceasstring-2a3fd35c0d failure_category=internal_api_contract
$class = "Error";
$member = "getTraceAsString";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
