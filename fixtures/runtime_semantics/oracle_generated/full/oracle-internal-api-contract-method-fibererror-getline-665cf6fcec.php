<?php
// oracle-probe: id=oracle-internal-api-contract-method-fibererror-getline-665cf6fcec area=internal_api_contract kind=method symbol=FiberError::getLine source=reference-php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-method-fibererror-getline-665cf6fcec failure_category=internal_api_contract
$class = "FiberError";
$member = "getLine";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
