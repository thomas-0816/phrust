<?php
// oracle-probe: id=oracle-internal-api-contract-method-compileerror-gettraceasstring-26fd61aaba area=internal_api_contract kind=method symbol=CompileError::getTraceAsString source=reference-php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-method-compileerror-gettraceasstring-26fd61aaba failure_category=internal_api_contract
$class = "CompileError";
$member = "getTraceAsString";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
