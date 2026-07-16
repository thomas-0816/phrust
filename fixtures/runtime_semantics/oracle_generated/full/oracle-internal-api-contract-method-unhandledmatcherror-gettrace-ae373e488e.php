<?php
// oracle-probe: id=oracle-internal-api-contract-method-unhandledmatcherror-gettrace-ae373e488e area=internal_api_contract kind=method symbol=UnhandledMatchError::getTrace source=reference-php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-method-unhandledmatcherror-gettrace-ae373e488e failure_category=internal_api_contract
$class = "UnhandledMatchError";
$member = "getTrace";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
