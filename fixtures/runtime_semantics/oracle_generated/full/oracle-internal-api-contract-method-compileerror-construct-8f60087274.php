<?php
// oracle-probe: id=oracle-internal-api-contract-method-compileerror-construct-8f60087274 area=internal_api_contract kind=method symbol=CompileError::__construct source=reference-php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-method-compileerror-construct-8f60087274 failure_category=internal_api_contract
$class = "CompileError";
$member = "__construct";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
