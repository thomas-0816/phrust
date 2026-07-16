<?php
// oracle-probe: id=oracle-internal-api-contract-class-errorexception-fca7d2caad area=internal_api_contract kind=class symbol=ErrorException source=Zend/zend_exceptions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-class-errorexception-fca7d2caad failure_category=internal_api_contract
$class = "ErrorException";
$available = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
echo $available ? "available\n" : "missing\n";
