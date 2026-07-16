<?php
// oracle-probe: id=oracle-internal-api-contract-method-arithmeticerror-wakeup-ba82e9bfaf area=internal_api_contract kind=method symbol=ArithmeticError::__wakeup source=reference-php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-method-arithmeticerror-wakeup-ba82e9bfaf failure_category=internal_api_contract
$class = "ArithmeticError";
$member = "__wakeup";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
