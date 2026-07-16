<?php
// oracle-probe: id=oracle-internal-api-contract-method-reflectiongenerator-getexecutingline-89f63059d5 area=internal_api_contract kind=method symbol=ReflectionGenerator::getExecutingLine source=ext/reflection/php_reflection.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-method-reflectiongenerator-getexecutingline-89f63059d5 failure_category=internal_api_contract requires_ref_extension=reflection
$class = "ReflectionGenerator";
$member = "getExecutingLine";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
