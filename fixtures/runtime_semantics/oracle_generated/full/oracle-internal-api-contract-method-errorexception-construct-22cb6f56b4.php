<?php
// oracle-probe: id=oracle-internal-api-contract-method-errorexception-construct-22cb6f56b4 area=internal_api_contract kind=method symbol=ErrorException::__construct source=Zend/zend_exceptions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-method-errorexception-construct-22cb6f56b4 failure_category=internal_api_contract
$class = "ErrorException";
$member = "__construct";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
