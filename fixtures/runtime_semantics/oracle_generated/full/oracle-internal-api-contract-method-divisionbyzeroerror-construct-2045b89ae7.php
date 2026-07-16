<?php
// oracle-probe: id=oracle-internal-api-contract-method-divisionbyzeroerror-construct-2045b89ae7 area=internal_api_contract kind=method symbol=DivisionByZeroError::__construct source=reference-php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-method-divisionbyzeroerror-construct-2045b89ae7 failure_category=internal_api_contract
$class = "DivisionByZeroError";
$member = "__construct";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
