<?php
// oracle-probe: id=oracle-internal-api-contract-property-divisionbyzeroerror-message-2ed003d9d3 area=internal_api_contract kind=property symbol=DivisionByZeroError::message source=reference-php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-property-divisionbyzeroerror-message-2ed003d9d3 failure_category=internal_api_contract
$class = "DivisionByZeroError";
$member = "message";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && (new ReflectionClass($class))->hasProperty($member);
echo $available ? "available\n" : "missing\n";
