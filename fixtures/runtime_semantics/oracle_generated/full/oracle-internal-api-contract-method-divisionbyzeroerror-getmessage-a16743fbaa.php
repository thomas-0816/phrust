<?php
// oracle-probe: id=oracle-internal-api-contract-method-divisionbyzeroerror-getmessage-a16743fbaa area=internal_api_contract kind=method symbol=DivisionByZeroError::getMessage source=reference-php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-method-divisionbyzeroerror-getmessage-a16743fbaa failure_category=internal_api_contract
$class = "DivisionByZeroError";
$member = "getMessage";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
