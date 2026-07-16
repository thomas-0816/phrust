<?php
// oracle-probe: id=oracle-internal-api-contract-class-arithmeticerror-0419fcda1b area=internal_api_contract kind=class symbol=ArithmeticError source=Zend/zend_exceptions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-class-arithmeticerror-0419fcda1b failure_category=internal_api_contract
$class = "ArithmeticError";
$available = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
echo $available ? "available\n" : "missing\n";
