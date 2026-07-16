<?php
// oracle-probe: id=oracle-internal-api-contract-class-constant-roundingmode-towardszero-66ec47252d area=internal_api_contract kind=class_constant symbol=RoundingMode::TowardsZero source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-class-constant-roundingmode-towardszero-66ec47252d failure_category=internal_api_contract
$class = "RoundingMode";
$member = "TowardsZero";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && defined($class . '::' . $member);
echo $available ? "available\n" : "missing\n";
