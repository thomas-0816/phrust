<?php
// oracle-probe: id=oracle-internal-api-contract-method-exception-getprevious-44ccbaa37e area=internal_api_contract kind=method symbol=Exception::getPrevious source=Zend/zend_exceptions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-method-exception-getprevious-44ccbaa37e failure_category=internal_api_contract
$class = "Exception";
$member = "getPrevious";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
